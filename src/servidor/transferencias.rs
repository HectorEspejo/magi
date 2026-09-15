//! Cola de transferencias del servidor (F4): una en curso por host y FIFO,
//! con hosts distintos avanzando en paralelo.
//!
//! Vive en el servidor porque es lo único de la vista Archivos que debe
//! sobrevivir a cerrar la ventana: todas las ventanas ven la misma cola. Aquí
//! no se dialoga nunca (T15): la política de conflicto y los avisos los decide
//! el cliente antes de encolar, y esta cola solo los aplica.

use std::io::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use russh_sftp::client::SftpSession;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::modelo::{fecha_ahora_epoca, ResultadoRegistro};
use crate::protocolo::{
    Direccion, ElementoTransferencia, EstadoTransferencia, InfoTransferencia, Politica,
};

use super::{sftp, EstadoServidor};

/// Cuánto se conserva una transferencia terminada antes de purgarla sola.
pub const RETENCION_TERMINADAS: i64 = 3600;

/// Cada cuánto se publica el progreso, como mucho (el lock es global).
const AVISO_PROGRESO: std::time::Duration = std::time::Duration::from_millis(100);

/// Plazo de un bloque de 64 KiB. Un host que deja de responder sin cerrar la
/// conexión (un cortafuegos que descarta, una máquina suspendida) no puede
/// dejar la transferencia en curso para siempre ni impedir el apagado.
const PLAZO_BLOQUE: std::time::Duration = std::time::Duration::from_secs(60);

/// Un elemento expandido de una transferencia: un directorio que hay que crear
/// o un fichero que hay que copiar, con su política ya resuelta.
#[derive(Debug, Clone, PartialEq)]
pub struct FicheroTransferencia {
    pub origen: String,
    pub destino: String,
    pub bytes: u64,
    pub es_dir: bool,
    pub politica: Politica,
}

/// Una transferencia en la cola, con su estado y su lista de ficheros.
pub struct Transferencia {
    pub info: InfoTransferencia,
    pub ficheros: Vec<FicheroTransferencia>,
    /// La levanta `CancelarTransferencia`; el motor la mira en cada bloque.
    pub cancelada: Arc<AtomicBool>,
}

impl Transferencia {
    pub fn cancelada(&self) -> bool {
        self.cancelada.load(Ordering::Relaxed)
    }
}

#[derive(Default)]
pub struct Cola {
    lista: Vec<Transferencia>,
    siguiente_id: u32,
}

impl Cola {
    /// Encola una transferencia ya expandida y devuelve su id (creciente).
    #[allow(clippy::too_many_arguments)]
    pub fn encolar(
        &mut self,
        host_id: i64,
        host_nombre: String,
        direccion: Direccion,
        solicitante: u32,
        elementos: &[ElementoTransferencia],
        borrar_origen: bool,
        ficheros: Vec<FicheroTransferencia>,
    ) -> u32 {
        self.siguiente_id += 1;
        let id = self.siguiente_id;
        let primer_elemento = elementos.first();
        let info = InfoTransferencia {
            id,
            host_id,
            host_nombre,
            direccion,
            estado: EstadoTransferencia::EnCola,
            origen: primer_elemento
                .map(|elemento| elemento.origen.clone())
                .unwrap_or_default(),
            destino: primer_elemento
                .map(|elemento| elemento.destino.clone())
                .unwrap_or_default(),
            es_directorio: primer_elemento
                .map(|elemento| elemento.es_directorio)
                .unwrap_or(false),
            ficheros_total: ficheros.iter().filter(|f| !f.es_dir).count() as u32,
            ficheros_hechos: 0,
            omitidos: 0,
            bytes_total: ficheros.iter().map(|fichero| fichero.bytes).sum(),
            bytes_hechos: 0,
            fichero_actual: None,
            error: None,
            borrar_origen,
            solicitante,
            creada_en: fecha_ahora_epoca(),
            terminada_en: None,
        };
        self.lista.push(Transferencia {
            info,
            ficheros,
            cancelada: Arc::new(AtomicBool::new(false)),
        });
        id
    }

    pub fn obtener(&self, id: u32) -> Option<&Transferencia> {
        self.lista
            .iter()
            .find(|transferencia| transferencia.info.id == id)
    }

    pub fn obtener_mut(&mut self, id: u32) -> Option<&mut Transferencia> {
        self.lista
            .iter_mut()
            .find(|transferencia| transferencia.info.id == id)
    }

    /// El host ya tiene una transferencia en curso.
    pub fn host_ocupado(&self, host_id: i64) -> bool {
        self.lista.iter().any(|transferencia| {
            transferencia.info.host_id == host_id
                && transferencia.info.estado == EstadoTransferencia::EnCurso
        })
    }

    /// Tiene algo en curso o en cola: mientras lo haya, el servidor no se
    /// apaga solo ni se cierra el canal SFTP del host.
    pub fn hay_vivas(&self) -> bool {
        self.lista
            .iter()
            .any(|transferencia| !transferencia.info.estado.terminada())
    }

    pub fn vivas_del_host(&self, host_id: i64) -> bool {
        self.lista.iter().any(|transferencia| {
            transferencia.info.host_id == host_id && !transferencia.info.estado.terminada()
        })
    }

    /// Pasa a `EnCurso` la primera en cola de cada host libre, y devuelve sus
    /// ids para que el llamador lance el motor (con el bloqueo ya suelto).
    pub fn despachar(&mut self) -> Vec<u32> {
        let mut lanzadas = Vec::new();
        let hosts: Vec<i64> = self
            .lista
            .iter()
            .filter(|transferencia| transferencia.info.estado == EstadoTransferencia::EnCola)
            .map(|transferencia| transferencia.info.host_id)
            .collect();
        for host_id in hosts {
            if self.host_ocupado(host_id) {
                continue;
            }
            let Some(indice) = self.lista.iter().position(|transferencia| {
                transferencia.info.host_id == host_id
                    && transferencia.info.estado == EstadoTransferencia::EnCola
            }) else {
                continue;
            };
            self.lista[indice].info.estado = EstadoTransferencia::EnCurso;
            lanzadas.push(self.lista[indice].info.id);
        }
        lanzadas
    }

    /// Marca una transferencia como cancelada. El motor lo nota en el
    /// siguiente bloque y borra el parcial.
    pub fn cancelar(&mut self, id: u32) -> bool {
        let Some(transferencia) = self.obtener_mut(id) else {
            return false;
        };
        if transferencia.info.estado.terminada() {
            return false;
        }
        transferencia.cancelada.store(true, Ordering::Relaxed);
        if transferencia.info.estado == EstadoTransferencia::EnCola {
            transferencia.info.estado = EstadoTransferencia::Cancelada;
            transferencia.info.terminada_en = Some(fecha_ahora_epoca());
        }
        true
    }

    /// Marca en error todas las que no estén terminadas: se usa al apagarse el
    /// servidor. Devuelve los hosts afectados.
    pub fn abortar_todas(&mut self, motivo: &str) -> Vec<i64> {
        let ahora = fecha_ahora_epoca();
        let mut hosts = Vec::new();
        for transferencia in &mut self.lista {
            if transferencia.info.estado.terminada() {
                continue;
            }
            transferencia.cancelada.store(true, Ordering::Relaxed);
            transferencia.info.estado = EstadoTransferencia::Error;
            transferencia.info.error = Some(motivo.to_string());
            transferencia.info.terminada_en = Some(ahora);
            if !hosts.contains(&transferencia.info.host_id) {
                hosts.push(transferencia.info.host_id);
            }
        }
        hosts
    }

    /// Marca en error las de un host: su conexión se cayó. Las que esperaban
    /// turno también caen, porque ya no hay por dónde copiarlas.
    pub fn abortar_host(&mut self, host_id: i64, motivo: &str) -> bool {
        let ahora = fecha_ahora_epoca();
        let mut afectadas = false;
        for transferencia in &mut self.lista {
            if transferencia.info.host_id != host_id || transferencia.info.estado.terminada() {
                continue;
            }
            transferencia.cancelada.store(true, Ordering::Relaxed);
            transferencia.info.estado = EstadoTransferencia::Error;
            transferencia.info.error = Some(motivo.to_string());
            transferencia.info.terminada_en = Some(ahora);
            afectadas = true;
        }
        afectadas
    }

    /// Quita de la cola las terminadas; devuelve cuántas.
    pub fn limpiar(&mut self) -> usize {
        let antes = self.lista.len();
        self.lista
            .retain(|transferencia| !transferencia.info.estado.terminada());
        antes - self.lista.len()
    }

    /// Quita las terminadas hace más de la retención; devuelve cuántas.
    pub fn purgar_caducadas(&mut self, ahora: i64) -> usize {
        let antes = self.lista.len();
        self.lista
            .retain(|transferencia| match transferencia.info.terminada_en {
                Some(terminada) => ahora - terminada < RETENCION_TERMINADAS,
                None => true,
            });
        antes - self.lista.len()
    }

    pub fn info(&self) -> Vec<InfoTransferencia> {
        self.lista
            .iter()
            .map(|transferencia| transferencia.info.clone())
            .collect()
    }

    /// Detalle para el registro: dirección, rutas, bytes y resultado. Nunca
    /// contenido.
    pub fn detalle_registro(&self, id: u32) -> Option<String> {
        let transferencia = self.obtener(id)?;
        let info = &transferencia.info;
        let omitidos = match info.omitidos {
            0 => String::new(),
            otros => format!(" · {otros} omitidos"),
        };
        let error = match &info.error {
            Some(error) => format!(" · {error}"),
            None => String::new(),
        };
        Some(format!(
            "{} {} · {} → {} · {} B · {} fichero(s){omitidos}{error}",
            info.direccion.texto(),
            info.estado.texto(),
            info.origen,
            info.destino,
            info.bytes_hechos,
            info.ficheros_hechos,
        ))
    }
}

// ---------------------------------------------------------------- encolado

/// Expande la petición del cliente, la encola y lanza las transferencias que
/// toquen. Es el único camino para encolar: el cliente nunca manda la cola
/// expandida de una bajada.
#[allow(clippy::too_many_arguments)]
pub async fn encolar(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    solicitante: u32,
    host_id: i64,
    direccion: Direccion,
    elementos: Vec<ElementoTransferencia>,
    politica: Politica,
    borrar_origen: bool,
) -> Result<u32, String> {
    if elementos.is_empty() {
        return Err("no hay nada que transferir".to_string());
    }
    let (host_nombre, sesion) = {
        let estado_bloqueado = estado.lock().await;
        let Some(canal) = estado_bloqueado.sftp.get(&host_id) else {
            return Err("no hay canal SFTP abierto para ese host".to_string());
        };
        (canal.host_nombre.clone(), canal.sesion.clone())
    };

    let (ficheros, enlaces_omitidos) = expandir(&sesion, direccion, &elementos, politica).await?;

    let id = {
        let mut estado_bloqueado = estado.lock().await;
        let id = estado_bloqueado.transferencias.encolar(
            host_id,
            host_nombre,
            direccion,
            solicitante,
            &elementos,
            borrar_origen,
            ficheros,
        );
        // Los enlaces a directorio que se dejaron atrás al expandir cuentan
        // como omitidos, para que el detalle lo diga.
        if enlaces_omitidos > 0 {
            if let Some(transferencia) = estado_bloqueado.transferencias.obtener_mut(id) {
                transferencia.info.omitidos = enlaces_omitidos;
            }
        }
        estado_bloqueado.difusion_cola.marcar(true);
        id
    };
    avisar(estado).await;
    Ok(id)
}

/// Despierta al supervisor de la cola para que arranque lo que pueda empezar.
pub async fn avisar(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>) {
    let tx = estado.lock().await.tx_cola.clone();
    if let Some(tx) = tx {
        let _ = tx.send(());
    }
}

/// Supervisor de la cola: arranca las transferencias que puedan empezar (una
/// por host libre). Es una tarea aparte y no recursiva a propósito: el motor
/// no puede lanzarse a sí mismo o el compilador no sabe probar que su futuro
/// es `Send`.
pub async fn supervisor(
    estado: Arc<tokio::sync::Mutex<EstadoServidor>>,
    mut rx: mpsc::UnboundedReceiver<()>,
) {
    loop {
        tokio::select! {
            aviso = rx.recv() => {
                if aviso.is_none() {
                    return;
                }
            }
            // Red de seguridad: si un motor muriera sin avisar, la cola no se
            // queda parada para siempre.
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {}
        }
        let lanzadas = {
            let mut estado_bloqueado = estado.lock().await;
            let lanzadas = estado_bloqueado.transferencias.despachar();
            if !lanzadas.is_empty() {
                estado_bloqueado.difusion_cola.marcar(true);
            }
            lanzadas
        };
        for id in lanzadas {
            tokio::spawn(tarea(estado.clone(), id));
        }
    }
}

/// Construye la lista de ficheros a copiar. En una subida ya viene expandida
/// del cliente (que es quien ve el disco local); en una bajada la expande el
/// servidor con `read_dir` recursivo.
async fn expandir(
    sesion: &SftpSession,
    direccion: Direccion,
    elementos: &[ElementoTransferencia],
    politica: Politica,
) -> Result<(Vec<FicheroTransferencia>, u32), String> {
    let mut ficheros = Vec::new();
    let mut omitidos = 0u32;
    for elemento in elementos {
        let politica = elemento.politica.unwrap_or(politica);
        if !elemento.es_directorio {
            ficheros.push(FicheroTransferencia {
                origen: elemento.origen.clone(),
                destino: elemento.destino.clone(),
                bytes: elemento.bytes,
                es_dir: false,
                politica,
            });
            continue;
        }
        ficheros.push(FicheroTransferencia {
            origen: elemento.origen.clone(),
            destino: elemento.destino.clone(),
            bytes: 0,
            es_dir: true,
            politica,
        });
        match direccion {
            Direccion::Subida => {
                // El cliente ya manda el contenido de los directorios: si aquí
                // solo viene el directorio, es que está vacío.
            }
            Direccion::Bajada => {
                expandir_remoto(
                    sesion,
                    &elemento.origen,
                    &elemento.destino,
                    politica,
                    &mut ficheros,
                    &mut omitidos,
                )
                .await?;
            }
        }
    }
    Ok((ficheros, omitidos))
}

/// Recorre un directorio remoto en anchura y añade sus ficheros.
async fn expandir_remoto(
    sesion: &SftpSession,
    origen: &str,
    destino: &str,
    politica: Politica,
    ficheros: &mut Vec<FicheroTransferencia>,
    omitidos: &mut u32,
) -> Result<(), String> {
    let entradas = sftp::listar(sesion, origen).await?;
    for entrada in entradas {
        let origen_hijo = sftp::join(origen, &entrada.nombre);
        let destino_hijo = sftp::join(destino, &entrada.nombre);
        match entrada.tipo {
            crate::archivos::TipoEntrada::Directorio => {
                ficheros.push(FicheroTransferencia {
                    origen: origen_hijo.clone(),
                    destino: destino_hijo.clone(),
                    bytes: 0,
                    es_dir: true,
                    politica,
                });
                Box::pin(expandir_remoto(
                    sesion,
                    &origen_hijo,
                    &destino_hijo,
                    politica,
                    ficheros,
                    omitidos,
                ))
                .await?;
            }
            crate::archivos::TipoEntrada::Fichero => {
                ficheros.push(FicheroTransferencia {
                    origen: origen_hijo,
                    destino: destino_hijo,
                    bytes: entrada.tamano,
                    es_dir: false,
                    politica,
                });
            }
            // Un enlace se copia como el fichero al que apunta; si apunta a
            // un directorio se omite (seguirlo podría duplicar árboles
            // enteros) y se cuenta para que el detalle lo diga.
            crate::archivos::TipoEntrada::Enlace => {
                let apunta_a_dir = sesion
                    .metadata(&origen_hijo)
                    .await
                    .map(|metadata| metadata.is_dir())
                    .unwrap_or(false);
                if apunta_a_dir {
                    *omitidos += 1;
                    continue;
                }
                ficheros.push(FicheroTransferencia {
                    origen: origen_hijo,
                    destino: destino_hijo,
                    bytes: entrada.tamano,
                    es_dir: false,
                    politica,
                });
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- motor

/// Copia una transferencia entera, fichero a fichero, y deja el estado final
/// en la cola. Nunca toca la red con el bloqueo del estado tomado.
pub async fn tarea(estado: Arc<tokio::sync::Mutex<EstadoServidor>>, id: u32) {
    let (host_id, direccion, ficheros, cancelada, borrar_origen) = {
        let estado_bloqueado = estado.lock().await;
        let Some(transferencia) = estado_bloqueado.transferencias.obtener(id) else {
            return;
        };
        (
            transferencia.info.host_id,
            transferencia.info.direccion,
            transferencia.ficheros.clone(),
            transferencia.cancelada.clone(),
            transferencia.info.borrar_origen,
        )
    };

    let sesion = {
        let estado_bloqueado = estado.lock().await;
        estado_bloqueado
            .sftp
            .get(&host_id)
            .map(|canal| canal.sesion.clone())
    };
    let Some(sesion) = sesion else {
        terminar(
            &estado,
            id,
            EstadoTransferencia::Error,
            Some("conexión caída".to_string()),
            0,
            0,
        )
        .await;
        return;
    };

    let mut hechos = 0u64;
    let mut ficheros_hechos = 0u32;
    // Lo que se ha copiado de verdad: con `borrar_origen` solo se borra esto,
    // nunca los orígenes que se omitieron (no se copiaron, no se pueden
    // perder).
    let mut copiados: Vec<FicheroTransferencia> = Vec::new();
    // Los omitidos al expandir (enlaces a directorio) ya cuentan desde el
    // principio y no se pueden perder al terminar.
    let mut omitidos = {
        let estado_bloqueado = estado.lock().await;
        estado_bloqueado
            .transferencias
            .obtener(id)
            .map(|transferencia| transferencia.info.omitidos)
            .unwrap_or(0)
    };
    let mut ultimo_aviso = Instant::now();
    let mut fallo: Option<String> = None;
    let mut cancelada_por_el_usuario = false;

    for fichero in &ficheros {
        if cancelada.load(Ordering::Relaxed) {
            cancelada_por_el_usuario = true;
            break;
        }
        if fichero.es_dir {
            let resultado = match direccion {
                Direccion::Subida => sftp::crear_dir_padres(&sesion, &fichero.destino).await,
                Direccion::Bajada => std::fs::create_dir_all(&fichero.destino)
                    .map_err(|error| format!("{}: {error}", fichero.destino)),
            };
            if let Err(motivo) = resultado {
                fallo = Some(motivo);
                break;
            }
            continue;
        }

        // El destino puede haberse creado o borrado desde que el cliente
        // miró: se recomprueba justo antes de escribir.
        let existe = match direccion {
            Direccion::Subida => sftp::existe(&sesion, &fichero.destino).await,
            Direccion::Bajada => Path::new(&fichero.destino).exists(),
        };
        if existe && fichero.politica == Politica::Omitir {
            omitidos += 1;
            continue;
        }

        avisar_progreso(
            &estado,
            id,
            &fichero.origen,
            hechos,
            ficheros_hechos,
            omitidos,
        )
        .await;
        match copiar(&sesion, direccion, fichero, &cancelada).await {
            Ok(ResultadoCopia::Hecho(bytes)) => {
                hechos += bytes;
                ficheros_hechos += 1;
                copiados.push(fichero.clone());
            }
            Ok(ResultadoCopia::Cancelada) => {
                cancelada_por_el_usuario = true;
                break;
            }
            Err(motivo) => {
                fallo = Some(motivo);
                break;
            }
        }
        if ultimo_aviso.elapsed() >= AVISO_PROGRESO {
            avisar_progreso(
                &estado,
                id,
                &fichero.origen,
                hechos,
                ficheros_hechos,
                omitidos,
            )
            .await;
            ultimo_aviso = Instant::now();
        }
    }

    // El origen solo se borra cuando la copia ha terminado bien; nunca antes.
    let estado_final = if fallo.is_some() {
        EstadoTransferencia::Error
    } else if cancelada_por_el_usuario {
        EstadoTransferencia::Cancelada
    } else {
        EstadoTransferencia::Hecha
    };
    if estado_final == EstadoTransferencia::Hecha && borrar_origen && direccion == Direccion::Bajada
    {
        if let Err(error) = borrar_origenes(&sesion, &copiados).await {
            warn!(transferencia = id, "no se pudo borrar el origen: {error}");
        }
    }
    terminar(&estado, id, estado_final, fallo, ficheros_hechos, omitidos).await;
    avisar(&estado).await;
}

/// Borra los orígenes remotos de una bajada con `borrar_origen`.
async fn borrar_origenes(
    sesion: &SftpSession,
    ficheros: &[FicheroTransferencia],
) -> Result<(), String> {
    // De dentro hacia fuera: primero los ficheros, luego los directorios.
    for fichero in ficheros.iter().rev() {
        if fichero.es_dir {
            let _ = sesion.remove_dir(&fichero.origen).await;
        } else {
            sftp::borrar(sesion, &fichero.origen).await?;
        }
    }
    Ok(())
}

enum ResultadoCopia {
    Hecho(u64),
    Cancelada,
}

/// Copia un fichero por bloques de 64 KiB, con un parcial que se renombra al
/// terminar. La cancelación se comprueba en cada bloque.
async fn copiar(
    sesion: &SftpSession,
    direccion: Direccion,
    fichero: &FicheroTransferencia,
    cancelada: &AtomicBool,
) -> Result<ResultadoCopia, String> {
    let resultado = match direccion {
        Direccion::Bajada => copiar_bajada(sesion, fichero, cancelada).await,
        Direccion::Subida => copiar_subida(sesion, fichero, cancelada).await,
    };
    // Un fallo a medias no puede dejar el parcial tirado en el destino: se
    // borra aquí, en el único sitio por el que pasan todas las salidas.
    if resultado.is_err() {
        let parcial = format!("{}{}", fichero.destino, sftp::SUFIJO_PARCIAL);
        match direccion {
            Direccion::Bajada => {
                let _ = std::fs::remove_file(&parcial);
            }
            Direccion::Subida => {
                let _ = sesion.remove_file(&parcial).await;
            }
        }
    }
    resultado
}

async fn copiar_bajada(
    sesion: &SftpSession,
    fichero: &FicheroTransferencia,
    cancelada: &AtomicBool,
) -> Result<ResultadoCopia, String> {
    let parcial = format!("{}{}", fichero.destino, sftp::SUFIJO_PARCIAL);
    if let Some(padre) = Path::new(&parcial).parent() {
        std::fs::create_dir_all(padre).map_err(|error| format!("{}: {error}", padre.display()))?;
    }
    let mut origen = sesion
        .open(&fichero.origen)
        .await
        .map_err(|error| sftp::describir(error.to_string()))?;
    let mut salida =
        std::fs::File::create(&parcial).map_err(|error| format!("{parcial}: {error}"))?;

    let mut bufer = vec![0u8; sftp::BLOQUE];
    let mut total = 0u64;
    loop {
        if cancelada.load(Ordering::Relaxed) {
            drop(salida);
            let _ = std::fs::remove_file(&parcial);
            return Ok(ResultadoCopia::Cancelada);
        }
        let leidos = match tokio::time::timeout(PLAZO_BLOQUE, origen.read(&mut bufer)).await {
            Ok(Ok(leidos)) => leidos,
            Ok(Err(error)) => return Err(format!("leyendo {}: {error}", fichero.origen)),
            Err(_) => {
                return Err(format!(
                    "el host dejó de responder leyendo {}",
                    fichero.origen
                ))
            }
        };
        if leidos == 0 {
            break;
        }
        salida
            .write_all(&bufer[..leidos])
            .map_err(|error| format!("escribiendo {parcial}: {error}"))?;
        total += leidos as u64;
    }
    salida
        .flush()
        .map_err(|error| format!("escribiendo {parcial}: {error}"))?;
    drop(salida);

    if let Ok(metadata) = sesion.metadata(&fichero.origen).await {
        if let Some(mtime) = metadata.mtime {
            let momento = filetime::FileTime::from_unix_time(i64::from(mtime), 0);
            let _ = filetime::set_file_mtime(&parcial, momento);
        }
    }
    std::fs::rename(&parcial, &fichero.destino)
        .map_err(|error| format!("{}: {error}", fichero.destino))?;
    Ok(ResultadoCopia::Hecho(total))
}

async fn copiar_subida(
    sesion: &SftpSession,
    fichero: &FicheroTransferencia,
    cancelada: &AtomicBool,
) -> Result<ResultadoCopia, String> {
    let parcial = format!("{}{}", fichero.destino, sftp::SUFIJO_PARCIAL);
    sftp::crear_dir_padres(sesion, &sftp::padre(&parcial)).await?;
    let mut entrada = tokio::fs::File::open(&fichero.origen)
        .await
        .map_err(|error| format!("{}: {error}", fichero.origen))?;
    let mut salida = sesion
        .open_with_flags(&parcial, sftp::banderas_escritura())
        .await
        .map_err(|error| sftp::describir(error.to_string()))?;

    let mut bufer = vec![0u8; sftp::BLOQUE];
    let mut total = 0u64;
    loop {
        if cancelada.load(Ordering::Relaxed) {
            drop(salida);
            let _ = sesion.remove_file(&parcial).await;
            return Ok(ResultadoCopia::Cancelada);
        }
        let leidos = entrada
            .read(&mut bufer)
            .await
            .map_err(|error| format!("leyendo {}: {error}", fichero.origen))?;
        if leidos == 0 {
            break;
        }
        match tokio::time::timeout(PLAZO_BLOQUE, salida.write_all(&bufer[..leidos])).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => return Err(format!("escribiendo {parcial}: {error}")),
            Err(_) => return Err(format!("el host dejó de responder escribiendo {parcial}")),
        }
        total += leidos as u64;
    }
    salida
        .shutdown()
        .await
        .map_err(|error| format!("cerrando {parcial}: {error}"))?;
    drop(salida);

    if let Ok(metadata) = std::fs::metadata(&fichero.origen) {
        let mtime = crate::archivos::local::mtime_de(&metadata);
        if mtime > 0 {
            let atributos = russh_sftp::protocol::FileAttributes {
                mtime: Some(mtime as u32),
                atime: Some(mtime as u32),
                ..Default::default()
            };
            let _ = sesion.set_metadata(&parcial, atributos).await;
        }
    }
    sesion
        .rename(&parcial, &fichero.destino)
        .await
        .map_err(|error| sftp::describir(error.to_string()))?;
    Ok(ResultadoCopia::Hecho(total))
}

/// Publica el progreso con el bloqueo tomado y lo suelta enseguida; el envío
/// lo hace la revisora de la cola, que coalesce a cuatro por segundo.
async fn avisar_progreso(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    id: u32,
    fichero_actual: &str,
    bytes_hechos: u64,
    ficheros_hechos: u32,
    omitidos: u32,
) {
    let mut estado_bloqueado = estado.lock().await;
    let mut host_id = None;
    if let Some(transferencia) = estado_bloqueado.transferencias.obtener_mut(id) {
        transferencia.info.bytes_hechos = bytes_hechos;
        transferencia.info.ficheros_hechos = ficheros_hechos;
        transferencia.info.omitidos = omitidos;
        transferencia.info.fichero_actual = Some(fichero_actual.to_string());
        host_id = Some(transferencia.info.host_id);
    }
    if let Some(host_id) = host_id {
        if let Some(canal) = estado_bloqueado.sftp.get_mut(&host_id) {
            canal.ultima_actividad = Instant::now();
        }
    }
    estado_bloqueado.difusion_cola.marcar(false);
}

/// Deja el estado final de la transferencia en la cola y lo anota en REGISTRO.
async fn terminar(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    id: u32,
    estado_final: EstadoTransferencia,
    fallo: Option<String>,
    ficheros_hechos: u32,
    omitidos: u32,
) {
    let mut estado_bloqueado = estado.lock().await;
    {
        let Some(transferencia) = estado_bloqueado.transferencias.obtener_mut(id) else {
            return;
        };
        transferencia.info.estado = estado_final;
        transferencia.info.terminada_en = Some(fecha_ahora_epoca());
        transferencia.info.error = fallo.clone();
        transferencia.info.omitidos = omitidos;
        transferencia.info.fichero_actual = None;
        transferencia.info.ficheros_hechos = ficheros_hechos;
        if estado_final == EstadoTransferencia::Hecha {
            transferencia.info.bytes_hechos = transferencia.info.bytes_total;
            transferencia.info.ficheros_hechos = transferencia.info.ficheros_total;
        }
    }
    let detalle = estado_bloqueado.transferencias.detalle_registro(id);
    let host_id = estado_bloqueado
        .transferencias
        .obtener(id)
        .map(|transferencia| transferencia.info.host_id);
    estado_bloqueado.difusion_cola.marcar(true);
    let resultado = if estado_final == EstadoTransferencia::Error {
        ResultadoRegistro::Error
    } else {
        ResultadoRegistro::Ok
    };
    let bd = estado_bloqueado.bd.clone();
    drop(estado_bloqueado);
    if let (Some(detalle), Some(host_id)) = (detalle, host_id) {
        // El efecto ya se ha producido: se anota ahora, no al decidir.
        let _ = bd.send(super::OrdenBd::Anotar {
            tipo: crate::registro::TRANSFERENCIA.to_string(),
            host_id: Some(host_id),
            identidad_id: None,
            detalle,
            resultado,
        });
    }
    info!(
        transferencia = id,
        estado = estado_final.texto(),
        "transferencia terminada"
    );
}

/// El host perdió la conexión: sus transferencias pasan a error y la cola
/// sigue con las demás.
pub async fn caida(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>, host_id: i64) {
    let afectadas = {
        let mut estado_bloqueado = estado.lock().await;
        let afectadas = estado_bloqueado
            .transferencias
            .abortar_host(host_id, "conexión caída");
        if afectadas {
            estado_bloqueado.difusion_cola.marcar(true);
        }
        afectadas
    };
    if afectadas {
        warn!(
            host = host_id,
            "transferencias abortadas por caída de la conexión"
        );
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn elemento(origen: &str, destino: &str, bytes: u64) -> ElementoTransferencia {
        ElementoTransferencia {
            origen: origen.to_string(),
            destino: destino.to_string(),
            bytes,
            es_directorio: false,
            politica: None,
        }
    }

    fn fichero(origen: &str, destino: &str, bytes: u64) -> FicheroTransferencia {
        FicheroTransferencia {
            origen: origen.to_string(),
            destino: destino.to_string(),
            bytes,
            es_dir: false,
            politica: Politica::Sobrescribir,
        }
    }

    fn encolar(cola: &mut Cola, host_id: i64, origen: &str) -> u32 {
        let elementos = vec![elemento(origen, "/destino", 100)];
        cola.encolar(
            host_id,
            format!("host-{host_id}"),
            Direccion::Subida,
            1,
            &elementos,
            false,
            vec![fichero(origen, "/destino", 100)],
        )
    }

    #[test]
    fn la_cola_despacha_una_transferencia_por_host_y_en_orden_fifo() {
        let mut cola = Cola::default();
        let primera = encolar(&mut cola, 1, "a");
        let segunda = encolar(&mut cola, 1, "b");
        let tercera = encolar(&mut cola, 2, "c");

        let lanzadas = cola.despachar();
        assert_eq!(lanzadas, vec![primera, tercera], "una por host, la primera");
        assert_eq!(
            cola.obtener(primera).unwrap().info.estado,
            EstadoTransferencia::EnCurso
        );
        assert_eq!(
            cola.obtener(segunda).unwrap().info.estado,
            EstadoTransferencia::EnCola
        );

        // Al terminar la del host 1, arranca la que esperaba.
        cola.obtener_mut(primera).unwrap().info.estado = EstadoTransferencia::Hecha;
        assert_eq!(cola.despachar(), vec![segunda]);
    }

    #[test]
    fn el_total_se_calcula_de_la_lista_expandida() {
        let mut cola = Cola::default();
        let mut directorio = elemento("static", "/var/www/static", 0);
        directorio.es_directorio = true;
        let elementos = vec![directorio];
        let id = cola.encolar(
            1,
            "host".to_string(),
            Direccion::Subida,
            1,
            &elementos,
            false,
            vec![
                FicheroTransferencia {
                    origen: "static".to_string(),
                    destino: "/var/www/static".to_string(),
                    bytes: 0,
                    es_dir: true,
                    politica: Politica::Omitir,
                },
                fichero("static/a.css", "/var/www/static/a.css", 300),
                fichero("static/b.css", "/var/www/static/b.css", 200),
            ],
        );
        let info = &cola.obtener(id).unwrap().info;
        assert_eq!(info.bytes_total, 500);
        assert_eq!(info.ficheros_total, 2, "los directorios no cuentan");
        assert!(info.es_directorio);
    }

    #[test]
    fn una_transferencia_en_cola_se_cancela_al_instante() {
        let mut cola = Cola::default();
        encolar(&mut cola, 1, "a");
        let segunda = encolar(&mut cola, 1, "b");
        cola.despachar();

        assert!(cola.cancelar(segunda));
        let transferencia = cola.obtener(segunda).unwrap();
        assert_eq!(transferencia.info.estado, EstadoTransferencia::Cancelada);
        assert!(transferencia.info.terminada_en.is_some());
        assert!(!cola.cancelar(segunda), "ya está terminada");
    }

    #[test]
    fn una_transferencia_en_curso_se_marca_para_que_el_motor_pare() {
        let mut cola = Cola::default();
        let id = encolar(&mut cola, 1, "a");
        cola.despachar();

        assert!(cola.cancelar(id));
        let transferencia = cola.obtener(id).unwrap();
        assert!(transferencia.cancelada(), "el motor lo verá en el bloque");
        assert_eq!(
            transferencia.info.estado,
            EstadoTransferencia::EnCurso,
            "el estado lo cierra el motor al borrar el parcial"
        );
    }

    #[test]
    fn la_caida_de_la_conexion_marca_en_error_las_del_host() {
        let mut cola = Cola::default();
        let uno = encolar(&mut cola, 1, "a");
        let dos = encolar(&mut cola, 2, "b");
        cola.despachar();

        assert!(cola.abortar_host(1, "conexión caída"));
        assert_eq!(
            cola.obtener(uno).unwrap().info.estado,
            EstadoTransferencia::Error
        );
        assert_eq!(
            cola.obtener(uno).unwrap().info.error.as_deref(),
            Some("conexión caída")
        );
        assert_eq!(
            cola.obtener(dos).unwrap().info.estado,
            EstadoTransferencia::EnCurso,
            "las de otros hosts siguen"
        );
    }

    #[test]
    fn las_terminadas_caducan_al_pasar_una_hora_y_limpiar_las_quita_antes() {
        let mut cola = Cola::default();
        let id = encolar(&mut cola, 1, "a");
        cola.obtener_mut(id).unwrap().info.estado = EstadoTransferencia::Hecha;
        cola.obtener_mut(id).unwrap().info.terminada_en = Some(1_000);

        assert_eq!(cola.purgar_caducadas(1_000 + RETENCION_TERMINADAS - 1), 0);
        assert_eq!(cola.info().len(), 1);
        assert_eq!(cola.purgar_caducadas(1_000 + RETENCION_TERMINADAS), 1);

        let otra = encolar(&mut cola, 2, "b");
        cola.obtener_mut(otra).unwrap().info.estado = EstadoTransferencia::Error;
        assert_eq!(cola.limpiar(), 1);
    }

    #[test]
    fn limpiar_no_toca_las_vivas_y_hay_vivas_lo_dice() {
        let mut cola = Cola::default();
        let terminada = encolar(&mut cola, 1, "a");
        let viva = encolar(&mut cola, 2, "b");
        cola.obtener_mut(terminada).unwrap().info.estado = EstadoTransferencia::Cancelada;

        assert!(cola.hay_vivas());
        assert!(cola.vivas_del_host(2));
        assert!(!cola.vivas_del_host(1));
        assert_eq!(cola.limpiar(), 1);
        assert_eq!(cola.info().len(), 1);
        assert_eq!(cola.info()[0].id, viva);
    }

    #[test]
    fn el_detalle_del_registro_lleva_direccion_rutas_bytes_y_omitidos() {
        let mut cola = Cola::default();
        let id = encolar(&mut cola, 1, "static");
        {
            let transferencia = cola.obtener_mut(id).unwrap();
            transferencia.info.estado = EstadoTransferencia::Hecha;
            transferencia.info.omitidos = 2;
        }
        let detalle = cola.detalle_registro(id).unwrap();
        assert!(detalle.contains("subida"), "{detalle}");
        assert!(detalle.contains("static"), "{detalle}");
        assert!(detalle.contains("2 omitidos"), "{detalle}");
    }
}
