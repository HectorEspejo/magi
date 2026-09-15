//! Canal SFTP por host (F4). Todo lo remoto de la vista Archivos pasa por
//! aquí: el servidor abre **un** canal por host sobre la conexión del pool (o
//! sobre una nueva, con los diálogos de siempre reenviados al solicitante) y
//! lo comparte entre todas las ventanas.
//!
//! El canal no es una sesión: no aparece en pestañas ni cuenta para la
//! difusión `Sesiones`. Se cierra solo tras diez minutos sin actividad y sin
//! transferencias, y el siguiente `AbrirSftp` lo vuelve a abrir.
//!
//! Ninguna ruta pasa por un shell: todo va por SFTP.

use std::collections::HashMap;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use russh_sftp::client::SftpSession;
use russh_sftp::protocol::OpenFlags;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::archivos::marcas::{ordenar, Entrada, TipoEntrada};
use crate::conexion::cliente::{Cliente, Contexto};
use crate::conexion::salto::{conectar_cadena, construir_cadena, Transporte};
use crate::conexion::{EventoConexion, FuenteContrasena};
use crate::modelo::Host;
use russh::client::Handle;

use super::{conexiones, sesiones, EstadoServidor};

/// Tiempo sin actividad tras el cual se cierra el canal de un host.
pub const INACTIVIDAD: Duration = Duration::from_secs(10 * 60);

/// Cada cuánto se revisan los canales inactivos.
const REVISION: Duration = Duration::from_secs(30);

/// Tamaño de bloque de las transferencias y de las copias temporales.
pub const BLOQUE: usize = 64 * 1024;

/// Plazo del saludo del subsistema SFTP: si el host no lo sirve, no contesta.
const HANDSHAKE_SFTP: Duration = Duration::from_secs(10);

/// Nombre del directorio de temporales dentro del directorio de ejecución.
const SUBDIR_TEMPORALES: &str = "tmp";

/// Sufijo de los ficheros que aún se están escribiendo.
pub const SUFIJO_PARCIAL: &str = ".magi-parcial";

/// Un canal SFTP abierto, compartido por todas las ventanas.
pub struct SftpHost {
    pub host_id: i64,
    pub host_nombre: String,
    pub sesion: Arc<SftpSession>,
    /// Conexión sobre la que vive el canal, para descontarla del pool solo si
    /// sigue siendo la misma.
    pub handle: Arc<Handle<Cliente>>,
    pub dir_inicio: String,
    pub solicitante: u32,
    pub ultima_actividad: Instant,
    /// Conexión propia del canal (host sin `multiplexar`): se cierra con él.
    /// Con la conexión en el pool basta con liberar el canal.
    pub propia: Option<Transporte>,
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

/// Prepara el directorio de temporales y borra lo que dejara una ejecución
/// anterior interrumpida.
pub fn preparar_temporales(rutas: &crate::config::Rutas) {
    let directorio = dir_temporales(rutas);
    if let Err(error) = std::fs::create_dir_all(&directorio) {
        warn!("no se pudo crear {}: {error}", directorio.display());
        return;
    }
    let _ = std::fs::set_permissions(
        &directorio,
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    );
    vaciar_temporales(rutas);
}

/// Vacía el directorio de temporales (al arrancar y al apagarse).
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
/// falta. Si no hay conexión viva, la abre con el flujo de la Fase 3 (huella,
/// frase y contraseña al solicitante). La apertura se serializa por host: dos
/// ventanas que pidan el mismo host a la vez comparten una única conexión.
pub async fn asegurar(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    host_id: i64,
    solicitante: u32,
) -> Result<(Arc<SftpSession>, String), String> {
    if let Some((sesion, dir)) = canal_vivo(estado, host_id).await {
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
        estado_bloqueado
            .aperturas_sftp
            .insert(id_solicitud, host_id);
        (host, todos_los_hosts, rutas, id_solicitud)
    };

    match abrir(
        estado,
        &host,
        &todos_los_hosts,
        &rutas,
        id_solicitud,
        solicitante,
    )
    .await
    {
        Ok((sesion, dir_inicio, propia, handle)) => {
            let mut estado_bloqueado = estado.lock().await;
            estado_bloqueado.sftp.insert(
                host_id,
                SftpHost {
                    host_id,
                    host_nombre: host.nombre.clone(),
                    sesion: sesion.clone(),
                    handle,
                    dir_inicio: dir_inicio.clone(),
                    solicitante,
                    ultima_actividad: Instant::now(),
                    propia,
                },
            );
            estado_bloqueado.aperturas_sftp.remove(&id_solicitud);
            info!(host = %host.nombre, dir = %dir_inicio, "canal SFTP abierto");
            Ok((sesion, dir_inicio))
        }
        Err(motivo) => {
            estado.lock().await.aperturas_sftp.remove(&id_solicitud);
            warn!(host = %host.nombre, "no se pudo abrir el canal SFTP: {motivo}");
            Err(motivo)
        }
    }
}

/// Canal ya abierto para el host, refrescando su marca de actividad.
async fn canal_vivo(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    host_id: i64,
) -> Option<(Arc<SftpSession>, String)> {
    let mut estado_bloqueado = estado.lock().await;
    let canal = estado_bloqueado.sftp.get_mut(&host_id)?;
    canal.ultima_actividad = Instant::now();
    Some((canal.sesion.clone(), canal.dir_inicio.clone()))
}

/// Sesión de un canal ya abierto, sin abrirlo si no lo está.
pub async fn canal_abierto(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    host_id: i64,
) -> Option<Arc<SftpSession>> {
    let mut estado_bloqueado = estado.lock().await;
    let canal = estado_bloqueado.sftp.get_mut(&host_id)?;
    canal.ultima_actividad = Instant::now();
    Some(canal.sesion.clone())
}

/// El canal de un host dejó de funcionar: se quita y sus transferencias pasan
/// a error, porque el siguiente `AbrirSftp` lo volverá a abrir.
pub async fn perdido(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>, host_id: i64) {
    let canal = estado.lock().await.sftp.remove(&host_id);
    let Some(canal) = canal else {
        return;
    };
    warn!(host = %canal.host_nombre, "se perdió el canal SFTP");
    if canal.propia.is_none() {
        estado
            .lock()
            .await
            .pool
            .liberar_si_es(host_id, &canal.handle);
    }
    super::transferencias::caida(estado, host_id).await;
}

/// Conecta (reutilizando el pool si se puede) y abre el subsistema `sftp`.
async fn abrir(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    host: &Host,
    todos_los_hosts: &HashMap<i64, Host>,
    rutas: &crate::config::Rutas,
    id_solicitud: u32,
    solicitante: u32,
) -> Result<
    (
        Arc<SftpSession>,
        String,
        Option<Transporte>,
        Arc<Handle<Cliente>>,
    ),
    String,
> {
    // Conexión: la del pool si la hay (el canal cuenta como canal suyo, así
    // que el pool no la cerrará mientras el SFTP viva) o una nueva.
    let (handle, propia) = {
        let mut estado_bloqueado = estado.lock().await;
        match estado_bloqueado.pool.reutilizar(host.id) {
            Some(handle) => (handle, None),
            None => {
                drop(estado_bloqueado);
                let (tx_eventos, rx_eventos) = mpsc::unbounded_channel::<EventoConexion>();
                // El puente traduce los diálogos al solicitante; se deja vivo
                // porque el handler de russh conserva un clon del canal
                // mientras la conexión viva (igual que en las sesiones).
                tokio::spawn(sesiones::puente_eventos(
                    estado.clone(),
                    id_solicitud,
                    solicitante,
                    rx_eventos,
                ));
                let cadena =
                    construir_cadena(host, todos_los_hosts).map_err(|error| error.to_string())?;
                let contexto = Contexto {
                    known_hosts: rutas.fichero_known_hosts(),
                    dir_ssh: rutas.dir_ssh(),
                    hogar: rutas.hogar.clone(),
                    usuario_local: crate::conexion::usuario_local(),
                    tx: tx_eventos.clone(),
                    interactivo: true,
                    fuente_contrasena: FuenteContrasena::Solicitante,
                };
                let transporte = conectar_cadena(&cadena, &contexto)
                    .await
                    .map_err(|error| error.to_string())?;
                let handle = transporte.handle.clone();
                if host.multiplexar {
                    let desplazada = {
                        let mut estado_bloqueado = estado.lock().await;
                        estado_bloqueado.pool.guardar(host.id, transporte)
                    };
                    // Si el pool ya tenía otra conexión para este host, se ha
                    // quedado fuera al sustituirla: se cierra aquí, que nadie
                    // más lo va a hacer.
                    if let Some((handle_viejo, saltos_viejos)) = desplazada {
                        conexiones::desconectar(handle_viejo, saltos_viejos).await;
                    }
                    (handle, None)
                } else {
                    (handle, Some(transporte))
                }
            }
        }
    };
    let conexion = handle.clone();

    // Abrir el canal y pedir el subsistema. Un host sin SFTP falla aquí.
    let canal = match handle.channel_open_session().await {
        Ok(canal) => canal,
        Err(error) => {
            liberar(estado, host.id, &conexion, propia).await;
            return Err(format!("no se pudo abrir el canal: {error}"));
        }
    };
    if let Err(error) = canal.request_subsystem(true, "sftp").await {
        liberar(estado, host.id, &conexion, propia).await;
        return Err(format!("el host no ofrece SFTP ({error})"));
    }
    // Un host sin subsistema `sftp` no falla al pedirlo: simplemente no
    // contesta al saludo del protocolo. Por eso hay plazo, como en DNS/TCP.
    let sesion =
        match tokio::time::timeout(HANDSHAKE_SFTP, SftpSession::new(canal.into_stream())).await {
            Ok(Ok(sesion)) => sesion,
            Ok(Err(error)) => {
                liberar(estado, host.id, &conexion, propia).await;
                return Err(format!("el host no ofrece SFTP ({error})"));
            }
            Err(_) => {
                liberar(estado, host.id, &conexion, propia).await;
                return Err("el host no ofrece SFTP".to_string());
            }
        };
    // El directorio de inicio del usuario remoto es el punto de partida.
    let dir_inicio = sesion
        .canonicalize(".")
        .await
        .unwrap_or_else(|_| "/".to_string());
    Ok((Arc::new(sesion), dir_inicio, propia, conexion))
}

/// Suelta lo que se hubiera tomado: el canal del pool (solo si sigue siendo
/// esa misma conexión) o la conexión propia del canal.
async fn liberar(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    host_id: i64,
    handle: &Arc<Handle<Cliente>>,
    propia: Option<Transporte>,
) {
    match propia {
        Some(transporte) => {
            conexiones::desconectar(
                transporte.handle,
                transporte.saltos.into_iter().map(Arc::new).collect(),
            )
            .await;
        }
        None => {
            estado.lock().await.pool.liberar_si_es(host_id, handle);
        }
    }
}

// ---------------------------------------------------------------- revision

/// Cierra los canales SFTP inactivos. Ninguna conexión de red se toca con el
/// bloqueo del estado tomado.
pub async fn revisar(estado: Arc<tokio::sync::Mutex<EstadoServidor>>) {
    loop {
        tokio::time::sleep(REVISION).await;
        let vencidos = {
            let mut estado_bloqueado = estado.lock().await;
            let ahora = Instant::now();
            let con_transferencias: Vec<i64> = estado_bloqueado
                .sftp
                .keys()
                .copied()
                .filter(|host_id| estado_bloqueado.transferencias.vivas_del_host(*host_id))
                .collect();
            let vencidos: Vec<i64> = estado_bloqueado
                .sftp
                .iter()
                .filter(|(host_id, canal)| {
                    puede_cerrarse(
                        canal.ultima_actividad,
                        ahora,
                        con_transferencias.contains(host_id),
                    )
                })
                .map(|(host_id, _)| *host_id)
                .collect();
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
            let _ = canal.sesion.close().await;
            let handle = canal.handle.clone();
            liberar(&estado, canal.host_id, &handle, canal.propia).await;
        }
    }
}

/// Cierra el canal de un host (conexión caída o apagado del servidor).
pub async fn cerrar(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    host_id: i64,
) -> Option<SftpHost> {
    let canal = estado.lock().await.sftp.remove(&host_id)?;
    let _ = canal.sesion.close().await;
    if canal.propia.is_none() {
        estado
            .lock()
            .await
            .pool
            .liberar_si_es(host_id, &canal.handle);
    }
    Some(canal)
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
        let _ = canal.sesion.close().await;
        if let Some(transporte) = canal.propia {
            conexiones::desconectar(
                transporte.handle,
                transporte.saltos.into_iter().map(Arc::new).collect(),
            )
            .await;
        }
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

/// `usuario:grupo` con lo que dé el protocolo: SFTP v3 solo manda números, así
/// que casi siempre se verá `1000:1000`.
fn propietario_de(metadata: &russh_sftp::protocol::FileAttributes) -> Option<String> {
    let usuario = metadata
        .user
        .clone()
        .or_else(|| metadata.uid.map(|uid| uid.to_string()));
    let grupo = metadata
        .group
        .clone()
        .or_else(|| metadata.gid.map(|gid| gid.to_string()));
    match (usuario, grupo) {
        (None, None) => None,
        (usuario, grupo) => Some(format!(
            "{}:{}",
            usuario.unwrap_or_else(|| "?".to_string()),
            grupo.unwrap_or_else(|| "?".to_string())
        )),
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

/// Copia un fichero remoto a un temporal local para verlo con `$PAGER`.
/// El directorio va en 700 y el fichero en 600, y va por bloques para no
/// cargar el fichero entero en memoria.
pub async fn copia_temporal(
    rutas: &crate::config::Rutas,
    sesion: &SftpSession,
    ruta_remota: &str,
    peticion_id: u64,
) -> Result<String, String> {
    preparar_temporales(rutas);
    let nombre = ruta_remota
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("fichero");
    let nombre = sanear_nombre(nombre);
    let destino = dir_temporales(rutas).join(format!("{peticion_id}-{nombre}"));

    let mut origen = sesion
        .open(ruta_remota)
        .await
        .map_err(|error| describir(error.to_string()))?;
    let mut salida = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&destino)
        .map_err(|error| format!("{}: {error}", destino.display()))?;

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
        || minusculas.contains("broken pipe")
        || minusculas.contains("connection reset")
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
        "fichero".to_string()
    } else {
        limpio
    }
}

/// Borra un temporal de los nuestros; si la ruta no está dentro del
/// directorio de temporales, no se toca nada.
pub fn borrar_temporal(rutas: &crate::config::Rutas, ruta: &str) -> Result<(), String> {
    let directorio = dir_temporales(rutas);
    let camino = Path::new(ruta);
    if !camino.starts_with(&directorio) {
        return Err("esa ruta no es un temporal de MAGI".to_string());
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
    exige_send_sync::<Transporte>();
    exige_send_sync::<Contexto>();
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
    }
}
