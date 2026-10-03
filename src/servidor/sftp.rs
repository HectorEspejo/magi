//! Canal SFTP por host (F4). Todo lo remoto de la vista Archivos pasa por
//! aquí: el servidor abre **un** canal por host sobre la conexión del pool (o
//! sobre una nueva, con los diálogos de siempre reenviados al solicitante) y
//! lo comparte entre todas las ventanas.
//!
//! El canal no es una sesión: no aparece en pestañas ni cuenta para la
//! difusión `Sesiones`. Se cierra solo tras diez minutos sin actividad y sin
//! transferencias, y el siguiente `AbrirSftp` lo vuelve a abrir.
//!
//! Desde la Fase 8, al abrirlo se abre también un segundo canal «raw» para
//! `posix-rename@openssh.com` (sobrescribir) y se leen `/etc/passwd` y
//! `/etc/group` del host para los nombres de propietario; los dos viven y
//! mueren con el canal y ninguno lo hace fallar.
//!
//! Ninguna ruta pasa por un shell: todo va por SFTP.

use std::collections::HashMap;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use russh_sftp::client::{RawSftpSession, SftpSession};
use russh_sftp::protocol::{FileAttributes, OpenFlags, Packet, StatusCode};
use tracing::{info, warn};

use crate::archivos::marcas::{ordenar, Entrada, TipoEntrada};
use crate::modelo::Host;
use crate::protocolo::EntradaArbol;

use super::conexiones::{self, ConexionTomada, PoliticaConexion, UsoCanal};
use super::EstadoServidor;

/// Tiempo sin actividad tras el cual se cierra el canal de un host.
pub const INACTIVIDAD: Duration = Duration::from_secs(10 * 60);

/// Cada cuánto se revisan los canales inactivos.
const REVISION: Duration = Duration::from_secs(30);

/// Tamaño de bloque de las transferencias y de las copias temporales.
pub const BLOQUE: usize = 64 * 1024;

/// Plazo del saludo del subsistema SFTP: si el host no lo sirve, no contesta.
/// Cubre también abrir el canal y pedir el subsistema.
const HANDSHAKE_SFTP: Duration = Duration::from_secs(10);

/// Nombre del directorio de temporales dentro del directorio de ejecución.
const SUBDIR_TEMPORALES: &str = "tmp";

/// Directorio de los temporales de edición (Fase 8): nadie lo vacía; cada
/// temporal se borra solo cuando el cliente lo pide (T55).
const SUBDIR_EDICIONES: &str = "ediciones";

/// Sufijo de los ficheros que aún se están escribiendo.
pub const SUFIJO_PARCIAL: &str = ".magi-parcial";

/// Para qué se abre un canal SFTP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModoSftp {
    /// La vista Archivos (o una transferencia): con diálogos al solicitante
    /// y cuenta para el ciclo automático de los túneles.
    Archivos,
    /// Una comprobación de la deliberación MAGI (BALTHASAR-2): sin diálogos
    /// y sin levantar túneles automáticos (mirar un directorio no es usar el
    /// host).
    Comprobacion,
}

/// Un canal SFTP abierto, compartido por todas las ventanas.
pub struct SftpHost {
    pub host_id: i64,
    pub host_nombre: String,
    pub sesion: Arc<SftpSession>,
    /// Conexión sobre la que vive el canal (la del pool): se suelta por
    /// identidad al cerrarlo.
    pub conexion: ConexionTomada,
    pub dir_inicio: String,
    pub solicitante: u32,
    pub ultima_actividad: Instant,
    /// Lo ha pedido la vista Archivos (o una transferencia): cuenta para el
    /// ciclo automático de los túneles. Uno abierto solo para comprobar un
    /// backup no cuenta hasta que Archivos lo use.
    pub para_archivos: bool,
    /// Cómo se sustituye un fichero que ya existe en el remoto (Fase 8).
    pub renombrador: Arc<Renombrador>,
    /// Nombres de usuarios y grupos del host, mientras el canal vive (§7.5).
    pub nombres: Arc<MapaNombres>,
    /// Usuario con el que autenticó la conexión y su uid en el host (del
    /// `/etc/passwd` remoto o, si no está, el dueño de `dir_inicio`).
    pub usuario_conexion: Option<String>,
    pub uid_conexion: Option<u32>,
}

/// Extensión de OpenSSH que renombra con `rename(2)`: sustituye el destino si
/// existe, de forma atómica.
const POSIX_RENAME: &str = "posix-rename@openssh.com";

/// Sufijo con el que se aparta el original mientras se sustituye en tres
/// pasos (hosts sin `posix-rename@openssh.com`).
pub const SUFIJO_VIEJO: &str = ".magi-viejo";

/// Plazo de un renombrado por el canal raw.
const PLAZO_RENOMBRAR: Duration = Duration::from_secs(10);

/// Sustituye un fichero remoto por el parcial recién escrito (Fase 8). El
/// `rename` de SFTP v3 de OpenSSH es `link()`+`unlink()` y falla si el destino
/// existe: con `posix-rename@openssh.com` (por un canal raw propio, que
/// `SftpSession` no expone) el cambio es atómico; sin la extensión se hace en
/// tres pasos con deshacer, sin perder nunca el original.
pub struct Renombrador {
    /// Segundo canal SFTP del host, solo si anuncia la extensión. Vive y se
    /// cierra con el canal principal.
    raw: Option<RawSftpSession>,
}

impl Renombrador {
    /// Sin canal raw: solo el renombrado en tres pasos.
    pub fn sin_extension() -> Self {
        Self { raw: None }
    }

    /// Abre el segundo canal sobre la conexión del canal principal, con los
    /// mismos plazos que este. Nunca falla: si algo sale mal (o el host no
    /// anuncia la extensión) queda el renombrado en tres pasos, y el canal
    /// principal sigue igual.
    pub async fn abrir(
        handle: Arc<russh::client::Handle<crate::conexion::cliente::Cliente>>,
    ) -> Self {
        let canal = match conexiones::abrir_con_plazo(HANDSHAKE_SFTP, async move {
            handle.channel_open_session().await
        })
        .await
        {
            Ok(canal) => canal,
            Err(motivo) => {
                warn!("sin canal para {POSIX_RENAME}: {motivo}");
                return Self::sin_extension();
            }
        };
        match tokio::time::timeout(HANDSHAKE_SFTP, canal.request_subsystem(true, "sftp")).await {
            Ok(Ok(())) => {}
            _ => {
                let _ = canal.close().await;
                warn!("sin canal para {POSIX_RENAME}: el host no dio el subsistema");
                return Self::sin_extension();
            }
        }
        // Desde aquí el canal va dentro del flujo, que lo cierra al soltarse.
        Self::sobre_flujo(canal.into_stream()).await
    }

    /// Saluda al subsistema por un flujo ya abierto y se queda con él solo si
    /// el host anuncia la extensión (con el saludo, que lleva plazo: un host
    /// que no sirve el subsistema no contesta). Separado de `abrir` para
    /// poder probarlo sobre un `sftp-server` local.
    #[doc(hidden)]
    pub async fn sobre_flujo<S>(flujo: S) -> Self
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let raw = RawSftpSession::new(flujo);
        match tokio::time::timeout(HANDSHAKE_SFTP, raw.init()).await {
            Ok(Ok(version))
                if version
                    .extensions
                    .get(POSIX_RENAME)
                    .is_some_and(|v| v == "1") =>
            {
                Self { raw: Some(raw) }
            }
            _ => {
                // Soltar el raw cierra su flujo (y con él el canal).
                Self::sin_extension()
            }
        }
    }

    /// ¿Renombra con `posix-rename@openssh.com`?
    pub fn con_extension(&self) -> bool {
        self.raw.is_some()
    }

    /// Cierra el canal raw. No espera a la red: solo corta el flujo, así que
    /// no necesita plazo.
    pub fn cerrar(&self) {
        if let Some(raw) = &self.raw {
            let _ = raw.close_session();
        }
    }

    /// Renombra `de` sobre `a`, exista `a` o no.
    pub async fn renombrar_sobre(
        &self,
        sesion: &SftpSession,
        de: &str,
        a: &str,
    ) -> Result<(), String> {
        match &self.raw {
            Some(raw) => renombrar_posix(raw, de, a).await,
            None => renombrar_en_tres_pasos(sesion, de, a).await,
        }
    }
}

/// `posix-rename@openssh.com`: los datos son las dos rutas como cadenas SSH y
/// la respuesta, un `Status`.
async fn renombrar_posix(raw: &RawSftpSession, de: &str, a: &str) -> Result<(), String> {
    let datos = cadenas_ssh(&[de, a]);
    match tokio::time::timeout(PLAZO_RENOMBRAR, raw.extended(POSIX_RENAME, datos)).await {
        Ok(Ok(Packet::Status(estado))) if estado.status_code == StatusCode::Ok => Ok(()),
        Ok(Ok(Packet::Status(estado))) => Err(describir(format!(
            "{}: {}",
            estado.status_code, estado.error_message
        ))),
        Ok(Ok(_)) => Err("el host contestó al renombrado con algo inesperado".to_string()),
        Ok(Err(error)) => Err(describir(error.to_string())),
        Err(_) => Err("el host dejó de responder al renombrar".to_string()),
    }
}

/// Cadenas del protocolo SSH: longitud en u32 big-endian y los bytes.
fn cadenas_ssh(cadenas: &[&str]) -> Vec<u8> {
    let mut datos = Vec::new();
    for cadena in cadenas {
        datos.extend_from_slice(&(cadena.len() as u32).to_be_bytes());
        datos.extend_from_slice(cadena.as_bytes());
    }
    datos
}

/// Sustitución sin la extensión: si `a` no existe, un `rename` normal; si
/// existe, se aparta como `a.magi-viejo`, se pone `de` en su sitio y se borra
/// el viejo. Si el segundo paso falla, el viejo vuelve a su sitio: el original
/// nunca se pierde. Un directorio nunca se sustituye por un fichero.
async fn renombrar_en_tres_pasos(sesion: &SftpSession, de: &str, a: &str) -> Result<(), String> {
    match sesion.symlink_metadata(a).await {
        Ok(metadata) if metadata.is_dir() => {
            return Err(format!("{a}: en el destino hay un directorio"));
        }
        Ok(_) => {}
        Err(error) => {
            let motivo = describir(error.to_string());
            if motivo == "la ruta no existe" {
                return renombrar(sesion, de, a).await;
            }
            return Err(motivo);
        }
    }
    let viejo = format!("{a}{SUFIJO_VIEJO}");
    // Un viejo de un intento anterior interrumpido: se quita solo si existe
    // (el nombre lleva nuestro sufijo) y nunca si es un directorio.
    if existe(sesion, &viejo).await {
        sesion
            .remove_file(&viejo)
            .await
            .map_err(|error| format!("{viejo}: {}", describir(error.to_string())))?;
    }
    renombrar(sesion, a, &viejo).await?;
    if let Err(motivo) = renombrar(sesion, de, a).await {
        if let Err(otro) = renombrar(sesion, &viejo, a).await {
            warn!("no se pudo devolver {viejo} a {a}: {otro}");
            return Err(format!("{motivo}; el original quedó en {viejo}"));
        }
        return Err(motivo);
    }
    if let Err(error) = sesion.remove_file(&viejo).await {
        warn!(
            "no se pudo borrar {viejo} tras sustituir {a}: {}",
            describir(error.to_string())
        );
    }
    Ok(())
}

// ---------------------------------------------------------------- nombres

/// Tope de `/etc/passwd` y `/etc/group` (cada uno).
const TOPE_NOMBRES: usize = 1024 * 1024;

/// Plazo total para leer los dos.
const PLAZO_NOMBRES: Duration = Duration::from_secs(2);

/// Nombres de usuarios y grupos del host, leídos de su `/etc/passwd` y su
/// `/etc/group` por SFTP al abrir el canal (§7.5), sin ejecutar nada (D99).
/// Los ids que no están (LDAP, SSSD) se quedan con el número (R46).
#[derive(Debug, Default)]
pub struct MapaNombres {
    usuarios: HashMap<u32, String>,
    grupos: HashMap<u32, String>,
    uids: HashMap<String, u32>,
}

impl MapaNombres {
    /// Construye el mapa con el texto de los dos ficheros (`nombre:x:id:…`).
    /// Si un id se repite, gana el primero, como en `getpwuid`.
    pub fn desde_textos(passwd: &str, group: &str) -> Self {
        let mut mapa = Self::default();
        for (nombre, uid) in pares_nombre_id(passwd) {
            mapa.uids.entry(nombre.clone()).or_insert(uid);
            mapa.usuarios.entry(uid).or_insert(nombre);
        }
        for (nombre, gid) in pares_nombre_id(group) {
            mapa.grupos.entry(gid).or_insert(nombre);
        }
        mapa
    }

    pub fn usuario(&self, uid: u32) -> Option<&str> {
        self.usuarios.get(&uid).map(String::as_str)
    }

    pub fn grupo(&self, gid: u32) -> Option<&str> {
        self.grupos.get(&gid).map(String::as_str)
    }

    pub fn uid_de(&self, usuario: &str) -> Option<u32> {
        self.uids.get(usuario).copied()
    }

    pub fn usuarios(&self) -> usize {
        self.usuarios.len()
    }

    pub fn grupos(&self) -> usize {
        self.grupos.len()
    }

    /// Pone los nombres que el mapa conoce (sin pisar los que ya vengan).
    pub fn nombrar(
        &self,
        propietario: Option<crate::archivos::Propietario>,
    ) -> Option<crate::archivos::Propietario> {
        let mut propietario = propietario?;
        if propietario.usuario.is_none() {
            propietario.usuario = propietario
                .uid
                .and_then(|uid| self.usuario(uid))
                .map(str::to_string);
        }
        if propietario.grupo.is_none() {
            propietario.grupo = propietario
                .gid
                .and_then(|gid| self.grupo(gid))
                .map(str::to_string);
        }
        Some(propietario)
    }

    /// Como `nombrar`, para un listado entero.
    pub fn nombrar_entradas(&self, entradas: &mut [Entrada]) {
        for entrada in entradas {
            entrada.propietario = self.nombrar(entrada.propietario.take());
        }
    }
}

/// Pares `(nombre, id)` de un fichero con el formato de `/etc/passwd`. Se
/// saltan comentarios, líneas de NIS (`+`/`-`), ids que no son números y
/// nombres raros (vacíos, con caracteres de control o demasiado largos), que
/// acabarían pintados en la interfaz.
fn pares_nombre_id(texto: &str) -> impl Iterator<Item = (String, u32)> + '_ {
    texto.lines().filter_map(|linea| {
        let mut campos = linea.split(':');
        let nombre = campos.next()?.trim();
        let _clave = campos.next()?;
        let id = campos.next()?.trim().parse::<u32>().ok()?;
        let raro = nombre.is_empty()
            || nombre.len() > 64
            || nombre.starts_with(['#', '+', '-'])
            || nombre.chars().any(char::is_control);
        (!raro).then(|| (nombre.to_string(), id))
    })
}

/// Lee `/etc/passwd` y `/etc/group` del host, a la vez y con un plazo total
/// (la apertura del canal espera por ellos). Lo que no se pueda leer a tiempo
/// se queda sin nombres: se sigue con números.
async fn leer_nombres(sesion: &SftpSession) -> MapaNombres {
    let limite = tokio::time::Instant::now() + PLAZO_NOMBRES;
    let (passwd, group) = tokio::join!(
        leer_tabla(sesion, "/etc/passwd", limite),
        leer_tabla(sesion, "/etc/group", limite)
    );
    MapaNombres::desde_textos(
        passwd.as_deref().unwrap_or_default(),
        group.as_deref().unwrap_or_default(),
    )
}

/// Una tabla del sistema con tope: si pasa de 1 MiB solo valen sus líneas
/// completas.
async fn leer_tabla(
    sesion: &SftpSession,
    ruta: &str,
    limite: tokio::time::Instant,
) -> Option<String> {
    match tokio::time::timeout_at(limite, leer_con_tope(sesion, ruta, TOPE_NOMBRES)).await {
        Ok(Ok(Some((datos, completo)))) => {
            let mut texto = String::from_utf8_lossy(&datos).into_owned();
            if !completo {
                let hasta = texto.rfind('\n').map_or(0, |posicion| posicion + 1);
                texto.truncate(hasta);
            }
            Some(texto)
        }
        Ok(Ok(None)) => None,
        Ok(Err(motivo)) => {
            warn!("no se pudo leer {ruta} del host: {motivo}");
            None
        }
        Err(_) => {
            warn!("{ruta} del host no llegó a tiempo: se sigue con números");
            None
        }
    }
}

/// Lee como mucho `tope` bytes de un fichero remoto. `None` si no existe; con
/// los datos, si cabía entero.
async fn leer_con_tope(
    sesion: &SftpSession,
    ruta: &str,
    tope: usize,
) -> Result<Option<(Vec<u8>, bool)>, String> {
    use tokio::io::AsyncReadExt as _;
    let fichero = match sesion.open(ruta).await {
        Ok(fichero) => fichero,
        Err(error) => {
            let motivo = describir(error.to_string());
            if motivo == "la ruta no existe" {
                return Ok(None);
            }
            return Err(motivo);
        }
    };
    let mut datos = Vec::new();
    fichero
        .take(tope as u64 + 1)
        .read_to_end(&mut datos)
        .await
        .map_err(|error| describir(error.to_string()))?;
    let completo = datos.len() <= tope;
    datos.truncate(tope);
    Ok(Some((datos, completo)))
}

/// Mapa de nombres del canal del host; vacío si no hay canal.
pub async fn nombres(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    host_id: i64,
) -> Arc<MapaNombres> {
    estado
        .lock()
        .await
        .sftp
        .get(&host_id)
        .map(|canal| canal.nombres.clone())
        .unwrap_or_default()
}

/// Usuario de la conexión del canal del host y su uid, para `SftpAbierto`.
pub async fn datos_conexion(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    host_id: i64,
) -> (Option<String>, Option<u32>) {
    estado
        .lock()
        .await
        .sftp
        .get(&host_id)
        .map(|canal| (canal.usuario_conexion.clone(), canal.uid_conexion))
        .unwrap_or_default()
}

/// El canal puede cerrarse si nadie lo ha usado en la ventana de inactividad
/// y no le quedan transferencias. Función pura, para poder probarla.
pub fn puede_cerrarse(ultima_actividad: Instant, ahora: Instant, hay_transferencias: bool) -> bool {
    !hay_transferencias && ahora.duration_since(ultima_actividad) >= INACTIVIDAD
}

/// Directorio de temporales: `$XDG_RUNTIME_DIR/magi/tmp`, con permisos 700.
pub fn dir_temporales(rutas: &crate::config::Rutas) -> PathBuf {
    rutas.dir_runtime().join(SUBDIR_TEMPORALES)
}

/// Directorio de los temporales de edición: `$XDG_RUNTIME_DIR/magi/ediciones`,
/// con permisos 700.
pub fn dir_ediciones(rutas: &crate::config::Rutas) -> PathBuf {
    rutas.dir_runtime().join(SUBDIR_EDICIONES)
}

/// Crea el directorio de temporales (700). No borra nada: vaciarlo en cada
/// descarga mataba los temporales que otra ventana aún estaba usando.
pub fn preparar_temporales(rutas: &crate::config::Rutas) {
    if let Err(motivo) = crear_dir_privado(&dir_temporales(rutas)) {
        warn!("{motivo}");
    }
}

/// Crea (si falta) un directorio solo para el usuario: 700.
fn crear_dir_privado(directorio: &Path) -> Result<(), String> {
    std::fs::create_dir_all(directorio)
        .map_err(|error| format!("no se pudo crear {}: {error}", directorio.display()))?;
    std::fs::set_permissions(
        directorio,
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )
    .map_err(|error| format!("no se pudo proteger {}: {error}", directorio.display()))
}

/// Vacía el directorio de temporales (al arrancar y al apagarse). Nunca toca
/// el de ediciones: un temporal con cambios sin subir solo se borra cuando el
/// cliente lo pide (T55).
pub fn vaciar_temporales(rutas: &crate::config::Rutas) {
    let directorio = dir_temporales(rutas);
    let Ok(lectura) = std::fs::read_dir(&directorio) else {
        return;
    };
    for elemento in lectura.flatten() {
        let _ = std::fs::remove_file(elemento.path());
    }
}

// ---------------------------------------------------------------- apertura

/// Devuelve el canal del host (y su directorio de inicio), abriéndolo si hace
/// falta sobre la conexión del pool (`conexion_para_canal`; si no hay viva, la
/// abre con los diálogos al solicitante). Un solo canal por host: dos ventanas
/// que lo pidan a la vez comparten uno.
pub async fn asegurar(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    host_id: i64,
    solicitante: u32,
    modo: ModoSftp,
) -> Result<(Arc<SftpSession>, String), String> {
    if let Some((sesion, dir)) = canal_vivo(estado, host_id).await {
        marcar_para_archivos(estado, host_id, modo).await;
        return Ok((sesion, dir));
    }

    // Cerrojo por host: solo una apertura a la vez para este host.
    let cerrojo = {
        let mut estado_bloqueado = estado.lock().await;
        estado_bloqueado
            .sftp_cerrojos
            .entry(host_id)
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    };
    let _guardia = cerrojo.lock().await;

    // Otra ventana pudo abrirlo mientras esperábamos el cerrojo.
    if let Some((sesion, dir)) = canal_vivo(estado, host_id).await {
        marcar_para_archivos(estado, host_id, modo).await;
        return Ok((sesion, dir));
    }

    let (host, todos_los_hosts, rutas, id_solicitud) = {
        let mut estado_bloqueado = estado.lock().await;
        let lectura = estado_bloqueado.lectura.clone();
        let lectura = lectura.lock().map_err(|_| "almacén envenenado")?;
        let host = lectura
            .obtener_host(host_id)
            .map_err(|_| "el host ya no existe".to_string())?;
        let todos_los_hosts: HashMap<i64, Host> = lectura
            .listar_hosts()
            .unwrap_or_default()
            .into_iter()
            .map(|host| (host.id, host))
            .collect();
        let rutas = estado_bloqueado.rutas.clone();
        // Id de solicitud para los diálogos: sale del mismo contador que las
        // sesiones, así que nunca choca con un id de sesión.
        let id_solicitud = estado_bloqueado.siguiente_sesion_id;
        estado_bloqueado.siguiente_sesion_id += 1;
        (host, todos_los_hosts, rutas, id_solicitud)
    };

    let politica = match modo {
        ModoSftp::Archivos => PoliticaConexion::Interactiva {
            solicitante,
            id_solicitud,
        },
        ModoSftp::Comprobacion => PoliticaConexion::NoInteractiva {
            solicitante: Some(solicitante),
            id_solicitud,
        },
    };
    match abrir(estado, &host, &todos_los_hosts, &rutas, politica).await {
        Ok(abierto) => {
            let CanalNuevo {
                sesion,
                dir_inicio,
                conexion,
                renombrador,
                nombres,
                usuario_conexion,
                uid_conexion,
            } = abierto;
            info!(
                host = %host.nombre,
                dir = %dir_inicio,
                posix_rename = renombrador.con_extension(),
                usuarios = nombres.usuarios(),
                grupos = nombres.grupos(),
                "canal SFTP abierto"
            );
            let mut estado_bloqueado = estado.lock().await;
            estado_bloqueado.sftp.insert(
                host_id,
                SftpHost {
                    host_id,
                    host_nombre: host.nombre.clone(),
                    sesion: sesion.clone(),
                    conexion,
                    dir_inicio: dir_inicio.clone(),
                    solicitante,
                    ultima_actividad: Instant::now(),
                    para_archivos: modo == ModoSftp::Archivos,
                    renombrador: Arc::new(renombrador),
                    nombres: Arc::new(nombres),
                    usuario_conexion: Some(usuario_conexion),
                    uid_conexion,
                },
            );
            drop(estado_bloqueado);
            // El host estrena canal: es el momento de sus túneles automáticos
            // (salvo que sea solo para una comprobación).
            if modo == ModoSftp::Archivos {
                super::tuneles::canales_cambiaron(estado, host_id).await;
            }
            Ok((sesion, dir_inicio))
        }
        Err(motivo) => {
            warn!(host = %host.nombre, "no se pudo abrir el canal SFTP: {motivo}");
            Err(motivo)
        }
    }
}

/// Archivos usa un canal que se abrió para una comprobación: desde ahora
/// cuenta para el ciclo automático de los túneles.
async fn marcar_para_archivos(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    host_id: i64,
    modo: ModoSftp,
) {
    if modo != ModoSftp::Archivos {
        return;
    }
    let estrena = {
        let mut estado_bloqueado = estado.lock().await;
        match estado_bloqueado.sftp.get_mut(&host_id) {
            Some(canal) if !canal.para_archivos => {
                canal.para_archivos = true;
                true
            }
            _ => false,
        }
    };
    if estrena {
        super::tuneles::canales_cambiaron(estado, host_id).await;
    }
}

/// Canal ya abierto para el host, refrescando su marca de actividad. Uno
/// cuya conexión se cayó no se devuelve: se retira (como `perdido`) para que
/// el siguiente `asegurar` abra otro.
async fn canal_vivo(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    host_id: i64,
) -> Option<(Arc<SftpSession>, String)> {
    let muerto = {
        let mut estado_bloqueado = estado.lock().await;
        let canal = estado_bloqueado.sftp.get_mut(&host_id)?;
        if !canal.conexion.handle.is_closed() {
            canal.ultima_actividad = Instant::now();
            return Some((canal.sesion.clone(), canal.dir_inicio.clone()));
        }
        true
    };
    if muerto {
        perdido(estado, host_id).await;
    }
    None
}

/// Sesión de un canal ya abierto, sin abrirlo si no lo está.
pub async fn canal_abierto(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    host_id: i64,
) -> Option<Arc<SftpSession>> {
    canal_vivo(estado, host_id).await.map(|(sesion, _)| sesion)
}

/// El canal de un host dejó de funcionar: se quita y sus transferencias pasan
/// a error, porque el siguiente `AbrirSftp` lo volverá a abrir.
pub async fn perdido(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>, host_id: i64) {
    let canal = estado.lock().await.sftp.remove(&host_id);
    let Some(canal) = canal else {
        return;
    };
    warn!(host = %canal.host_nombre, "se perdió el canal SFTP");
    canal.renombrador.cerrar();
    canal.conexion.soltar().await;
    super::transferencias::caida(estado, host_id).await;
    // Sin canal SFTP: si no le queda ninguna pestaña, sus túneles automáticos
    // se paran.
    super::tuneles::canales_cambiaron(estado, host_id).await;
}

/// Lo que deja `abrir`: el canal principal y lo que se averigua al abrirlo.
struct CanalNuevo {
    sesion: Arc<SftpSession>,
    dir_inicio: String,
    conexion: ConexionTomada,
    renombrador: Renombrador,
    nombres: MapaNombres,
    usuario_conexion: String,
    uid_conexion: Option<u32>,
}

/// Toma la conexión del host (del pool, o una nueva que queda en él) y abre el
/// subsistema `sftp`. En toda rama de error se cierra el canal y se suelta la
/// conexión por identidad. Con el canal ya abierto, el segundo canal del
/// renombrador y los nombres de propietario: ninguno de los dos lo hace fallar.
async fn abrir(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    host: &Host,
    todos_los_hosts: &HashMap<i64, Host>,
    rutas: &crate::config::Rutas,
    politica: PoliticaConexion,
) -> Result<CanalNuevo, String> {
    let conexion = conexiones::conexion_para_canal(
        estado,
        host,
        todos_los_hosts,
        rutas,
        politica,
        UsoCanal::Subsistema,
    )
    .await?;

    // Abrir el canal y pedir el subsistema. Un host sin SFTP falla aquí.
    let handle = conexion.handle.clone();
    let canal = match conexiones::abrir_con_plazo(HANDSHAKE_SFTP, async move {
        handle.channel_open_session().await
    })
    .await
    {
        Ok(canal) => canal,
        Err(motivo) => {
            conexion.soltar().await;
            return Err(format!("no se pudo abrir el canal SFTP: {motivo}"));
        }
    };
    // `request_subsystem` devuelve `Ok` aunque el host diga que no: el plazo
    // lo pone el saludo de abajo, pero pedirlo también puede colgarse.
    match tokio::time::timeout(HANDSHAKE_SFTP, canal.request_subsystem(true, "sftp")).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            let _ = canal.close().await;
            conexion.soltar().await;
            return Err(format!("el host no ofrece SFTP ({error})"));
        }
        Err(_) => {
            let _ = canal.close().await;
            conexion.soltar().await;
            return Err("el host no ofrece SFTP".to_string());
        }
    }
    // Un host sin subsistema `sftp` no falla al pedirlo: simplemente no
    // contesta al saludo del protocolo. Por eso hay plazo, como en DNS/TCP.
    // Desde aquí el canal va dentro del flujo, que lo cierra al soltarse.
    let sesion =
        match tokio::time::timeout(HANDSHAKE_SFTP, SftpSession::new(canal.into_stream())).await {
            Ok(Ok(sesion)) => sesion,
            Ok(Err(error)) => {
                conexion.soltar().await;
                return Err(format!("el host no ofrece SFTP ({error})"));
            }
            Err(_) => {
                conexion.soltar().await;
                return Err("el host no ofrece SFTP".to_string());
            }
        };
    // El directorio de inicio del usuario remoto es el punto de partida.
    let inicio_real = match tokio::time::timeout(HANDSHAKE_SFTP, sesion.canonicalize(".")).await {
        Ok(Ok(dir)) => Some(dir),
        _ => None,
    };
    // El renombrador y los nombres, a la vez: cada uno con su plazo.
    let (renombrador, nombres) = tokio::join!(
        Renombrador::abrir(conexion.handle.clone()),
        leer_nombres(&sesion)
    );
    // El mismo usuario con el que autentica la conexión (`autenticar`).
    let usuario_conexion = host
        .usuario
        .clone()
        .unwrap_or_else(crate::conexion::usuario_local);
    // Si el host no lo tiene en su `/etc/passwd`, el dueño de su directorio
    // de inicio; nunca el de `/` (sería root y callaría el aviso de dueño).
    let uid_conexion = match (nombres.uid_de(&usuario_conexion), &inicio_real) {
        (Some(uid), _) => Some(uid),
        (None, Some(dir)) => {
            match tokio::time::timeout(HANDSHAKE_SFTP, sesion.metadata(dir.as_str())).await {
                Ok(Ok(metadata)) => metadata.uid,
                _ => None,
            }
        }
        (None, None) => None,
    };
    let dir_inicio = inicio_real.unwrap_or_else(|| "/".to_string());
    Ok(CanalNuevo {
        sesion: Arc::new(sesion),
        dir_inicio,
        conexion,
        renombrador,
        nombres,
        usuario_conexion,
        uid_conexion,
    })
}

// ---------------------------------------------------------------- revision

/// Cierra los canales SFTP inactivos. Ninguna conexión de red se toca con el
/// bloqueo del estado tomado.
pub async fn revisar(estado: Arc<tokio::sync::Mutex<EstadoServidor>>) {
    loop {
        tokio::time::sleep(REVISION).await;
        let vencidos = {
            let estado_bloqueado = estado.lock().await;
            let ahora = Instant::now();
            let con_transferencias: Vec<i64> = estado_bloqueado
                .sftp
                .keys()
                .copied()
                .filter(|host_id| estado_bloqueado.transferencias.vivas_del_host(*host_id))
                .collect();
            let muertos: Vec<i64> = estado_bloqueado
                .sftp
                .iter()
                .filter(|(_, canal)| canal.conexion.handle.is_closed())
                .map(|(host_id, _)| *host_id)
                .collect();
            let vencidos: Vec<i64> = estado_bloqueado
                .sftp
                .iter()
                .filter(|(host_id, canal)| {
                    !muertos.contains(host_id)
                        && puede_cerrarse(
                            canal.ultima_actividad,
                            ahora,
                            con_transferencias.contains(host_id),
                        )
                })
                .map(|(host_id, _)| *host_id)
                .collect();
            // Los de una conexión caída se tratan como perdidos (sus
            // transferencias pasan a error) sin esperar a la inactividad.
            drop(estado_bloqueado);
            for host_id in muertos {
                perdido(&estado, host_id).await;
            }
            let mut estado_bloqueado = estado.lock().await;
            let mut salidas = Vec::new();
            for host_id in vencidos {
                if let Some(canal) = estado_bloqueado.sftp.remove(&host_id) {
                    salidas.push(canal);
                }
            }
            salidas
        };
        for canal in vencidos {
            info!(host = %canal.host_nombre, "cerrando el canal SFTP inactivo");
            let _ = tokio::time::timeout(HANDSHAKE_SFTP, canal.sesion.close()).await;
            canal.renombrador.cerrar();
            let host_id = canal.host_id;
            canal.conexion.soltar().await;
            // Un canal SFTP que se cierra por inactividad es un canal menos para
            // el ciclo automático de los túneles del host.
            super::tuneles::canales_cambiaron(&estado, host_id).await;
        }
    }
}

/// Cierra el canal de un host (conexión caída o apagado del servidor).
/// Devuelve el nombre del host si había canal.
pub async fn cerrar(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    host_id: i64,
) -> Option<String> {
    let SftpHost {
        sesion,
        conexion,
        host_nombre,
        renombrador,
        ..
    } = estado.lock().await.sftp.remove(&host_id)?;
    let _ = tokio::time::timeout(HANDSHAKE_SFTP, sesion.close()).await;
    renombrador.cerrar();
    conexion.soltar().await;
    super::tuneles::canales_cambiaron(estado, host_id).await;
    Some(host_nombre)
}

/// Cierra todos los canales (apagado del servidor).
pub async fn cerrar_todos(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>) {
    let canales: Vec<SftpHost> = {
        let mut estado_bloqueado = estado.lock().await;
        let ids: Vec<i64> = estado_bloqueado.sftp.keys().copied().collect();
        ids.into_iter()
            .filter_map(|host_id| estado_bloqueado.sftp.remove(&host_id))
            .collect()
    };
    for canal in canales {
        let _ = tokio::time::timeout(HANDSHAKE_SFTP, canal.sesion.close()).await;
        canal.renombrador.cerrar();
        canal.conexion.soltar().await;
    }
}

// ---------------------------------------------------------------- operaciones

/// Lista un directorio remoto. Las entradas salen ordenadas (directorios
/// primero) y con el destino de los enlaces resuelto.
pub async fn listar(sesion: &SftpSession, ruta: &str) -> Result<Vec<Entrada>, String> {
    let lectura = sesion
        .read_dir(ruta)
        .await
        .map_err(|error| describir(error.to_string()))?;
    let mut entradas = Vec::new();
    for elemento in lectura {
        let metadata = elemento.metadata();
        let tipo = if metadata.is_symlink() {
            TipoEntrada::Enlace
        } else if metadata.is_dir() {
            TipoEntrada::Directorio
        } else {
            TipoEntrada::Fichero
        };
        let enlace = if tipo == TipoEntrada::Enlace {
            let camino = format!("{}/{}", ruta.trim_end_matches('/'), elemento.file_name());
            sesion.read_link(camino).await.ok()
        } else {
            None
        };
        entradas.push(Entrada {
            nombre: elemento.file_name(),
            tipo,
            tamano: metadata.size.unwrap_or(0),
            mtime: metadata.mtime.map(i64::from).unwrap_or(0),
            permisos: metadata.permissions,
            propietario: propietario_de(&metadata),
            enlace,
            marca: Default::default(),
        });
    }
    ordenar(&mut entradas);
    Ok(entradas)
}

/// uid y gid de los atributos (SFTP v3 solo manda números). Los nombres los
/// pone después el mapa de `/etc/passwd` y `/etc/group` del host.
pub fn propietario_de(
    metadata: &russh_sftp::protocol::FileAttributes,
) -> Option<crate::archivos::Propietario> {
    if metadata.uid.is_none() && metadata.gid.is_none() {
        return None;
    }
    Some(crate::archivos::Propietario {
        uid: metadata.uid,
        gid: metadata.gid,
        usuario: metadata.user.clone(),
        grupo: metadata.group.clone(),
    })
}

/// `CambiarPermisos` tal como llega: a cada ruta (y, con alcance, a lo que
/// contiene) se le aplica `(viejo & !mascara) | (modo & mascara)`.
#[derive(Debug, Clone)]
pub struct PeticionPermisos {
    pub rutas: Vec<String>,
    pub modo: u32,
    pub mascara: u32,
    pub alcance: Option<crate::protocolo::AlcancePermisos>,
}

/// `ListarArbol` tal como llega.
#[derive(Debug, Clone)]
pub struct PeticionArbol {
    pub ruta: String,
    /// Antes del `.magiignore`.
    pub exclusiones: Vec<String>,
    pub usar_magiignore: bool,
    /// Después del `.magiignore`.
    pub exclusiones_extra: Vec<String>,
}

/// `StatRemoto`: metadatos sin seguir enlaces, o ninguno si la ruta no existe.
pub async fn stat(sesion: &SftpSession, ruta: &str) -> Result<Option<Entrada>, String> {
    match metadatos(sesion, ruta).await {
        Ok(entrada) => Ok(Some(entrada)),
        Err(motivo) if motivo == "la ruta no existe" => Ok(None),
        Err(motivo) => Err(motivo),
    }
}

/// Metadatos de una ruta remota, sin seguir enlaces.
pub async fn metadatos(sesion: &SftpSession, ruta: &str) -> Result<Entrada, String> {
    let metadata = sesion
        .symlink_metadata(ruta)
        .await
        .map_err(|error| describir(error.to_string()))?;
    let tipo = if metadata.is_symlink() {
        TipoEntrada::Enlace
    } else if metadata.is_dir() {
        TipoEntrada::Directorio
    } else {
        TipoEntrada::Fichero
    };
    let nombre = ruta
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(ruta)
        .to_string();
    Ok(Entrada {
        nombre,
        tipo,
        tamano: metadata.size.unwrap_or(0),
        mtime: metadata.mtime.map(i64::from).unwrap_or(0),
        permisos: metadata.permissions,
        propietario: propietario_de(&metadata),
        enlace: None,
        marca: Default::default(),
    })
}

pub async fn existe(sesion: &SftpSession, ruta: &str) -> bool {
    sesion.symlink_metadata(ruta).await.is_ok()
}

// ---------------------------------------------------------------- permisos

/// Errores que caben en el detalle de `Hecho` (el resto se cuenta).
const ERRORES_EN_DETALLE: usize = 5;

/// Lo que pasó al aplicar un `CambiarPermisos`.
#[derive(Debug, Default)]
pub struct ResultadoPermisos {
    /// Rutas a las que se aplicó el modo.
    pub afectados: u32,
    /// Enlaces que se saltaron (`setstat` los seguiría).
    pub enlaces: u32,
    /// Rutas que fallaron, con su motivo: no paran el resto.
    pub errores: Vec<(String, String)>,
    /// La conexión se cayó: lo que quedaba no se intentó.
    pub caida: Option<String>,
}

impl ResultadoPermisos {
    fn anotar_error(&mut self, ruta: &str, motivo: String) {
        if motivo.contains("se cayó la conexión") {
            self.caida = Some(motivo);
        } else {
            self.errores.push((ruta.to_string(), motivo));
        }
    }

    /// Detalle de `Hecho`: «N afectados · E enlaces sin tocar · M errores:
    /// ruta: motivo; …», con la lista de errores recortada.
    pub fn detalle(&self) -> String {
        let mut partes = vec![contar(self.afectados as usize, "afectado", "afectados")];
        if self.enlaces > 0 {
            partes.push(contar(
                self.enlaces as usize,
                "enlace sin tocar",
                "enlaces sin tocar",
            ));
        }
        if !self.errores.is_empty() {
            let mut lista: Vec<String> = self
                .errores
                .iter()
                .take(ERRORES_EN_DETALLE)
                .map(|(ruta, motivo)| format!("{ruta}: {motivo}"))
                .collect();
            let resto = self.errores.len().saturating_sub(ERRORES_EN_DETALLE);
            if resto > 0 {
                lista.push(format!("…y {resto} más"));
            }
            partes.push(format!(
                "{}: {}",
                contar(self.errores.len(), "error", "errores"),
                lista.join("; ")
            ));
        }
        partes.join(" · ")
    }

    /// Detalle de la anotación `permisos_cambiados`: «rutas · modo 0644 ·
    /// máscara 0777 · alcance · N afectados[ · M errores]».
    pub fn detalle_registro(&self, peticion: &PeticionPermisos) -> String {
        let alcance = peticion
            .alcance
            .map_or("sin recursivo", crate::protocolo::AlcancePermisos::texto);
        let mut detalle = format!(
            "{} · modo {:04o} · máscara {:04o} · {alcance} · {}",
            peticion.rutas.join(", "),
            peticion.modo & 0o7777,
            peticion.mascara & 0o7777,
            contar(self.afectados as usize, "afectado", "afectados"),
        );
        if !self.errores.is_empty() {
            detalle.push_str(&format!(
                " · {}",
                contar(self.errores.len(), "error", "errores")
            ));
        }
        if let Some(motivo) = &self.caida {
            detalle.push_str(&format!(" · {motivo}"));
        }
        detalle
    }
}

/// «1 afectado», «3 afectados».
fn contar(cuantos: usize, singular: &str, plural: &str) -> String {
    if cuantos == 1 {
        format!("1 {singular}")
    } else {
        format!("{cuantos} {plural}")
    }
}

/// Modo nuevo de una ruta: los bits de la máscara salen de `modo` y el resto
/// (también setuid, setgid y sticky) se conserva. Sin el modo viejo solo se
/// puede si la máscara lo cubre todo.
pub fn modo_nuevo(viejo: Option<u32>, modo: u32, mascara: u32) -> Option<u32> {
    let mascara = mascara & 0o7777;
    match viejo {
        Some(viejo) => Some((viejo & 0o7777 & !mascara) | (modo & mascara)),
        None if mascara == 0o7777 => Some(modo & 0o7777),
        None => None,
    }
}

/// `CambiarPermisos` (§7.4): a cada ruta y, con alcance, a lo que contiene.
/// Recorre sin seguir enlaces y los enlaces no se tocan nunca. Un error en una
/// ruta no para el resto; una caída de la conexión, sí.
pub async fn cambiar_permisos(
    sesion: &SftpSession,
    peticion: &PeticionPermisos,
) -> ResultadoPermisos {
    let mut resultado = ResultadoPermisos::default();
    for ruta in &peticion.rutas {
        if resultado.caida.is_some() {
            break;
        }
        aplicar_permisos(sesion, peticion, ruta, &mut resultado).await;
    }
    resultado
}

async fn aplicar_permisos(
    sesion: &SftpSession,
    peticion: &PeticionPermisos,
    ruta: &str,
    resultado: &mut ResultadoPermisos,
) {
    use crate::protocolo::AlcancePermisos;
    let metadata = match sesion.symlink_metadata(ruta).await {
        Ok(metadata) => metadata,
        Err(error) => {
            resultado.anotar_error(ruta, describir(error.to_string()));
            return;
        }
    };
    if metadata.is_symlink() {
        resultado.enlaces += 1;
        return;
    }
    let es_dir = metadata.is_dir();
    let toca = match peticion.alcance {
        None | Some(AlcancePermisos::Todo) => true,
        Some(AlcancePermisos::Directorios) => es_dir,
        Some(AlcancePermisos::Ficheros) => !es_dir,
    };
    let recorre = es_dir && peticion.alcance.is_some();
    let nuevo = modo_nuevo(metadata.permissions, peticion.modo, peticion.mascara);
    // Un directorio que tras el cambio deja entrar se cambia antes de
    // recorrerlo (así un 755 abre lo que estaba en 000); uno que no, después
    // (así un 644 con alcance «todo» llega igualmente al fondo).
    let antes = toca && !(recorre && nuevo.is_some_and(|modo| modo & 0o500 != 0o500));
    if antes {
        poner_modo(sesion, ruta, nuevo, resultado).await;
    }
    if recorre && resultado.caida.is_none() {
        match sesion.read_dir(ruta).await {
            Ok(lectura) => {
                for elemento in lectura {
                    if resultado.caida.is_some() {
                        break;
                    }
                    let hijo = join(ruta, &elemento.file_name());
                    Box::pin(aplicar_permisos(sesion, peticion, &hijo, resultado)).await;
                }
            }
            Err(error) => resultado.anotar_error(ruta, describir(error.to_string())),
        }
    }
    if toca && !antes && resultado.caida.is_none() {
        poner_modo(sesion, ruta, nuevo, resultado).await;
    }
}

async fn poner_modo(
    sesion: &SftpSession,
    ruta: &str,
    nuevo: Option<u32>,
    resultado: &mut ResultadoPermisos,
) {
    let Some(nuevo) = nuevo else {
        resultado.anotar_error(ruta, "el host no dio sus permisos".to_string());
        return;
    };
    let atributos = FileAttributes {
        permissions: Some(nuevo),
        ..FileAttributes::empty()
    };
    match sesion.set_metadata(ruta, atributos).await {
        Ok(()) => resultado.afectados += 1,
        Err(error) => resultado.anotar_error(ruta, describir(error.to_string())),
    }
}

// ---------------------------------------------------------------- árbol

/// Entradas por bloque de `Arbol`.
pub const BLOQUE_ARBOL: usize = 1000;

/// Tope del `.magiignore` de la raíz.
const TOPE_MAGIIGNORE: usize = 1024 * 1024;

/// Recorrido de `ListarArbol`: lee el árbol a medida que se le piden bloques,
/// podando lo excluido (que se cuenta y, si es un directorio, no se recorre).
pub struct RecorridoArbol {
    raiz: String,
    exclusiones: crate::archivos::exclusiones::Exclusiones,
    nombres: Arc<MapaNombres>,
    /// Directorios por leer, relativos a la raíz (la raíz es `""`).
    pendientes: Vec<String>,
    /// Entradas leídas que aún no han salido en un bloque.
    leidas: std::collections::VecDeque<EntradaArbol>,
    excluidos: u32,
}

impl RecorridoArbol {
    /// Comprueba la raíz (se sigue si es un enlace: es la que pidió el
    /// usuario), lee su `.magiignore` si se pide y compila las exclusiones en
    /// su orden. Devuelve el texto del `.magiignore` para el primer bloque.
    pub async fn abrir(
        sesion: &SftpSession,
        peticion: &PeticionArbol,
        nombres: Arc<MapaNombres>,
    ) -> Result<(Self, Option<String>), String> {
        let raiz = match peticion.ruta.trim_end_matches('/') {
            "" => "/".to_string(),
            raiz => raiz.to_string(),
        };
        let metadata = sesion
            .metadata(raiz.as_str())
            .await
            .map_err(|error| describir(error.to_string()))?;
        if !metadata.is_dir() {
            return Err(format!("{raiz} no es un directorio"));
        }
        let magiignore = if peticion.usar_magiignore {
            leer_magiignore(sesion, &join(&raiz, ".magiignore")).await?
        } else {
            None
        };
        let exclusiones = crate::archivos::exclusiones::Exclusiones::nueva(
            &peticion.exclusiones,
            magiignore.as_deref(),
            &peticion.exclusiones_extra,
        )?;
        let recorrido = Self {
            raiz,
            exclusiones,
            nombres,
            pendientes: vec![String::new()],
            leidas: Default::default(),
            excluidos: 0,
        };
        Ok((recorrido, magiignore))
    }

    /// Siguiente bloque (hasta `BLOQUE_ARBOL` entradas) y si es el último. Un
    /// directorio que no se puede leer aborta el recorrido: un plan sobre un
    /// árbol a medias borraría lo que no se vio.
    pub async fn siguiente(
        &mut self,
        sesion: &SftpSession,
    ) -> Result<(Vec<EntradaArbol>, bool), String> {
        while self.leidas.len() < BLOQUE_ARBOL {
            let Some(relativa) = self.pendientes.pop() else {
                break;
            };
            self.leer_directorio(sesion, &relativa).await?;
        }
        let cuantas = self.leidas.len().min(BLOQUE_ARBOL);
        let bloque: Vec<EntradaArbol> = self.leidas.drain(..cuantas).collect();
        let fin = self.leidas.is_empty() && self.pendientes.is_empty();
        Ok((bloque, fin))
    }

    /// Entradas que dejaron fuera las exclusiones (completo al terminar).
    pub fn excluidos(&self) -> u32 {
        self.excluidos
    }

    async fn leer_directorio(
        &mut self,
        sesion: &SftpSession,
        relativa: &str,
    ) -> Result<(), String> {
        let absoluta = if relativa.is_empty() {
            self.raiz.clone()
        } else {
            join(&self.raiz, relativa)
        };
        let lectura = sesion
            .read_dir(absoluta.as_str())
            .await
            .map_err(|error| format!("{absoluta}: {}", describir(error.to_string())))?;
        for elemento in lectura {
            let nombre = elemento.file_name();
            let ruta = if relativa.is_empty() {
                nombre.clone()
            } else {
                format!("{relativa}/{nombre}")
            };
            // `readdir` da los atributos del propio enlace (lstat).
            let propia = elemento.metadata();
            let (tipo, atributos, enlace_a_dir) = if propia.is_symlink() {
                // Se sigue solo para describirlo: a un directorio no se entra
                // (el plan lo omite); de uno a fichero cuenta el destino; uno
                // roto se queda con lo suyo.
                match sesion.metadata(join(&absoluta, &nombre)).await {
                    Ok(destino) if destino.is_dir() => (TipoEntrada::Enlace, propia, true),
                    Ok(destino) => (TipoEntrada::Enlace, destino, false),
                    Err(error) => {
                        let motivo = describir(error.to_string());
                        if motivo.contains("se cayó la conexión") {
                            return Err(motivo);
                        }
                        (TipoEntrada::Enlace, propia, false)
                    }
                }
            } else if propia.is_dir() {
                (TipoEntrada::Directorio, propia, false)
            } else {
                (TipoEntrada::Fichero, propia, false)
            };
            let es_dir = tipo == TipoEntrada::Directorio || enlace_a_dir;
            if self.exclusiones.excluida(&ruta, es_dir) {
                self.excluidos += 1;
                continue;
            }
            if tipo == TipoEntrada::Directorio {
                self.pendientes.push(ruta.clone());
            }
            self.leidas.push_back(EntradaArbol {
                ruta,
                tipo,
                tamano: atributos.size.unwrap_or(0),
                mtime: atributos.mtime.map(i64::from).unwrap_or(0),
                permisos: atributos.permissions,
                propietario: self.nombres.nombrar(propietario_de(&atributos)),
                enlace_a_dir,
            });
        }
        Ok(())
    }
}

/// El `.magiignore` de la raíz: ninguno si no existe. Uno que pasa del tope o
/// no es texto es un error: recortarlo dejaría sin proteger lo que excluye.
async fn leer_magiignore(sesion: &SftpSession, ruta: &str) -> Result<Option<String>, String> {
    let Some((datos, completo)) = leer_con_tope(sesion, ruta, TOPE_MAGIIGNORE)
        .await
        .map_err(|motivo| format!("{ruta}: {motivo}"))?
    else {
        return Ok(None);
    };
    if !completo {
        return Err(format!("{ruta} pasa de 1 MiB"));
    }
    String::from_utf8(datos)
        .map(Some)
        .map_err(|_| format!("{ruta} no es texto UTF-8"))
}

/// Borra una ruta remota; para un directorio, recursivamente. Un enlace se
/// borra como enlace (nunca se sigue).
pub async fn borrar(sesion: &SftpSession, ruta: &str) -> Result<(), String> {
    let metadata = sesion
        .symlink_metadata(ruta)
        .await
        .map_err(|error| describir(error.to_string()))?;
    if metadata.is_dir() && !metadata.is_symlink() {
        let lectura = sesion
            .read_dir(ruta)
            .await
            .map_err(|error| describir(error.to_string()))?;
        for elemento in lectura {
            let hijo = join(ruta, &elemento.file_name());
            Box::pin(borrar(sesion, &hijo)).await?;
        }
        sesion
            .remove_dir(ruta)
            .await
            .map_err(|error| describir(error.to_string()))?;
    } else {
        sesion
            .remove_file(ruta)
            .await
            .map_err(|error| describir(error.to_string()))?;
    }
    Ok(())
}

pub async fn renombrar(sesion: &SftpSession, de: &str, a: &str) -> Result<(), String> {
    sesion
        .rename(de, a)
        .await
        .map_err(|error| describir(error.to_string()))
}

pub async fn crear_dir(sesion: &SftpSession, ruta: &str) -> Result<(), String> {
    sesion
        .create_dir(ruta)
        .await
        .map_err(|error| describir(error.to_string()))
}

/// Crea un directorio y los que le falten a sus padres.
pub async fn crear_dir_padres(sesion: &SftpSession, ruta: &str) -> Result<(), String> {
    let mut camino = String::new();
    for trozo in ruta.trim_end_matches('/').split('/') {
        if trozo.is_empty() {
            camino.push('/');
            continue;
        }
        camino = join(&camino, trozo);
        if sesion.symlink_metadata(&camino).await.is_ok() {
            continue;
        }
        sesion
            .create_dir(&camino)
            .await
            .map_err(|error| describir(error.to_string()))?;
    }
    Ok(())
}

/// Directorio padre de una ruta remota, en la notación de la SFTP.
pub fn padre(ruta: &str) -> String {
    let sin_barra = ruta.trim_end_matches('/');
    match sin_barra.rfind('/') {
        Some(0) => "/".to_string(),
        Some(posicion) => sin_barra[..posicion].to_string(),
        None => ".".to_string(),
    }
}

/// Une dos trozos de una ruta remota sin duplicar la barra.
pub fn join(base: &str, nombre: &str) -> String {
    if base.is_empty() {
        return nombre.to_string();
    }
    if base.ends_with('/') {
        format!("{base}{nombre}")
    } else {
        format!("{base}/{nombre}")
    }
}

/// Número de los temporales: único dentro del servidor, así que dos ventanas
/// (cada una con su contador de peticiones) nunca comparten temporal.
static SIGUIENTE_TEMPORAL: AtomicU64 = AtomicU64::new(1);

/// Intentos de encontrar un nombre libre (el directorio de ediciones no se
/// vacía y puede guardar los de un servidor anterior).
const INTENTOS_NOMBRE: usize = 1000;

/// Copia un fichero remoto a un temporal local para verlo con `$PAGER` o, con
/// `edicion`, para editarlo: ese va al directorio de ediciones, que nadie
/// vacía. El directorio va en 700 y el fichero en 600, con un nombre único
/// `<n>-<nombre>` creado en exclusiva, y va por bloques para no cargar el
/// fichero entero en memoria.
pub async fn copia_temporal(
    rutas: &crate::config::Rutas,
    sesion: &SftpSession,
    ruta_remota: &str,
    edicion: bool,
) -> Result<String, String> {
    let directorio = if edicion {
        dir_ediciones(rutas)
    } else {
        dir_temporales(rutas)
    };
    crear_dir_privado(&directorio)?;
    let nombre = ruta_remota
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("fichero");
    let nombre = sanear_nombre(nombre);

    let mut origen = sesion
        .open(ruta_remota)
        .await
        .map_err(|error| describir(error.to_string()))?;
    let (destino, mut salida) = crear_temporal(&directorio, &nombre)?;

    let copia = copiar_fichero(&mut origen, &mut salida).await;
    if let Err(motivo) = copia {
        drop(salida);
        let _ = std::fs::remove_file(&destino);
        return Err(motivo);
    }
    drop(salida);

    if let Ok(metadata) = sesion.metadata(ruta_remota).await {
        if let Some(mtime) = metadata.mtime {
            let momento = filetime::FileTime::from_unix_time(i64::from(mtime), 0);
            let _ = filetime::set_file_mtime(&destino, momento);
        }
    }
    Ok(destino.display().to_string())
}

/// Crea en exclusiva (`create_new`, que tampoco sigue un enlace que ya esté
/// ahí) un fichero 600 con el siguiente número libre.
fn crear_temporal(directorio: &Path, nombre: &str) -> Result<(PathBuf, std::fs::File), String> {
    for _ in 0..INTENTOS_NOMBRE {
        let numero = SIGUIENTE_TEMPORAL.fetch_add(1, Ordering::Relaxed);
        let destino = directorio.join(format!("{numero}-{nombre}"));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&destino)
        {
            Ok(fichero) => return Ok((destino, fichero)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("{}: {error}", destino.display())),
        }
    }
    Err(format!(
        "no hay un nombre libre para el temporal en {}",
        directorio.display()
    ))
}

/// Copia por bloques de 64 KiB entre un fichero SFTP y uno local.
async fn copiar_fichero(
    origen: &mut russh_sftp::client::fs::File,
    salida: &mut std::fs::File,
) -> Result<u64, String> {
    use std::io::Write as _;
    use tokio::io::AsyncReadExt as _;
    let mut bufer = vec![0u8; BLOQUE];
    let mut total = 0u64;
    loop {
        let leidos = origen
            .read(&mut bufer)
            .await
            .map_err(|error| format!("leyendo el remoto: {error}"))?;
        if leidos == 0 {
            break;
        }
        salida
            .write_all(&bufer[..leidos])
            .map_err(|error| format!("escribiendo el temporal: {error}"))?;
        total += leidos as u64;
    }
    salida
        .flush()
        .map_err(|error| format!("escribiendo el temporal: {error}"))?;
    Ok(total)
}

/// Traduce un error de SFTP a algo legible para la barra del cliente.
pub fn describir(motivo: String) -> String {
    let minusculas = motivo.to_lowercase();
    if minusculas.contains("no such file") || minusculas.contains("nosuchfile") {
        return "la ruta no existe".to_string();
    }
    if minusculas.contains("permission denied") || minusculas.contains("permissiondenied") {
        return "permiso denegado".to_string();
    }
    if minusculas.contains("no connection")
        || minusculas.contains("noconnection")
        || minusculas.contains("unexpectedeof")
        || minusculas.contains("unexpected eof")
        || minusculas.contains("broken pipe")
        || minusculas.contains("connection reset")
        || minusculas.contains("session closed")
        || minusculas.contains("sender dropped")
    {
        return "se cayó la conexión".to_string();
    }
    if minusculas.contains("already exists") {
        return "ya existe algo con ese nombre".to_string();
    }
    motivo
}

/// Quita de un nombre cualquier cosa que permita salir del directorio.
fn sanear_nombre(nombre: &str) -> String {
    let limpio: String = nombre
        .chars()
        .map(|caracter| match caracter {
            '/' | '\\' | '\0' => '_',
            otro => otro,
        })
        .collect();
    if limpio.is_empty() || limpio == "." || limpio == ".." {
        return "fichero".to_string();
    }
    // Con el número delante tiene que caber en los 255 bytes de un nombre.
    let mut corte = limpio.len().min(TOPE_NOMBRE_TEMPORAL);
    while !limpio.is_char_boundary(corte) {
        corte -= 1;
    }
    limpio[..corte].to_string()
}

/// Bytes del nombre original que se conservan en el del temporal.
const TOPE_NOMBRE_TEMPORAL: usize = 200;

/// Borra un temporal de los nuestros: un fichero que cuelgue directamente del
/// directorio de temporales o del de ediciones, sin `.` ni `..` en la ruta
/// (`remove_file` los resolvería y saldría del directorio). Cualquier otra
/// cosa no se toca.
pub fn borrar_temporal(rutas: &crate::config::Rutas, ruta: &str) -> Result<(), String> {
    let rechazo = || "esa ruta no es un temporal de MAGI".to_string();
    if ruta.split('/').any(|trozo| trozo == "." || trozo == "..") {
        return Err(rechazo());
    }
    let camino = Path::new(ruta);
    let padre_valido = camino
        .parent()
        .is_some_and(|padre| padre == dir_temporales(rutas) || padre == dir_ediciones(rutas));
    if !padre_valido {
        return Err(rechazo());
    }
    match std::fs::symlink_metadata(camino) {
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(_) => return Err(rechazo()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("{}: {error}", camino.display())),
    }
    match std::fs::remove_file(camino) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("{}: {error}", camino.display())),
    }
}

/// El canal se comparte entre tareas: si esto dejara de cumplirse, el diseño
/// de «un canal por host» no se sostendría.
const _: fn() = || {
    fn exige_send_sync<T: Send + Sync>() {}
    exige_send_sync::<SftpSession>();
    exige_send_sync::<Arc<SftpSession>>();
    exige_send_sync::<russh_sftp::client::fs::File>();
    exige_send_sync::<crate::conexion::salto::Transporte>();
    exige_send_sync::<crate::conexion::cliente::Contexto>();
    exige_send_sync::<ConexionTomada>();
    exige_send_sync::<Renombrador>();
    exige_send_sync::<MapaNombres>();
};

/// Banderas de apertura de un fichero remoto para escribir: se crea si no
/// existe y se trunca si existe (la política de conflicto ya se decidió).
pub fn banderas_escritura() -> OpenFlags {
    OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE
}

#[cfg(test)]
mod pruebas {
    use super::*;

    const _: fn() = || {
        fn exige_send_sync<T: Send + Sync>() {}
        exige_send_sync::<SftpSession>();
    };

    #[test]
    fn un_canal_inactivo_sin_transferencias_se_cierra_a_los_diez_minutos() {
        let ahora = Instant::now();
        assert!(!puede_cerrarse(ahora, ahora, false));
        assert!(!puede_cerrarse(
            ahora - Duration::from_secs(599),
            ahora,
            false
        ));
        assert!(puede_cerrarse(
            ahora - Duration::from_secs(600),
            ahora,
            false
        ));
    }

    #[test]
    fn un_canal_con_transferencias_no_se_cierra_aunque_este_inactivo() {
        let ahora = Instant::now();
        assert!(!puede_cerrarse(
            ahora - Duration::from_secs(9999),
            ahora,
            true
        ));
    }

    #[test]
    fn las_rutas_remotas_se_componen_sin_barras_de_mas() {
        assert_eq!(join("/var/www", "app.css"), "/var/www/app.css");
        assert_eq!(join("/var/www/", "app.css"), "/var/www/app.css");
        assert_eq!(join("", "app.css"), "app.css");
        assert_eq!(padre("/var/www/app.css"), "/var/www");
        assert_eq!(padre("/app.css"), "/");
        assert_eq!(padre("app.css"), ".");
    }

    #[test]
    fn el_nombre_del_temporal_no_puede_escaparse() {
        assert_eq!(sanear_nombre("../../etc/passwd"), ".._.._etc_passwd");
        assert_eq!(sanear_nombre("normal.txt"), "normal.txt");
        assert_eq!(sanear_nombre(".."), "fichero");
        assert_eq!(sanear_nombre(""), "fichero");
        let largo = "ñ".repeat(200);
        let saneado = sanear_nombre(&largo);
        assert!(saneado.len() <= TOPE_NOMBRE_TEMPORAL, "cabe en un nombre");
        assert!(saneado.chars().all(|caracter| caracter == 'ñ'));
    }

    // ------------------------------------------------------------ nombres

    #[test]
    fn el_mapa_de_nombres_lee_passwd_y_group() {
        let passwd = "\
root:x:0:0:root:/root:/bin/bash
# comentario:x:5:
www-data:x:33:33:www-data:/var/www:/usr/sbin/nologin
+nis:x:77:
raro:x:no-es-numero:
duplicado:x:33:33::/:/bin/false
hector:x:1000:1000::/home/hector:/bin/bash";
        let group = "root:x:0:\nwww-data:x:33:\ndevs:x:1001:hector,ana\n";
        let mapa = MapaNombres::desde_textos(passwd, group);
        assert_eq!(mapa.usuario(0), Some("root"));
        assert_eq!(mapa.usuario(33), Some("www-data"), "gana el primero");
        assert_eq!(mapa.usuario(1000), Some("hector"));
        assert_eq!(mapa.usuario(77), None, "las líneas de NIS no cuentan");
        assert_eq!(mapa.usuario(5), None, "ni los comentarios");
        assert_eq!(mapa.uid_de("hector"), Some(1000));
        assert_eq!(mapa.uid_de("duplicado"), Some(33));
        assert_eq!(mapa.uid_de("raro"), None);
        assert_eq!(mapa.grupo(1001), Some("devs"));
        assert_eq!(mapa.grupo(2000), None);
    }

    #[test]
    fn nombrar_pone_lo_que_sabe_y_deja_el_numero_si_no() {
        let mapa = MapaNombres::desde_textos("www-data:x:33:33::/:/bin/false\n", "");
        let propietario = mapa
            .nombrar(Some(crate::archivos::Propietario {
                uid: Some(33),
                gid: Some(4242),
                usuario: None,
                grupo: None,
            }))
            .unwrap();
        assert_eq!(propietario.usuario_legible(), "www-data (33)");
        assert_eq!(propietario.grupo_legible(), "4242", "sin nombre, el número");
        assert_eq!(mapa.nombrar(None), None);
        let vacio = MapaNombres::default();
        let solo_numeros = vacio
            .nombrar(Some(crate::archivos::Propietario {
                uid: Some(33),
                ..Default::default()
            }))
            .unwrap();
        assert_eq!(solo_numeros.usuario, None);
    }

    // ------------------------------------------------------------ permisos

    #[test]
    fn el_modo_nuevo_solo_toca_los_bits_de_la_mascara() {
        // Casillas (9 bits): setuid se conserva.
        assert_eq!(modo_nuevo(Some(0o104755), 0o640, 0o777), Some(0o4640));
        // Octal de cuatro dígitos: se escribe todo.
        assert_eq!(modo_nuevo(Some(0o104755), 0o0640, 0o7777), Some(0o640));
        // Una casilla «mixta» fuera de la máscara se queda como estaba.
        assert_eq!(modo_nuevo(Some(0o100640), 0o004, 0o007), Some(0o644));
        assert_eq!(modo_nuevo(Some(0o100600), 0o004, 0o007), Some(0o604));
        // Sin el modo viejo solo vale una máscara completa.
        assert_eq!(modo_nuevo(None, 0o644, 0o7777), Some(0o644));
        assert_eq!(modo_nuevo(None, 0o644, 0o777), None);
    }

    #[test]
    fn el_detalle_de_permisos_cuenta_y_recorta_los_errores() {
        let mut resultado = ResultadoPermisos {
            afectados: 1,
            ..Default::default()
        };
        assert_eq!(resultado.detalle(), "1 afectado");
        resultado.enlaces = 2;
        for indice in 0..7 {
            resultado.anotar_error(&format!("/r{indice}"), "permiso denegado".to_string());
        }
        assert_eq!(
            resultado.detalle(),
            "1 afectado · 2 enlaces sin tocar · 7 errores: /r0: permiso denegado; \
             /r1: permiso denegado; /r2: permiso denegado; /r3: permiso denegado; \
             /r4: permiso denegado; …y 2 más"
        );
        // Una caída no es un error de una ruta: corta el recorrido.
        resultado.anotar_error("/r9", "se cayó la conexión".to_string());
        assert_eq!(resultado.errores.len(), 7);
        assert!(resultado.caida.is_some());

        let peticion = PeticionPermisos {
            rutas: vec!["/var/www/a".to_string(), "/var/www/b".to_string()],
            modo: 0o644,
            mascara: 0o777,
            alcance: Some(crate::protocolo::AlcancePermisos::Ficheros),
        };
        let correcto = ResultadoPermisos {
            afectados: 12,
            ..Default::default()
        };
        assert_eq!(
            correcto.detalle_registro(&peticion),
            "/var/www/a, /var/www/b · modo 0644 · máscara 0777 · solo ficheros · 12 afectados"
        );
    }

    // ------------------------------------------------------------ temporales

    fn rutas_de_prueba(raiz: &Path) -> crate::config::Rutas {
        crate::config::Rutas {
            datos: raiz.join("datos"),
            config: raiz.join("config"),
            estado: raiz.join("estado"),
            hogar: raiz.join("hogar"),
            runtime: raiz.join("runtime"),
        }
    }

    /// Al arrancar y al apagarse se vacían los temporales de ver, nunca los
    /// de edición (pueden tener cambios sin subir).
    #[test]
    fn vaciar_los_temporales_no_toca_las_ediciones() {
        let raiz = tempfile::tempdir().unwrap();
        let rutas = rutas_de_prueba(raiz.path());
        preparar_temporales(&rutas);
        crear_dir_privado(&dir_ediciones(&rutas)).unwrap();
        let ver = dir_temporales(&rutas).join("1-error.log");
        let editar = dir_ediciones(&rutas).join("2-nginx.conf");
        std::fs::write(&ver, b"x").unwrap();
        std::fs::write(&editar, b"cambios sin subir").unwrap();

        vaciar_temporales(&rutas);
        assert!(!ver.exists());
        assert!(editar.exists(), "una edición nunca se vacía");
        // Preparar ya no vacía nada.
        std::fs::write(&ver, b"x").unwrap();
        preparar_temporales(&rutas);
        assert!(ver.exists(), "preparar solo crea el directorio");
    }

    #[test]
    fn borrar_temporal_no_sigue_un_enlace_puesto_en_su_directorio() {
        let raiz = tempfile::tempdir().unwrap();
        let rutas = rutas_de_prueba(raiz.path());
        preparar_temporales(&rutas);
        let fuera = raiz.path().join("fuera.txt");
        std::fs::write(&fuera, b"no me borres").unwrap();
        let enlace = dir_temporales(&rutas).join("3-enlace");
        std::os::unix::fs::symlink(&fuera, &enlace).unwrap();

        assert!(borrar_temporal(&rutas, &enlace.display().to_string()).is_err());
        assert!(fuera.exists());
        assert!(std::fs::symlink_metadata(&enlace).is_ok());
        // Uno que ya no está no es un error (doble borrado).
        let ido = dir_temporales(&rutas).join("4-ido.txt");
        assert!(borrar_temporal(&rutas, &ido.display().to_string()).is_ok());
    }

    #[test]
    fn dos_temporales_con_el_mismo_nombre_no_se_pisan() {
        let raiz = tempfile::tempdir().unwrap();
        let directorio = raiz.path().join("tmp");
        crear_dir_privado(&directorio).unwrap();
        let (uno, _) = crear_temporal(&directorio, "a.txt").unwrap();
        let (dos, _) = crear_temporal(&directorio, "a.txt").unwrap();
        assert_ne!(uno, dos);
        // Un hueco ocupado por un servidor anterior se salta.
        let siguiente = SIGUIENTE_TEMPORAL.load(Ordering::Relaxed);
        std::fs::write(directorio.join(format!("{siguiente}-b.txt")), b"viejo").unwrap();
        let (tres, _) = crear_temporal(&directorio, "b.txt").unwrap();
        assert_ne!(tres, directorio.join(format!("{siguiente}-b.txt")));
        assert_eq!(
            std::fs::read(directorio.join(format!("{siguiente}-b.txt"))).unwrap(),
            b"viejo"
        );
    }

    // ------------------------------------------------------------ renombrador

    #[test]
    fn las_cadenas_ssh_llevan_su_longitud_delante() {
        assert_eq!(
            cadenas_ssh(&["ab", "ñ"]),
            vec![0, 0, 0, 2, b'a', b'b', 0, 0, 0, 2, 0xc3, 0xb1]
        );
    }

    /// Un `sftp-server` local hace de host: su stdin y su stdout son el flujo
    /// del subsistema. Sin él, la prueba se salta.
    fn servidor_local() -> Option<(
        tokio::process::Child,
        tokio::io::Join<tokio::process::ChildStdout, tokio::process::ChildStdin>,
    )> {
        let programa = [
            "/usr/lib/ssh/sftp-server",
            "/usr/lib/openssh/sftp-server",
            "/usr/libexec/openssh/sftp-server",
            "/usr/libexec/sftp-server",
        ]
        .into_iter()
        .map(PathBuf::from)
        .find(|ruta| ruta.exists());
        let Some(programa) = programa else {
            eprintln!("[AVISO] no hay sftp-server en el sistema: se salta la prueba");
            return None;
        };
        let mut hijo = tokio::process::Command::new(programa)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .ok()?;
        let salida = hijo.stdout.take()?;
        let entrada = hijo.stdin.take()?;
        Some((hijo, tokio::io::join(salida, entrada)))
    }

    async fn sesion_local() -> Option<(tokio::process::Child, SftpSession)> {
        let (hijo, flujo) = servidor_local()?;
        Some((hijo, SftpSession::new(flujo).await.unwrap()))
    }

    fn nombres_en(directorio: &Path) -> Vec<String> {
        let mut nombres: Vec<String> = std::fs::read_dir(directorio)
            .unwrap()
            .map(|entrada| entrada.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        nombres.sort();
        nombres
    }

    fn texto(ruta: &Path) -> String {
        std::fs::read_to_string(ruta).unwrap()
    }

    #[tokio::test]
    async fn en_tres_pasos_sustituye_el_destino_sin_dejar_restos() {
        let Some((_hijo, sesion)) = sesion_local().await else {
            return;
        };
        let raiz = tempfile::tempdir().unwrap();
        let destino = raiz.path().join("nginx.conf");
        let parcial = raiz.path().join("nginx.conf.magi-parcial");
        std::fs::write(&destino, b"viejo").unwrap();
        std::fs::write(&parcial, b"nuevo").unwrap();
        // Un viejo que dejó un intento interrumpido no estorba.
        std::fs::write(raiz.path().join("nginx.conf.magi-viejo"), b"rancio").unwrap();

        let renombrador = Renombrador::sin_extension();
        renombrador
            .renombrar_sobre(
                &sesion,
                &parcial.display().to_string(),
                &destino.display().to_string(),
            )
            .await
            .unwrap();
        assert_eq!(texto(&destino), "nuevo");
        assert_eq!(nombres_en(raiz.path()), vec!["nginx.conf"]);

        // Sin destino, un `rename` normal.
        let otro = raiz.path().join("otro.txt");
        std::fs::write(&parcial, b"solo").unwrap();
        renombrador
            .renombrar_sobre(
                &sesion,
                &parcial.display().to_string(),
                &otro.display().to_string(),
            )
            .await
            .unwrap();
        assert_eq!(texto(&otro), "solo");
        assert_eq!(nombres_en(raiz.path()), vec!["nginx.conf", "otro.txt"]);
    }

    /// Si el segundo paso falla, el original vuelve a su sitio: nunca se
    /// pierde.
    #[tokio::test]
    async fn en_tres_pasos_un_fallo_devuelve_el_original() {
        let Some((_hijo, sesion)) = sesion_local().await else {
            return;
        };
        let raiz = tempfile::tempdir().unwrap();
        let destino = raiz.path().join("datos.db");
        std::fs::write(&destino, b"lo de siempre").unwrap();
        let no_esta = raiz.path().join("no-esta.magi-parcial");

        let resultado = Renombrador::sin_extension()
            .renombrar_sobre(
                &sesion,
                &no_esta.display().to_string(),
                &destino.display().to_string(),
            )
            .await;
        assert!(resultado.is_err());
        assert_eq!(texto(&destino), "lo de siempre");
        assert_eq!(nombres_en(raiz.path()), vec!["datos.db"]);
    }

    #[tokio::test]
    async fn en_tres_pasos_un_directorio_no_se_sustituye() {
        let Some((_hijo, sesion)) = sesion_local().await else {
            return;
        };
        let raiz = tempfile::tempdir().unwrap();
        let destino = raiz.path().join("static");
        std::fs::create_dir(&destino).unwrap();
        std::fs::write(destino.join("app.css"), b"body{}").unwrap();
        let parcial = raiz.path().join("static.magi-parcial");
        std::fs::write(&parcial, b"fichero").unwrap();

        let resultado = Renombrador::sin_extension()
            .renombrar_sobre(
                &sesion,
                &parcial.display().to_string(),
                &destino.display().to_string(),
            )
            .await;
        assert!(resultado.unwrap_err().contains("directorio"));
        assert_eq!(texto(&destino.join("app.css")), "body{}");
    }

    #[tokio::test]
    async fn con_posix_rename_sustituye_de_una_vez() {
        let Some((_hijo, sesion)) = sesion_local().await else {
            return;
        };
        let Some((_otro_hijo, flujo)) = servidor_local() else {
            return;
        };
        let renombrador = Renombrador::sobre_flujo(flujo).await;
        assert!(
            renombrador.con_extension(),
            "el sftp-server de OpenSSH anuncia {POSIX_RENAME}"
        );
        let raiz = tempfile::tempdir().unwrap();
        let destino = raiz.path().join("app.env");
        let parcial = raiz.path().join("app.env.magi-parcial");
        std::fs::write(&destino, b"viejo").unwrap();
        std::fs::write(&parcial, b"nuevo").unwrap();

        renombrador
            .renombrar_sobre(
                &sesion,
                &parcial.display().to_string(),
                &destino.display().to_string(),
            )
            .await
            .unwrap();
        assert_eq!(texto(&destino), "nuevo");
        assert_eq!(nombres_en(raiz.path()), vec!["app.env"]);

        // Un origen que no está es un error legible y el destino sigue.
        let error = renombrador
            .renombrar_sobre(
                &sesion,
                &parcial.display().to_string(),
                &destino.display().to_string(),
            )
            .await
            .unwrap_err();
        assert_eq!(error, "la ruta no existe");
        assert_eq!(texto(&destino), "nuevo");

        // Cerrado, no se cuelga: falla enseguida.
        renombrador.cerrar();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(renombrador
            .renombrar_sobre(
                &sesion,
                &parcial.display().to_string(),
                &destino.display().to_string(),
            )
            .await
            .is_err());
    }

    /// Un host que corta el subsistema en el saludo deja el renombrador en
    /// tres pasos, sin error.
    #[tokio::test]
    async fn sin_saludo_queda_en_tres_pasos() {
        let (cliente, servidor) = tokio::io::duplex(1024);
        drop(servidor);
        let renombrador = Renombrador::sobre_flujo(cliente).await;
        assert!(!renombrador.con_extension());
    }
}
