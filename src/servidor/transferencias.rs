//! Cola de transferencias del servidor (F4): una en curso por host y FIFO,
//! con hosts distintos avanzando en paralelo.
//!
//! Vive en el servidor porque es lo único de la vista Archivos que debe
//! sobrevivir a cerrar la ventana: todas las ventanas ven la misma cola. Aquí
//! no se dialoga nunca (T15): la política de conflicto y los avisos los decide
//! el cliente antes de encolar, y esta cola solo los aplica.
//!
//! Fase 8: un elemento puede llevar permisos (se aplican tras el `rename` del
//! parcial), una transferencia puede llevar la deliberación que la autoriza y,
//! si ejecuta una sincronización, llega con la lista plana del plan y las rutas
//! que se borran en el destino solo si la copia termina sin errores (D95).
//! Cada transferencia se cierra una sola vez: su fila de `REGISTRO`
//! (`transferencia` o `sincronizacion`), el `ultimo_resultado` de la guardada y
//! el cierre de la deliberación.

use std::collections::HashSet;
use std::io::Write as _;
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use russh_sftp::client::SftpSession;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::deliberacion::EjecucionResultado;
use crate::modelo::{fecha_ahora_epoca, ResultadoRegistro, ResultadoSincronizacion};
use crate::protocolo::{
    DeliberacionLanzada, Direccion, ElementoTransferencia, EstadoTransferencia,
    EtiquetaTransferencia, InfoTransferencia, Politica, SincronizacionLanzada,
};

use super::ejecuciones::{self, DeliberacionFijada};
use super::{sftp, EstadoServidor, OrdenBd};

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
    /// Permisos que se aplican al destino tras el `rename` (o al crear el
    /// directorio).
    pub permisos: Option<u32>,
}

/// Una `Transferir` tal como llega del cliente.
#[derive(Debug, Clone)]
pub struct PeticionTransferir {
    pub host_id: i64,
    pub direccion: Direccion,
    pub elementos: Vec<ElementoTransferencia>,
    pub politica: Politica,
    pub borrar_origen: bool,
    pub peticion_id: Option<u64>,
    pub borrar_al_terminar: Vec<String>,
    pub deliberacion: Option<DeliberacionLanzada>,
    pub sincronizacion: Option<SincronizacionLanzada>,
    pub etiqueta: Option<EtiquetaTransferencia>,
}

/// Lo que una transferencia lleva además de sus ficheros: lo que se borra al
/// terminar bien y lo que hay que anotar al cerrar. Se fija al encolar y no se
/// relee (T33).
#[derive(Debug, Clone, Default)]
pub struct ExtrasTransferencia {
    pub borrar_al_terminar: Vec<String>,
    pub deliberacion: Option<super::ejecuciones::DeliberacionFijada>,
    pub sincronizacion: Option<SincronizacionLanzada>,
}

/// Una transferencia en la cola, con su estado y su lista de ficheros.
pub struct Transferencia {
    pub info: InfoTransferencia,
    pub ficheros: Vec<FicheroTransferencia>,
    pub extras: ExtrasTransferencia,
    /// La levanta `CancelarTransferencia`; el motor la mira en cada bloque.
    pub cancelada: Arc<AtomicBool>,
    /// El motor ya cambió algo en el destino (un fichero en su sitio, un
    /// directorio creado, un borrado): separa «parcial» de «error».
    pub escrito: Arc<AtomicBool>,
    /// Un motor la copia (o la copió): es él quien la cierra al terminar.
    lanzada: bool,
    /// La terminó `abortar_*` (conexión caída, apagado): la marca de
    /// cancelación no es del usuario y el estado final es ese error.
    abortada: bool,
    /// Su cierre (REGISTRO, `ultimo_resultado`, deliberación) ya salió: nunca
    /// sale dos veces, y sin él la fila no se quita de la cola.
    cerrada: bool,
    /// Lo que dejó el borrado al terminar, para el detalle del cierre.
    conservados: u32,
    fallos_borrado: u32,
}

impl Transferencia {
    pub fn cancelada(&self) -> bool {
        self.cancelada.load(Ordering::Relaxed)
    }
}

/// Lo que hay que escribir al cerrar una transferencia, una sola vez: su fila
/// de `REGISTRO`, el `ultimo_resultado` de una sincronización guardada y el
/// cierre de la deliberación que la autorizó. Se emite sin el bloqueo del
/// estado, por el hilo escritor.
#[derive(Debug, Clone)]
pub struct CierreTransferencia {
    pub host_id: i64,
    /// Tipo, detalle y resultado. Ninguna para una transferencia normal que
    /// no llegó a arrancar (cancelada o abortada en cola): no tuvo efecto.
    pub anotacion: Option<(&'static str, String, ResultadoRegistro)>,
    pub resultado: ResultadoSincronizacion,
    pub sincronizacion_id: Option<i64>,
    pub deliberacion: Option<DeliberacionFijada>,
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
        peticion_id: Option<u64>,
        etiqueta: Option<EtiquetaTransferencia>,
        extras: ExtrasTransferencia,
    ) -> u32 {
        self.siguiente_id += 1;
        let id = self.siguiente_id;
        let (origen, destino, es_directorio) = match &extras.sincronizacion {
            // Una sincronización se enseña por sus raíces, no por su primer
            // elemento (que puede no tener: un plan con solo borrados).
            Some(sincronizacion) => (
                sincronizacion.raiz_origen.clone(),
                sincronizacion.raiz_destino.clone(),
                true,
            ),
            None => {
                let primer_elemento = elementos.first();
                (
                    primer_elemento
                        .map(|elemento| elemento.origen.clone())
                        .unwrap_or_default(),
                    primer_elemento
                        .map(|elemento| elemento.destino.clone())
                        .unwrap_or_default(),
                    primer_elemento
                        .map(|elemento| elemento.es_directorio)
                        .unwrap_or(false),
                )
            }
        };
        let info = InfoTransferencia {
            id,
            host_id,
            host_nombre,
            direccion,
            estado: EstadoTransferencia::EnCola,
            origen,
            destino,
            es_directorio,
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
            peticion_id,
            etiqueta,
            borrados: 0,
            borrados_total: extras.borrar_al_terminar.len() as u32,
        };
        self.lista.push(Transferencia {
            info,
            ficheros,
            extras,
            cancelada: Arc::new(AtomicBool::new(false)),
            escrito: Arc::new(AtomicBool::new(false)),
            lanzada: false,
            abortada: false,
            cerrada: false,
            conservados: 0,
            fallos_borrado: 0,
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

    /// El host ya tiene una transferencia en curso (copiando o borrando).
    pub fn host_ocupado(&self, host_id: i64) -> bool {
        self.lista.iter().any(|transferencia| {
            transferencia.info.host_id == host_id
                && matches!(
                    transferencia.info.estado,
                    EstadoTransferencia::EnCurso | EstadoTransferencia::Borrando
                )
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

    /// ¿Lleva alguna transferencia de la cola esta deliberación? Las
    /// terminadas cuentan mientras sigan en la cola: su cierre ya va camino
    /// de `DELIBERACIONES`.
    pub fn deliberacion_usada(&self, deliberacion_id: i64) -> bool {
        self.lista.iter().any(|transferencia| {
            transferencia
                .extras
                .deliberacion
                .as_ref()
                .is_some_and(|fijada| fijada.registro.id == deliberacion_id)
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
            self.lista[indice].lanzada = true;
            lanzadas.push(self.lista[indice].info.id);
        }
        lanzadas
    }

    /// Marca una transferencia como cancelada. El motor lo nota en el
    /// siguiente bloque y borra el parcial. Una que esperaba turno termina
    /// aquí y la cierra `cerrar_sin_motor`.
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

    /// Apagado del servidor: marca en error todas las que no estén terminadas
    /// y cierra aquí mismo todo lo que quede sin cerrar, también lo que un
    /// motor iba a cerrar después (el proceso sale antes de que termine). El
    /// motor que vuelva ya no encuentra nada que cerrar.
    pub fn abortar_todas(&mut self, motivo: &str) -> Vec<CierreTransferencia> {
        let ahora = fecha_ahora_epoca();
        for transferencia in &mut self.lista {
            if transferencia.info.estado.terminada() {
                continue;
            }
            transferencia.cancelada.store(true, Ordering::Relaxed);
            transferencia.abortada = true;
            transferencia.info.estado = EstadoTransferencia::Error;
            transferencia.info.error = Some(motivo.to_string());
            transferencia.info.terminada_en = Some(ahora);
        }
        let ids: Vec<u32> = self
            .lista
            .iter()
            .filter(|transferencia| !transferencia.cerrada)
            .map(|transferencia| transferencia.info.id)
            .collect();
        ids.into_iter().filter_map(|id| self.cerrar(id)).collect()
    }

    /// Marca en error las de un host: su conexión se cayó. Las que esperaban
    /// turno también caen, porque ya no hay por dónde copiarlas; esas las
    /// cierra `cerrar_sin_motor` y las que corren, su motor.
    pub fn abortar_host(&mut self, host_id: i64, motivo: &str) -> bool {
        let ahora = fecha_ahora_epoca();
        let mut afectadas = false;
        for transferencia in &mut self.lista {
            if transferencia.info.host_id != host_id || transferencia.info.estado.terminada() {
                continue;
            }
            transferencia.cancelada.store(true, Ordering::Relaxed);
            transferencia.abortada = true;
            transferencia.info.estado = EstadoTransferencia::Error;
            transferencia.info.error = Some(motivo.to_string());
            transferencia.info.terminada_en = Some(ahora);
            afectadas = true;
        }
        afectadas
    }

    /// Cierra las terminadas que ningún motor va a cerrar: las que se
    /// cancelaron o abortaron mientras esperaban turno.
    pub fn cerrar_sin_motor(&mut self) -> Vec<CierreTransferencia> {
        let ids: Vec<u32> = self
            .lista
            .iter()
            .filter(|transferencia| {
                transferencia.info.estado.terminada()
                    && !transferencia.cerrada
                    && !transferencia.lanzada
            })
            .map(|transferencia| transferencia.info.id)
            .collect();
        ids.into_iter().filter_map(|id| self.cerrar(id)).collect()
    }

    /// Calcula el cierre de una transferencia terminada, una sola vez: la
    /// segunda llamada no devuelve nada (la anotación nunca se duplica).
    fn cerrar(&mut self, id: u32) -> Option<CierreTransferencia> {
        let transferencia = self.obtener_mut(id)?;
        if transferencia.cerrada {
            return None;
        }
        transferencia.cerrada = true;
        let info = &transferencia.info;
        let resultado = resultado_de(
            info.estado,
            transferencia.escrito.load(Ordering::Relaxed),
            transferencia.fallos_borrado > 0,
        );
        let anotacion = match &transferencia.extras.sincronizacion {
            Some(sincronizacion) => Some((
                crate::registro::SINCRONIZACION,
                detalle_sincronizacion(info, sincronizacion, resultado, transferencia.conservados),
                if matches!(
                    resultado,
                    ResultadoSincronizacion::Ok | ResultadoSincronizacion::Cancelada
                ) {
                    ResultadoRegistro::Ok
                } else {
                    ResultadoRegistro::Error
                },
            )),
            None if transferencia.lanzada => Some((
                crate::registro::TRANSFERENCIA,
                detalle_transferencia(info),
                if info.estado == EstadoTransferencia::Error {
                    ResultadoRegistro::Error
                } else {
                    ResultadoRegistro::Ok
                },
            )),
            None => None,
        };
        Some(CierreTransferencia {
            host_id: info.host_id,
            anotacion,
            resultado,
            sincronizacion_id: transferencia
                .extras
                .sincronizacion
                .as_ref()
                .and_then(|sincronizacion| sincronizacion.id),
            deliberacion: transferencia.extras.deliberacion.clone(),
        })
    }

    /// Quita de la cola las terminadas; devuelve cuántas. Una terminada cuyo
    /// cierre aún no ha salido se queda hasta que salga.
    pub fn limpiar(&mut self) -> usize {
        let antes = self.lista.len();
        self.lista.retain(|transferencia| {
            !transferencia.info.estado.terminada() || !transferencia.cerrada
        });
        antes - self.lista.len()
    }

    /// Quita las terminadas hace más de la retención; devuelve cuántas.
    pub fn purgar_caducadas(&mut self, ahora: i64) -> usize {
        let antes = self.lista.len();
        self.lista
            .retain(|transferencia| match transferencia.info.terminada_en {
                Some(terminada) => {
                    ahora - terminada < RETENCION_TERMINADAS || !transferencia.cerrada
                }
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
        self.obtener(id)
            .map(|transferencia| detalle_transferencia(&transferencia.info))
    }
}

/// Detalle de `transferencia`: dirección, estado, rutas, bytes y ficheros; una
/// edición remota lo dice. Nunca contenido.
fn detalle_transferencia(info: &InfoTransferencia) -> String {
    let etiqueta = match info.etiqueta {
        Some(EtiquetaTransferencia::Edicion) => " · edición",
        _ => "",
    };
    let omitidos = match info.omitidos {
        0 => String::new(),
        otros => format!(" · {otros} omitidos"),
    };
    let error = match &info.error {
        Some(error) => format!(" · {error}"),
        None => String::new(),
    };
    format!(
        "{} {}{etiqueta} · {} → {} · {} B · {} fichero(s){omitidos}{error}",
        info.direccion.texto(),
        info.estado.texto(),
        info.origen,
        info.destino,
        info.bytes_hechos,
        info.ficheros_hechos,
    )
}

/// Detalle de `sincronizacion`: nombre (o «ad hoc»), dirección, creados,
/// actualizados, borrados, omitidos, bytes y resultado. Los creados y
/// actualizados son los del plan fijado; los borrados, los de verdad.
fn detalle_sincronizacion(
    info: &InfoTransferencia,
    sincronizacion: &SincronizacionLanzada,
    resultado: ResultadoSincronizacion,
    conservados: u32,
) -> String {
    let nombre = match &sincronizacion.nombre {
        Some(nombre) => format!("«{}»", crate::snippets::salida::sanear_linea(nombre, 64)),
        None => "ad hoc".to_string(),
    };
    let conservados = match conservados {
        0 => String::new(),
        1 => " · 1 directorio no vacío conservado".to_string(),
        otros => format!(" · {otros} directorios no vacíos conservados"),
    };
    let error = match &info.error {
        Some(error) => format!(" · {error}"),
        None => String::new(),
    };
    format!(
        "{nombre} · {} · +{} ~{} −{}{conservados} · {} omitidos · {} B · {}{error}",
        info.direccion.texto(),
        sincronizacion.creados,
        sincronizacion.actualizados,
        info.borrados,
        sincronizacion.omitidos + info.omitidos,
        info.bytes_hechos,
        resultado.como_texto(),
    )
}

/// Cómo acabó una transferencia, con los nombres de `ultimo_resultado`:
/// «parcial» es un error con algo ya escrito en el destino o un borrado que
/// no se pudo completar; «error», uno sin nada escrito.
fn resultado_de(
    estado: EstadoTransferencia,
    escrito: bool,
    fallos_borrado: bool,
) -> ResultadoSincronizacion {
    match estado {
        EstadoTransferencia::Hecha => ResultadoSincronizacion::Ok,
        EstadoTransferencia::Cancelada => ResultadoSincronizacion::Cancelada,
        _ if escrito || fallos_borrado => ResultadoSincronizacion::Parcial,
        _ => ResultadoSincronizacion::Error,
    }
}

/// El mismo resultado en el vocabulario de `DELIBERACIONES`.
fn resultado_ejecucion(resultado: ResultadoSincronizacion) -> EjecucionResultado {
    match resultado {
        ResultadoSincronizacion::Ok => EjecucionResultado::Ok,
        ResultadoSincronizacion::Parcial => EjecucionResultado::Parcial,
        ResultadoSincronizacion::Error => EjecucionResultado::Error,
        ResultadoSincronizacion::Cancelada => EjecucionResultado::Cancelada,
    }
}

/// Manda al hilo escritor el cierre de una transferencia: su anotación, el
/// `ultimo_resultado` de la guardada y la deliberación. El efecto ya se ha
/// producido: se anota ahora, no al decidir.
pub fn emitir_cierre(bd: &std::sync::mpsc::Sender<OrdenBd>, cierre: CierreTransferencia) {
    if let Some((tipo, detalle, resultado)) = cierre.anotacion {
        let _ = bd.send(OrdenBd::Anotar {
            tipo: tipo.to_string(),
            host_id: Some(cierre.host_id),
            identidad_id: None,
            detalle,
            resultado,
        });
    }
    if let Some(sincronizacion_id) = cierre.sincronizacion_id {
        let _ = bd.send(OrdenBd::ResultadoSincronizacion {
            sincronizacion_id,
            resultado: cierre.resultado,
        });
    }
    if let Some(fijada) = &cierre.deliberacion {
        ejecuciones::anotar_deliberacion(bd, fijada, resultado_ejecucion(cierre.resultado));
    }
}

// ---------------------------------------------------------------- validación

/// Componentes de una ruta absoluta (las barras repetidas no cuentan), o
/// ninguno si no es absoluta o lleva `.` o `..`: una ruta así podría salir de
/// la raíz sin que lo pareciera.
fn componentes_absolutos(ruta: &str) -> Option<Vec<&str>> {
    let resto = ruta.strip_prefix('/')?;
    let mut componentes = Vec::new();
    for trozo in resto.split('/') {
        match trozo {
            "" => continue,
            "." | ".." => return None,
            componente => componentes.push(componente),
        }
    }
    Some(componentes)
}

/// Componentes de `ruta` si cuelga estrictamente de `raiz` (la raíz misma no
/// vale), comparando por componentes y no por texto.
fn cuelga_de<'a>(ruta: &'a str, raiz: &[&str]) -> Option<Vec<&'a str>> {
    let componentes = componentes_absolutos(ruta)?;
    (componentes.len() > raiz.len() && componentes[..raiz.len()] == *raiz).then_some(componentes)
}

/// Valida una `Transferir` antes de tocar nada (T33). Borrar al terminar solo
/// existe en una sincronización, y una sincronización solo escribe y borra
/// por debajo de su raíz de destino: rutas absolutas, sin `.` ni `..`, que
/// cuelgan estrictamente de ella. Una ruta no puede copiarse y borrarse a la
/// vez (se perdería lo recién copiado).
pub fn validar_peticion(peticion: &PeticionTransferir) -> Result<(), String> {
    let es_sincronizacion = peticion.etiqueta == Some(EtiquetaTransferencia::Sincronizacion);
    if es_sincronizacion != peticion.sincronizacion.is_some() {
        return Err("una sincronización necesita su etiqueta y sus datos".to_string());
    }
    if peticion.elementos.is_empty() && peticion.borrar_al_terminar.is_empty() {
        return Err("no hay nada que transferir".to_string());
    }
    let Some(sincronizacion) = &peticion.sincronizacion else {
        if !peticion.borrar_al_terminar.is_empty() {
            return Err("solo una sincronización borra en el destino al terminar".to_string());
        }
        return Ok(());
    };
    let raiz_destino = &sincronizacion.raiz_destino;
    let Some(raiz) = componentes_absolutos(raiz_destino) else {
        return Err(format!(
            "la raíz de destino «{raiz_destino}» no es una ruta absoluta sin «.» ni «..»"
        ));
    };
    let mut destinos: HashSet<Vec<&str>> = HashSet::new();
    for elemento in &peticion.elementos {
        let Some(componentes) = cuelga_de(&elemento.destino, &raiz) else {
            return Err(format!(
                "el destino «{}» no está dentro de «{raiz_destino}»",
                elemento.destino
            ));
        };
        destinos.insert(componentes);
    }
    for ruta in &peticion.borrar_al_terminar {
        let Some(componentes) = cuelga_de(ruta, &raiz) else {
            return Err(format!(
                "no se borra «{ruta}»: no está dentro de «{raiz_destino}»"
            ));
        };
        if destinos.contains(&componentes) {
            return Err(format!("«{ruta}» no puede copiarse y borrarse a la vez"));
        }
    }
    Ok(())
}

/// Lee y fija lo que la petición trae del almacén: la deliberación (válida y
/// sin usar) y que la sincronización guardada sea de este host. Sin el mutex
/// del estado: es E/S de SQLite y podría tardar hasta el `busy_timeout`.
async fn leer_autorizacion(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    host_id: i64,
    deliberacion: Option<&DeliberacionLanzada>,
    sincronizacion: Option<&SincronizacionLanzada>,
) -> Result<Option<DeliberacionFijada>, String> {
    let guardada = sincronizacion.and_then(|sincronizacion| sincronizacion.id);
    if deliberacion.is_none() && guardada.is_none() {
        return Ok(None);
    }
    let lectura = estado.lock().await.lectura.clone();
    let lectura = lectura
        .lock()
        .map_err(|_| "almacén envenenado".to_string())?;
    if let Some(id) = guardada {
        let es_del_host = lectura
            .obtener_sincronizacion(id)
            .is_ok_and(|fila| fila.host_id == host_id);
        if !es_del_host {
            return Err(format!(
                "la sincronización guardada #{id} no existe o es de otro host"
            ));
        }
    }
    deliberacion
        .map(|lanzada| ejecuciones::validar_deliberacion(&lectura, lanzada))
        .transpose()
}

// ---------------------------------------------------------------- encolado

/// Expande la petición del cliente, la encola y lanza las transferencias que
/// toquen. Es el único camino para encolar: el cliente nunca manda la cola
/// expandida de una bajada (salvo la de una sincronización, que llega plana).
pub async fn encolar(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    solicitante: u32,
    peticion: PeticionTransferir,
) -> Result<u32, String> {
    validar_peticion(&peticion)?;
    let PeticionTransferir {
        host_id,
        direccion,
        elementos,
        politica,
        borrar_origen,
        peticion_id,
        borrar_al_terminar,
        deliberacion,
        sincronizacion,
        etiqueta,
    } = peticion;
    let deliberacion = leer_autorizacion(
        estado,
        host_id,
        deliberacion.as_ref(),
        sincronizacion.as_ref(),
    )
    .await?;
    let extras = ExtrasTransferencia {
        borrar_al_terminar,
        deliberacion,
        sincronizacion,
    };
    let (host_nombre, sesion) = {
        let estado_bloqueado = estado.lock().await;
        let Some(canal) = estado_bloqueado.sftp.get(&host_id) else {
            return Err("no hay canal SFTP abierto para ese host".to_string());
        };
        (canal.host_nombre.clone(), canal.sesion.clone())
    };

    // La lista de una sincronización llega plana: sus directorios solo se
    // crean, nunca se recorren.
    let plana = etiqueta == Some(EtiquetaTransferencia::Sincronizacion);
    let (ficheros, enlaces_omitidos) =
        expandir(&sesion, direccion, &elementos, politica, plana).await?;

    let id = {
        let mut estado_bloqueado = estado.lock().await;
        // Mirado en el mismo bloqueo que la inserción: una transferencia que
        // entrara tras `abortar_todas` moriría a medias al salir el proceso.
        if estado_bloqueado.apagando {
            return Err(super::conexiones::APAGANDO.to_string());
        }
        // Y la deliberación, también aquí: dos `Transferir` (o una
        // ejecución) con la misma no pueden pasar las dos.
        if let Some(fijada) = &extras.deliberacion {
            if ejecuciones::deliberacion_usada(&estado_bloqueado, fijada.registro.id) {
                return Err(ejecuciones::YA_USADA.to_string());
            }
        }
        let id = estado_bloqueado.transferencias.encolar(
            host_id,
            host_nombre,
            direccion,
            solicitante,
            &elementos,
            borrar_origen,
            ficheros,
            peticion_id,
            etiqueta,
            extras,
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
/// servidor con `read_dir` recursivo, salvo que llegue `plana` (una
/// sincronización: el plan ya lo dice todo).
async fn expandir(
    sesion: &SftpSession,
    direccion: Direccion,
    elementos: &[ElementoTransferencia],
    politica: Politica,
    plana: bool,
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
                permisos: elemento.permisos,
            });
            continue;
        }
        ficheros.push(FicheroTransferencia {
            origen: elemento.origen.clone(),
            destino: elemento.destino.clone(),
            bytes: 0,
            es_dir: true,
            politica,
            permisos: elemento.permisos,
        });
        match direccion {
            Direccion::Subida => {
                // El cliente ya manda el contenido de los directorios: si aquí
                // solo viene el directorio, es que está vacío.
            }
            Direccion::Bajada if plana => {}
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
                    permisos: None,
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
                    permisos: None,
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
                    permisos: None,
                });
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- motor

/// Lo que el motor lee una vez de la transferencia al arrancar.
struct Motor {
    host_id: i64,
    direccion: Direccion,
    ficheros: Vec<FicheroTransferencia>,
    cancelada: Arc<AtomicBool>,
    escrito: Arc<AtomicBool>,
    borrar_origen: bool,
    borrar_al_terminar: Vec<String>,
    /// La de la sincronización: de ella cuelga todo lo que se borra.
    raiz_destino: String,
    /// Los omitidos al expandir (enlaces a directorio): cuentan desde el
    /// principio y no se pueden perder al terminar.
    omitidos: u32,
}

/// Cómo dejó el motor la transferencia.
#[derive(Debug, Clone)]
struct FinMotor {
    estado: EstadoTransferencia,
    fallo: Option<String>,
    bytes_hechos: u64,
    ficheros_hechos: u32,
    omitidos: u32,
    borrado: ResultadoBorrado,
}

impl FinMotor {
    fn fallida(motivo: String, omitidos: u32) -> Self {
        Self {
            estado: EstadoTransferencia::Error,
            fallo: Some(motivo),
            bytes_hechos: 0,
            ficheros_hechos: 0,
            omitidos,
            borrado: ResultadoBorrado::default(),
        }
    }
}

/// Copia una transferencia entera, fichero a fichero, borra lo que toque si
/// terminó bien y deja el estado final en la cola. Nunca toca la red con el
/// bloqueo del estado tomado.
pub async fn tarea(estado: Arc<tokio::sync::Mutex<EstadoServidor>>, id: u32) {
    let motor = {
        let estado_bloqueado = estado.lock().await;
        let Some(transferencia) = estado_bloqueado.transferencias.obtener(id) else {
            return;
        };
        Motor {
            host_id: transferencia.info.host_id,
            direccion: transferencia.info.direccion,
            ficheros: transferencia.ficheros.clone(),
            cancelada: transferencia.cancelada.clone(),
            escrito: transferencia.escrito.clone(),
            borrar_origen: transferencia.info.borrar_origen,
            borrar_al_terminar: transferencia.extras.borrar_al_terminar.clone(),
            raiz_destino: transferencia
                .extras
                .sincronizacion
                .as_ref()
                .map(|sincronizacion| sincronizacion.raiz_destino.clone())
                .unwrap_or_default(),
            omitidos: transferencia.info.omitidos,
        }
    };

    let canal = {
        let estado_bloqueado = estado.lock().await;
        estado_bloqueado
            .sftp
            .get(&motor.host_id)
            .map(|canal| (canal.sesion.clone(), canal.renombrador.clone()))
    };
    let Some((sesion, renombrador)) = canal else {
        terminar(
            &estado,
            id,
            FinMotor::fallida("conexión caída".to_string(), motor.omitidos),
        )
        .await;
        return;
    };

    let (mut fin, copiados) = copiar_todo(&estado, id, &sesion, &renombrador, &motor).await;

    // El origen solo se borra cuando la copia ha terminado bien; nunca antes.
    if fin.estado == EstadoTransferencia::Hecha
        && motor.borrar_origen
        && motor.direccion == Direccion::Bajada
    {
        if let Err(error) = borrar_origenes(&sesion, &copiados).await {
            warn!(transferencia = id, "no se pudo borrar el origen: {error}");
        }
    }
    // Y el destino de una sincronización, igual: solo tras una copia sin
    // errores ni cancelación (D95).
    if fin.estado == EstadoTransferencia::Hecha && !motor.borrar_al_terminar.is_empty() {
        borrar_tras_copia(&estado, id, &sesion, &motor, &mut fin).await;
    }
    terminar(&estado, id, fin).await;
    avisar(&estado).await;
}

/// La copia en sí: directorios y ficheros en orden, parando en el primer
/// error (F4) o en la cancelación. Devuelve el fin y lo copiado de verdad.
async fn copiar_todo(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    id: u32,
    sesion: &SftpSession,
    renombrador: &sftp::Renombrador,
    motor: &Motor,
) -> (FinMotor, Vec<FicheroTransferencia>) {
    let mut hechos = 0u64;
    let mut ficheros_hechos = 0u32;
    // Lo que se ha copiado de verdad: con `borrar_origen` solo se borra esto,
    // nunca los orígenes que se omitieron (no se copiaron, no se pueden
    // perder).
    let mut copiados: Vec<FicheroTransferencia> = Vec::new();
    let mut omitidos = motor.omitidos;
    let mut ultimo_aviso = Instant::now();
    let mut fallo: Option<String> = None;
    let mut cancelada_por_el_usuario = false;
    // Directorios creados aquí cuyo modo dejaría al dueño sin entrar o sin
    // escribir: se crean abiertos para él y su modo final se pone al acabar.
    let mut modos_pendientes: Vec<(String, u32)> = Vec::new();

    for fichero in &motor.ficheros {
        if motor.cancelada.load(Ordering::Relaxed) {
            cancelada_por_el_usuario = true;
            break;
        }
        if fichero.es_dir {
            match crear_directorio(sesion, motor.direccion, fichero).await {
                Ok(creado) => {
                    if creado.creado {
                        motor.escrito.store(true, Ordering::Relaxed);
                    }
                    modos_pendientes.extend(creado.modo_pendiente);
                }
                Err(motivo) => {
                    fallo = Some(motivo);
                    break;
                }
            }
            continue;
        }

        // El destino puede haberse creado o borrado desde que el cliente
        // miró: se recomprueba justo antes de escribir.
        let existe = match motor.direccion {
            Direccion::Subida => sftp::existe(sesion, &fichero.destino).await,
            Direccion::Bajada => Path::new(&fichero.destino).exists(),
        };
        if existe && fichero.politica == Politica::Omitir {
            omitidos += 1;
            continue;
        }

        avisar_progreso(
            estado,
            id,
            &fichero.origen,
            hechos,
            ficheros_hechos,
            omitidos,
        )
        .await;
        match copiar(
            sesion,
            renombrador,
            motor.direccion,
            fichero,
            &motor.cancelada,
        )
        .await
        {
            Ok(ResultadoCopia::Hecho(bytes)) => {
                hechos += bytes;
                ficheros_hechos += 1;
                copiados.push(fichero.clone());
                motor.escrito.store(true, Ordering::Relaxed);
                // Tras el `rename` del parcial (T56): el fichero ya está en su
                // sitio, así que un fallo aquí no lo deshace pero se dice.
                if let Some(permisos) = fichero.permisos {
                    let modo = permisos & 0o7777;
                    if let Err(motivo) =
                        aplicar_permisos(sesion, motor.direccion, &fichero.destino, modo).await
                    {
                        let hecho = match motor.direccion {
                            Direccion::Subida => "subido",
                            Direccion::Bajada => "bajado",
                        };
                        fallo = Some(format!(
                            "{}: {hecho}, pero no se pudieron aplicar los permisos {modo:04o}: {motivo}",
                            fichero.destino
                        ));
                        break;
                    }
                }
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
                estado,
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

    // El modo final de los directorios que se crearon abiertos para el dueño.
    // Se pone pase lo que pase con la copia; su fallo solo cuenta si la copia
    // iba bien.
    for (ruta, modo) in modos_pendientes {
        if let Err(motivo) = aplicar_permisos(sesion, motor.direccion, &ruta, modo).await {
            if fallo.is_none() && !cancelada_por_el_usuario {
                fallo = Some(format!(
                    "{ruta}: creado, pero no se pudieron aplicar los permisos {modo:04o}: {motivo}"
                ));
            }
        }
    }

    let estado_final = if fallo.is_some() {
        EstadoTransferencia::Error
    } else if cancelada_por_el_usuario {
        EstadoTransferencia::Cancelada
    } else {
        EstadoTransferencia::Hecha
    };
    let fin = FinMotor {
        estado: estado_final,
        fallo,
        bytes_hechos: hechos,
        ficheros_hechos,
        omitidos,
        borrado: ResultadoBorrado::default(),
    };
    (fin, copiados)
}

/// Un directorio de la lista ya tratado: si lo creó esta transferencia y, si
/// su modo final hay que ponerlo al acabar, cuál.
struct DirectorioCreado {
    creado: bool,
    modo_pendiente: Option<(String, u32)>,
}

/// Crea un directorio de la lista. Sus permisos se aplican solo si lo crea
/// esta transferencia: uno que ya existía no se toca.
async fn crear_directorio(
    sesion: &SftpSession,
    direccion: Direccion,
    fichero: &FicheroTransferencia,
) -> Result<DirectorioCreado, String> {
    let existia = match direccion {
        Direccion::Subida => sftp::existe(sesion, &fichero.destino).await,
        Direccion::Bajada => std::fs::symlink_metadata(&fichero.destino).is_ok(),
    };
    match direccion {
        Direccion::Subida => sftp::crear_dir_padres(sesion, &fichero.destino).await?,
        Direccion::Bajada => std::fs::create_dir_all(&fichero.destino)
            .map_err(|error| format!("{}: {error}", fichero.destino))?,
    }
    let Some(permisos) = fichero.permisos.filter(|_| !existia) else {
        return Ok(DirectorioCreado {
            creado: !existia,
            modo_pendiente: None,
        });
    };
    let modo = permisos & 0o7777;
    // Mientras se copia, el dueño tiene que poder entrar y escribir dentro.
    let mientras = modo | 0o700;
    aplicar_permisos(sesion, direccion, &fichero.destino, mientras)
        .await
        .map_err(|motivo| {
            format!(
                "{}: creado, pero no se pudieron aplicar los permisos {modo:04o}: {motivo}",
                fichero.destino
            )
        })?;
    Ok(DirectorioCreado {
        creado: true,
        modo_pendiente: (mientras != modo).then(|| (fichero.destino.clone(), modo)),
    })
}

/// `chmod` del destino: por SFTP en una subida, en el disco local en una
/// bajada.
async fn aplicar_permisos(
    sesion: &SftpSession,
    direccion: Direccion,
    ruta: &str,
    modo: u32,
) -> Result<(), String> {
    match direccion {
        Direccion::Subida => {
            let atributos = russh_sftp::protocol::FileAttributes {
                permissions: Some(modo),
                ..Default::default()
            };
            sesion
                .set_metadata(ruta, atributos)
                .await
                .map_err(|error| sftp::describir(error.to_string()))
        }
        Direccion::Bajada => std::fs::set_permissions(ruta, std::fs::Permissions::from_mode(modo))
            .map_err(|error| error.to_string()),
    }
}

/// Modo con el que nace el parcial de un elemento con permisos: los del
/// original para grupo y otros (el contenido nunca está más abierto de lo que
/// estaba) y lectura y escritura para el dueño, que tiene que poder reabrir
/// un parcial que quedara de un intento anterior. El modo exacto se pone
/// tras el `rename`.
fn modo_del_parcial(permisos: u32) -> u32 {
    (permisos & 0o777) | 0o600
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
    renombrador: &sftp::Renombrador,
    direccion: Direccion,
    fichero: &FicheroTransferencia,
    cancelada: &AtomicBool,
) -> Result<ResultadoCopia, String> {
    let resultado = match direccion {
        Direccion::Bajada => copiar_bajada(sesion, fichero, cancelada).await,
        Direccion::Subida => copiar_subida(sesion, renombrador, fichero, cancelada).await,
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
    let mut opciones = std::fs::OpenOptions::new();
    opciones.write(true).create(true).truncate(true);
    if let Some(permisos) = fichero.permisos {
        opciones.mode(modo_del_parcial(permisos));
    }
    let mut salida = opciones
        .open(&parcial)
        .map_err(|error| format!("{parcial}: {error}"))?;

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
    renombrador: &sftp::Renombrador,
    fichero: &FicheroTransferencia,
    cancelada: &AtomicBool,
) -> Result<ResultadoCopia, String> {
    let parcial = format!("{}{}", fichero.destino, sftp::SUFIJO_PARCIAL);
    sftp::crear_dir_padres(sesion, &sftp::padre(&parcial)).await?;
    let mut entrada = tokio::fs::File::open(&fichero.origen)
        .await
        .map_err(|error| format!("{}: {error}", fichero.origen))?;
    let atributos = russh_sftp::protocol::FileAttributes {
        permissions: fichero.permisos.map(modo_del_parcial),
        ..Default::default()
    };
    let mut salida = sesion
        .open_with_flags_and_attributes(&parcial, sftp::banderas_escritura(), atributos)
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
    // Sobre un fichero que ya existe también (el `rename` de SFTP v3 de
    // OpenSSH falla si el destino existe): lo resuelve el renombrador del
    // canal.
    renombrador
        .renombrar_sobre(sesion, &parcial, &fichero.destino)
        .await?;
    Ok(ResultadoCopia::Hecho(total))
}

// ---------------------------------------------------------------- borrado

/// Lo que dejó el borrado de `borrar_al_terminar`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ResultadoBorrado {
    /// Rutas borradas (o que ya no existían: cuentan como hechas).
    borrados: u32,
    /// Directorios que no quedaron vacíos (contenido excluido o ajeno al
    /// plan): se conservan y no es un error.
    conservados: u32,
    /// Rutas que no se pudieron borrar, con su motivo.
    fallos: Vec<String>,
    /// La cancelación llegó entre dos borrados: se paró ahí.
    cancelado: bool,
}

/// Dónde se borra al terminar: en el remoto (subida, por SFTP) o en el disco
/// local (bajada, lo hace el propio servidor).
enum LadoDestino<'a> {
    Remoto(&'a SftpSession),
    Local,
}

/// Cómo acabó el `rmdir` de un directorio de `borrar_al_terminar`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BorradoDir {
    Borrado,
    /// Le queda contenido que no estaba en el plan: se conserva.
    NoVacio,
}

/// ¿Es «la ruta no existe» este error de SFTP?
fn no_existe(error: &russh_sftp::client::error::Error) -> bool {
    matches!(
        error,
        russh_sftp::client::error::Error::Status(estado)
            if estado.status_code == russh_sftp::protocol::StatusCode::NoSuchFile
    )
}

impl LadoDestino<'_> {
    /// Sin seguir enlaces: ninguno si ya no existe, `true` si es un
    /// directorio de verdad (un enlace a un directorio es un enlace).
    async fn es_directorio(&self, ruta: &str) -> Result<Option<bool>, String> {
        match self {
            LadoDestino::Remoto(sesion) => match sesion.symlink_metadata(ruta).await {
                Ok(metadata) => Ok(Some(metadata.is_dir() && !metadata.is_symlink())),
                Err(error) if no_existe(&error) => Ok(None),
                Err(error) => Err(sftp::describir(error.to_string())),
            },
            LadoDestino::Local => match std::fs::symlink_metadata(ruta) {
                Ok(metadata) => Ok(Some(metadata.is_dir())),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error.to_string()),
            },
        }
    }

    /// Borra un fichero o un enlace (como enlace). Uno que ya no está cuenta
    /// como borrado.
    async fn borrar_fichero(&self, ruta: &str) -> Result<(), String> {
        match self {
            LadoDestino::Remoto(sesion) => match sesion.remove_file(ruta).await {
                Ok(()) => Ok(()),
                Err(error) if no_existe(&error) => Ok(()),
                Err(error) => Err(sftp::describir(error.to_string())),
            },
            LadoDestino::Local => match std::fs::remove_file(ruta) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error.to_string()),
            },
        }
    }

    /// `rmdir`, nunca recursivo. SFTP v3 no distingue «no vacío» de otros
    /// fallos, así que tras un fallo se mira dentro: con contenido se
    /// conserva; si ya no existe, cuenta como borrado.
    async fn borrar_dir(&self, ruta: &str) -> Result<BorradoDir, String> {
        let motivo = match self {
            LadoDestino::Remoto(sesion) => match sesion.remove_dir(ruta).await {
                Ok(()) => return Ok(BorradoDir::Borrado),
                Err(error) => sftp::describir(error.to_string()),
            },
            LadoDestino::Local => match std::fs::remove_dir(ruta) {
                Ok(()) => return Ok(BorradoDir::Borrado),
                Err(error) => error.to_string(),
            },
        };
        match self.tiene_contenido(ruta).await {
            Some(true) => Ok(BorradoDir::NoVacio),
            None => Ok(BorradoDir::Borrado),
            Some(false) => Err(motivo),
        }
    }

    /// ¿Es un enlace? Sin seguirlo; lo que ya no existe no lo es.
    async fn es_enlace(&self, ruta: &str) -> Result<bool, String> {
        match self {
            LadoDestino::Remoto(sesion) => match sesion.symlink_metadata(ruta).await {
                Ok(metadata) => Ok(metadata.is_symlink()),
                Err(error) if no_existe(&error) => Ok(false),
                Err(error) => Err(sftp::describir(error.to_string())),
            },
            LadoDestino::Local => match std::fs::symlink_metadata(ruta) {
                Ok(metadata) => Ok(metadata.file_type().is_symlink()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(error) => Err(error.to_string()),
            },
        }
    }

    /// `true` si el directorio tiene algo dentro; ninguno si ya no existe. Si
    /// no se puede mirar, se da por vacío (y el fallo del `rmdir` manda).
    async fn tiene_contenido(&self, ruta: &str) -> Option<bool> {
        match self {
            LadoDestino::Remoto(sesion) => match sesion.read_dir(ruta).await {
                Ok(mut lectura) => Some(lectura.next().is_some()),
                Err(error) if no_existe(&error) => None,
                Err(_) => Some(false),
            },
            LadoDestino::Local => match std::fs::read_dir(ruta) {
                Ok(mut lectura) => Some(lectura.next().is_some()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(_) => Some(false),
            },
        }
    }
}

/// Profundidad de una ruta absoluta (componentes no vacíos).
fn profundidad(ruta: &str) -> usize {
    ruta.split('/').filter(|trozo| !trozo.is_empty()).count()
}

/// ¿Cuelga `ruta` de un enlace por debajo de `raiz`? Una ruta del plan nunca
/// lo hace (el plan omite los enlaces a directorio): si ahora sí, el destino
/// cambió desde que se planificó y borrar ahí saldría del árbol. Los
/// directorios ya mirados se recuerdan. `ruta` ya se validó al encolar: es
/// absoluta, limpia y cuelga de `raiz` por componentes.
async fn cuelga_de_un_enlace(
    lado: &LadoDestino<'_>,
    raiz: &str,
    ruta: &str,
    mirados: &mut HashSet<String>,
) -> Result<bool, String> {
    let desde = profundidad(raiz);
    let componentes: Vec<&str> = ruta.split('/').filter(|trozo| !trozo.is_empty()).collect();
    let mut camino = String::new();
    // Los directorios entre la raíz (sin incluirla) y la ruta (tampoco).
    for (indice, componente) in componentes
        .iter()
        .enumerate()
        .take(componentes.len().saturating_sub(1))
    {
        camino.push('/');
        camino.push_str(componente);
        if indice < desde || mirados.contains(&camino) {
            continue;
        }
        if lado.es_enlace(&camino).await? {
            return Ok(true);
        }
        mirados.insert(camino.clone());
    }
    Ok(false)
}

/// Borra las rutas fijadas en el plan. Dos pasadas: primero lo que no es
/// directorio (con `lstat`: un enlace se borra como enlace, nunca se sigue) y
/// después los directorios, de más profundo a menos y con `rmdir` (nunca
/// recursivo). Nada que cuelgue de un enlace por debajo de `raiz` se toca. Un
/// fallo no detiene el resto; la cancelación, sí. Con `progreso`, cada paso
/// actualiza la fila de la cola y se difunde.
async fn borrar_rutas(
    lado: &LadoDestino<'_>,
    raiz: &str,
    rutas: &[String],
    cancelada: &AtomicBool,
    escrito: &AtomicBool,
    progreso: Option<(&Arc<tokio::sync::Mutex<EstadoServidor>>, u32)>,
) -> ResultadoBorrado {
    let mut resultado = ResultadoBorrado::default();
    let mut directorios: Vec<&str> = Vec::new();
    let mut mirados: HashSet<String> = HashSet::new();
    for ruta in rutas {
        if cancelada.load(Ordering::Relaxed) {
            resultado.cancelado = true;
            return resultado;
        }
        match cuelga_de_un_enlace(lado, raiz, ruta, &mut mirados).await {
            Ok(false) => {}
            Ok(true) => {
                resultado.fallos.push(format!(
                    "{ruta}: cuelga de un enlace (el destino cambió desde el plan); no se borra"
                ));
                continue;
            }
            Err(motivo) => {
                resultado.fallos.push(format!("{ruta}: {motivo}"));
                continue;
            }
        }
        match lado.es_directorio(ruta).await {
            Ok(None) => resultado.borrados += 1,
            Ok(Some(true)) => {
                directorios.push(ruta);
                continue;
            }
            Ok(Some(false)) => match lado.borrar_fichero(ruta).await {
                Ok(()) => {
                    resultado.borrados += 1;
                    escrito.store(true, Ordering::Relaxed);
                }
                Err(motivo) => resultado.fallos.push(format!("{ruta}: {motivo}")),
            },
            Err(motivo) => resultado.fallos.push(format!("{ruta}: {motivo}")),
        }
        if let Some((estado, id)) = progreso {
            avisar_borrados(estado, id, resultado.borrados).await;
        }
    }
    // Estable: a igual profundidad, en el orden del plan.
    directorios.sort_by_key(|ruta| std::cmp::Reverse(profundidad(ruta)));
    for ruta in directorios {
        if cancelada.load(Ordering::Relaxed) {
            resultado.cancelado = true;
            return resultado;
        }
        match lado.borrar_dir(ruta).await {
            Ok(BorradoDir::Borrado) => {
                resultado.borrados += 1;
                escrito.store(true, Ordering::Relaxed);
            }
            Ok(BorradoDir::NoVacio) => resultado.conservados += 1,
            Err(motivo) => resultado.fallos.push(format!("{ruta}: {motivo}")),
        }
        if let Some((estado, id)) = progreso {
            avisar_borrados(estado, id, resultado.borrados).await;
        }
    }
    resultado
}

/// La copia terminó sin errores: pasa a «borrando» (difundido antes del
/// efecto lento) y borra `borrar_al_terminar` en el destino. El resultado
/// queda en `fin`: cancelada si se paró, error «copia completa; …» si algo no
/// se pudo borrar.
async fn borrar_tras_copia(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    id: u32,
    sesion: &SftpSession,
    motor: &Motor,
    fin: &mut FinMotor,
) {
    let empieza = {
        let mut estado_bloqueado = estado.lock().await;
        let empieza = match estado_bloqueado.transferencias.obtener_mut(id) {
            // Abortada o cancelada mientras tanto: no se borra nada.
            Some(transferencia)
                if transferencia.info.estado == EstadoTransferencia::EnCurso
                    && !transferencia.cancelada() =>
            {
                transferencia.info.estado = EstadoTransferencia::Borrando;
                transferencia.info.fichero_actual = None;
                true
            }
            _ => false,
        };
        estado_bloqueado.difusion_cola.marcar(true);
        empieza
    };
    if !empieza {
        fin.estado = EstadoTransferencia::Cancelada;
        return;
    }
    let lado = match motor.direccion {
        Direccion::Subida => LadoDestino::Remoto(sesion),
        Direccion::Bajada => LadoDestino::Local,
    };
    let borrado = borrar_rutas(
        &lado,
        &motor.raiz_destino,
        &motor.borrar_al_terminar,
        &motor.cancelada,
        &motor.escrito,
        Some((estado, id)),
    )
    .await;
    if borrado.cancelado {
        fin.estado = EstadoTransferencia::Cancelada;
    } else if let Some(primero) = borrado.fallos.first() {
        for fallo in &borrado.fallos {
            warn!(transferencia = id, "no se pudo borrar al terminar: {fallo}");
        }
        let mas = match borrado.fallos.len() {
            1 => String::new(),
            otros => format!(" (y {} más)", otros - 1),
        };
        fin.estado = EstadoTransferencia::Error;
        fin.fallo = Some(format!(
            "copia completa; no se pudieron borrar {} de {}: {primero}{mas}",
            borrado.fallos.len(),
            motor.borrar_al_terminar.len(),
        ));
    }
    fin.borrado = borrado;
}

/// Publica cuántas rutas lleva borradas; el envío lo coalesce la revisora.
async fn avisar_borrados(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>, id: u32, borrados: u32) {
    let mut estado_bloqueado = estado.lock().await;
    let mut host_id = None;
    if let Some(transferencia) = estado_bloqueado.transferencias.obtener_mut(id) {
        transferencia.info.borrados = borrados;
        host_id = Some(transferencia.info.host_id);
    }
    if let Some(host_id) = host_id {
        if let Some(canal) = estado_bloqueado.sftp.get_mut(&host_id) {
            canal.ultima_actividad = Instant::now();
        }
    }
    estado_bloqueado.difusion_cola.marcar(false);
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

/// Deja el estado final de la transferencia en la cola y emite su cierre. Si
/// la abortaron mientras corría (conexión caída), el estado final es ese
/// error y no una cancelación; si ya se cerró (apagado), no hay nada que
/// hacer.
async fn terminar(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>, id: u32, fin: FinMotor) {
    let mut estado_bloqueado = estado.lock().await;
    let estado_final = {
        let Some(transferencia) = estado_bloqueado.transferencias.obtener_mut(id) else {
            return;
        };
        if transferencia.cerrada {
            return;
        }
        let info = &mut transferencia.info;
        if !transferencia.abortada {
            info.estado = fin.estado;
            info.error = fin.fallo.clone();
            info.terminada_en = Some(fecha_ahora_epoca());
        }
        info.omitidos = fin.omitidos;
        info.fichero_actual = None;
        info.bytes_hechos = fin.bytes_hechos;
        info.ficheros_hechos = fin.ficheros_hechos;
        info.borrados = fin.borrado.borrados;
        if info.estado == EstadoTransferencia::Hecha {
            info.bytes_hechos = info.bytes_total;
            info.ficheros_hechos = info.ficheros_total;
        }
        transferencia.conservados = fin.borrado.conservados;
        transferencia.fallos_borrado = fin.borrado.fallos.len() as u32;
        info.estado
    };
    let cierre = estado_bloqueado.transferencias.cerrar(id);
    estado_bloqueado.difusion_cola.marcar(true);
    let bd = estado_bloqueado.bd.clone();
    drop(estado_bloqueado);
    if let Some(cierre) = cierre {
        emitir_cierre(&bd, cierre);
    }
    info!(
        transferencia = id,
        estado = estado_final.texto(),
        "transferencia terminada"
    );
}

/// `CancelarTransferencia`: la que esperaba turno termina y se cierra aquí; la
/// que corre, la para su motor en el siguiente bloque (o entre dos borrados).
pub async fn cancelar(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>, id: u32) {
    let (cierres, bd) = {
        let mut estado_bloqueado = estado.lock().await;
        if !estado_bloqueado.transferencias.cancelar(id) {
            return;
        }
        estado_bloqueado.difusion_cola.marcar(true);
        (
            estado_bloqueado.transferencias.cerrar_sin_motor(),
            estado_bloqueado.bd.clone(),
        )
    };
    for cierre in cierres {
        emitir_cierre(&bd, cierre);
    }
}

/// El host perdió la conexión: sus transferencias pasan a error y la cola
/// sigue con las demás. Las que esperaban turno se cierran aquí; las que
/// corrían, las cierra su motor al volver.
pub async fn caida(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>, host_id: i64) {
    let (afectadas, cierres, bd) = {
        let mut estado_bloqueado = estado.lock().await;
        let afectadas = estado_bloqueado
            .transferencias
            .abortar_host(host_id, "conexión caída");
        if afectadas {
            estado_bloqueado.difusion_cola.marcar(true);
        }
        (
            afectadas,
            estado_bloqueado.transferencias.cerrar_sin_motor(),
            estado_bloqueado.bd.clone(),
        )
    };
    for cierre in cierres {
        emitir_cierre(&bd, cierre);
    }
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
            permisos: None,
        }
    }

    fn fichero(origen: &str, destino: &str, bytes: u64) -> FicheroTransferencia {
        FicheroTransferencia {
            origen: origen.to_string(),
            destino: destino.to_string(),
            bytes,
            es_dir: false,
            politica: Politica::Sobrescribir,
            permisos: None,
        }
    }

    fn encolar(cola: &mut Cola, host_id: i64, origen: &str) -> u32 {
        encolar_con(cola, host_id, origen, ExtrasTransferencia::default(), None)
    }

    fn encolar_con(
        cola: &mut Cola,
        host_id: i64,
        origen: &str,
        extras: ExtrasTransferencia,
        etiqueta: Option<EtiquetaTransferencia>,
    ) -> u32 {
        let elementos = vec![elemento(origen, "/destino", 100)];
        cola.encolar(
            host_id,
            format!("host-{host_id}"),
            Direccion::Subida,
            1,
            &elementos,
            false,
            vec![fichero(origen, "/destino", 100)],
            None,
            etiqueta,
            extras,
        )
    }

    fn sincronizacion(id: Option<i64>, nombre: Option<&str>) -> SincronizacionLanzada {
        SincronizacionLanzada {
            id,
            nombre: nombre.map(str::to_string),
            creados: 12,
            actualizados: 3,
            omitidos: 1,
            raiz_origen: "/home/hector/web".to_string(),
            raiz_destino: "/var/www/app".to_string(),
        }
    }

    fn extras_sincronizacion(id: Option<i64>, borrar: &[&str]) -> ExtrasTransferencia {
        ExtrasTransferencia {
            borrar_al_terminar: borrar.iter().map(|ruta| ruta.to_string()).collect(),
            deliberacion: None,
            sincronizacion: Some(sincronizacion(id, Some("web-prod"))),
        }
    }

    fn deliberacion_fijada(id: i64) -> DeliberacionFijada {
        DeliberacionFijada {
            registro: crate::deliberacion::RegistroDeliberacion {
                id,
                fecha: "2026-10-03 12:00:00".to_string(),
                snippet_id: None,
                accion: "sync web-prod → prueba: 15 ficheros, 2 borrados".to_string(),
                hosts: vec![(1, "prueba".to_string())],
                comprobaciones: Vec::new(),
                resultado: crate::deliberacion::ResultadoDeliberacion::Aprobada,
                bloqueada: false,
                motivo: None,
                usuario: "hector".to_string(),
                ejecucion_resultado: None,
            },
            forzada: false,
        }
    }

    fn peticion(
        borrar: &[&str],
        destinos: &[&str],
        sincronizacion: Option<SincronizacionLanzada>,
    ) -> PeticionTransferir {
        PeticionTransferir {
            host_id: 1,
            direccion: Direccion::Subida,
            elementos: destinos
                .iter()
                .map(|destino| elemento("/home/hector/web/x", destino, 1))
                .collect(),
            politica: Politica::Sobrescribir,
            borrar_origen: false,
            peticion_id: Some(7),
            borrar_al_terminar: borrar.iter().map(|ruta| ruta.to_string()).collect(),
            deliberacion: None,
            etiqueta: sincronizacion
                .is_some()
                .then_some(EtiquetaTransferencia::Sincronizacion),
            sincronizacion,
        }
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
                    permisos: None,
                },
                fichero("static/a.css", "/var/www/static/a.css", 300),
                fichero("static/b.css", "/var/www/static/b.css", 200),
            ],
            None,
            None,
            ExtrasTransferencia::default(),
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
        assert!(
            cola.cerrar_sin_motor().is_empty(),
            "la cierra su motor, no la cancelación"
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
        cola.cerrar(id);

        assert_eq!(cola.purgar_caducadas(1_000 + RETENCION_TERMINADAS - 1), 0);
        assert_eq!(cola.info().len(), 1);
        assert_eq!(cola.purgar_caducadas(1_000 + RETENCION_TERMINADAS), 1);

        let otra = encolar(&mut cola, 2, "b");
        cola.obtener_mut(otra).unwrap().info.estado = EstadoTransferencia::Error;
        cola.cerrar(otra);
        assert_eq!(cola.limpiar(), 1);
    }

    #[test]
    fn limpiar_no_toca_las_vivas_y_hay_vivas_lo_dice() {
        let mut cola = Cola::default();
        let terminada = encolar(&mut cola, 1, "a");
        let viva = encolar(&mut cola, 2, "b");
        cola.obtener_mut(terminada).unwrap().info.estado = EstadoTransferencia::Cancelada;
        cola.cerrar(terminada);

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
        assert!(!detalle.contains("edición"), "{detalle}");
    }

    #[test]
    fn una_edicion_se_anota_como_transferencia_que_lo_dice() {
        let mut cola = Cola::default();
        let id = encolar_con(
            &mut cola,
            1,
            "/run/magi/ediciones/1-nginx.conf",
            ExtrasTransferencia::default(),
            Some(EtiquetaTransferencia::Edicion),
        );
        cola.despachar();
        cola.obtener_mut(id).unwrap().info.estado = EstadoTransferencia::Hecha;
        let cierre = cola.cerrar(id).expect("se cierra");
        let (tipo, detalle, resultado) = cierre.anotacion.expect("con anotación");
        assert_eq!(tipo, crate::registro::TRANSFERENCIA);
        assert!(
            detalle.starts_with("subida hecha · edición · "),
            "{detalle}"
        );
        assert_eq!(resultado, ResultadoRegistro::Ok);
        assert_eq!(cierre.sincronizacion_id, None);
    }

    #[test]
    fn una_sincronizacion_ensena_sus_raices_y_cuenta_lo_que_borrara() {
        let mut cola = Cola::default();
        let id = encolar_con(
            &mut cola,
            1,
            "/home/hector/web/a",
            extras_sincronizacion(Some(4), &["/var/www/app/viejo.css", "/var/www/app/tmp"]),
            Some(EtiquetaTransferencia::Sincronizacion),
        );
        let info = &cola.obtener(id).unwrap().info;
        assert_eq!(info.origen, "/home/hector/web");
        assert_eq!(info.destino, "/var/www/app");
        assert!(info.es_directorio);
        assert_eq!(info.borrados_total, 2);
        assert_eq!(info.borrados, 0);
    }

    #[test]
    fn el_resultado_de_una_sincronizacion_distingue_parcial_de_error() {
        use EstadoTransferencia::*;
        use ResultadoSincronizacion as R;
        assert_eq!(resultado_de(Hecha, true, false), R::Ok);
        assert_eq!(resultado_de(Cancelada, true, false), R::Cancelada);
        assert_eq!(resultado_de(Error, true, false), R::Parcial, "algo escrito");
        assert_eq!(resultado_de(Error, false, false), R::Error, "nada escrito");
        assert_eq!(
            resultado_de(Error, false, true),
            R::Parcial,
            "un fallo al borrar siempre es parcial"
        );
    }

    #[test]
    fn el_cierre_de_una_sincronizacion_anota_y_fija_el_resultado_una_sola_vez() {
        let mut cola = Cola::default();
        let mut extras = extras_sincronizacion(Some(4), &["/var/www/app/viejo.css"]);
        extras.deliberacion = Some(deliberacion_fijada(9));
        let id = encolar_con(
            &mut cola,
            1,
            "/home/hector/web/a",
            extras,
            Some(EtiquetaTransferencia::Sincronizacion),
        );
        cola.despachar();
        {
            let transferencia = cola.obtener_mut(id).unwrap();
            transferencia.escrito.store(true, Ordering::Relaxed);
            transferencia.info.estado = EstadoTransferencia::Error;
            transferencia.info.error = Some("/home/hector/web/b: permiso denegado".to_string());
            transferencia.info.bytes_hechos = 4200;
        }
        let cierre = cola.cerrar(id).expect("se cierra");
        assert_eq!(cierre.resultado, ResultadoSincronizacion::Parcial);
        assert_eq!(cierre.sincronizacion_id, Some(4));
        assert_eq!(cierre.deliberacion.as_ref().map(|d| d.registro.id), Some(9));
        let (tipo, detalle, resultado) = cierre.anotacion.expect("con anotación");
        assert_eq!(tipo, crate::registro::SINCRONIZACION);
        assert_eq!(resultado, ResultadoRegistro::Error);
        assert_eq!(
            detalle,
            "«web-prod» · subida · +12 ~3 −0 · 1 omitidos · 4200 B · parcial · \
             /home/hector/web/b: permiso denegado"
        );
        assert!(cola.cerrar(id).is_none(), "el cierre nunca se duplica");
    }

    #[test]
    fn el_detalle_de_una_sincronizacion_ad_hoc_cuenta_los_conservados() {
        let mut cola = Cola::default();
        let mut extras = extras_sincronizacion(None, &["/var/www/app/a", "/var/www/app/b"]);
        extras.sincronizacion.as_mut().unwrap().nombre = None;
        let id = encolar_con(
            &mut cola,
            1,
            "/home/hector/web/a",
            extras,
            Some(EtiquetaTransferencia::Sincronizacion),
        );
        cola.despachar();
        {
            let transferencia = cola.obtener_mut(id).unwrap();
            transferencia.info.estado = EstadoTransferencia::Hecha;
            transferencia.info.borrados = 1;
            transferencia.conservados = 1;
        }
        let cierre = cola.cerrar(id).unwrap();
        let (_, detalle, resultado) = cierre.anotacion.unwrap();
        assert_eq!(resultado, ResultadoRegistro::Ok);
        assert!(
            detalle
                .starts_with("ad hoc · subida · +12 ~3 −1 · 1 directorio no vacío conservado · "),
            "{detalle}"
        );
        assert!(detalle.ends_with(" · ok"), "{detalle}");
        assert_eq!(cierre.sincronizacion_id, None, "una ad hoc no tiene fila");
    }

    #[test]
    fn abortar_cierra_ya_las_que_esperaban_y_deja_a_su_motor_las_que_corren() {
        let mut cola = Cola::default();
        let corre = encolar_con(
            &mut cola,
            1,
            "/home/hector/web/a",
            extras_sincronizacion(Some(1), &[]),
            Some(EtiquetaTransferencia::Sincronizacion),
        );
        let espera = encolar_con(
            &mut cola,
            1,
            "/home/hector/web/b",
            extras_sincronizacion(Some(2), &[]),
            Some(EtiquetaTransferencia::Sincronizacion),
        );
        let normal_en_cola = encolar(&mut cola, 1, "c");
        cola.despachar();

        assert!(cola.abortar_host(1, "conexión caída"));
        let cierres = cola.cerrar_sin_motor();
        let ids: Vec<Option<i64>> = cierres.iter().map(|c| c.sincronizacion_id).collect();
        assert_eq!(ids, vec![Some(2), None], "solo las que no corrían");
        assert_eq!(cierres[0].resultado, ResultadoSincronizacion::Error);
        assert!(cierres[0].anotacion.is_some(), "la sincronización se anota");
        assert!(
            cierres[1].anotacion.is_none(),
            "una transferencia normal que no arrancó no deja fila"
        );
        assert!(cola.cerrar_sin_motor().is_empty(), "una sola vez");
        assert!(cola.obtener(espera).unwrap().cerrada);
        assert!(cola.obtener(normal_en_cola).unwrap().cerrada);

        // La que corría no se quita de la cola hasta que su motor la cierre.
        assert_eq!(cola.limpiar(), 2);
        assert!(cola.obtener(corre).is_some());

        // Su motor vuelve: el estado sigue siendo el error de la caída.
        let transferencia = cola.obtener_mut(corre).unwrap();
        assert!(transferencia.abortada);
        assert_eq!(transferencia.info.estado, EstadoTransferencia::Error);
        let cierre = cola.cerrar(corre).unwrap();
        assert_eq!(cierre.sincronizacion_id, Some(1));
        assert!(cola.cerrar(corre).is_none());
    }

    #[test]
    fn el_apagado_cierra_todo_lo_que_quede_sin_cerrar_y_el_motor_ya_no() {
        let mut cola = Cola::default();
        let corre = encolar_con(
            &mut cola,
            1,
            "/home/hector/web/a",
            extras_sincronizacion(Some(1), &["/var/www/app/x"]),
            Some(EtiquetaTransferencia::Sincronizacion),
        );
        let abortada = encolar_con(
            &mut cola,
            2,
            "/home/hector/web/b",
            extras_sincronizacion(Some(2), &[]),
            Some(EtiquetaTransferencia::Sincronizacion),
        );
        let hecha = encolar(&mut cola, 3, "c");
        cola.despachar();
        // Una ya abortada por una caída cuyo motor aún no volvió, y otra que
        // terminó y se cerró del todo.
        cola.abortar_host(2, "conexión caída");
        cola.obtener(abortada)
            .unwrap()
            .escrito
            .store(true, Ordering::Relaxed);
        cola.obtener_mut(hecha).unwrap().info.estado = EstadoTransferencia::Hecha;
        cola.cerrar(hecha);

        let cierres = cola.abortar_todas("servidor detenido");
        let mut ids: Vec<Option<i64>> = cierres.iter().map(|c| c.sincronizacion_id).collect();
        ids.sort();
        assert_eq!(ids, vec![Some(1), Some(2)]);
        assert!(cierres.iter().all(|c| c.anotacion.is_some()));
        let de_la_abortada = cierres
            .iter()
            .find(|c| c.sincronizacion_id == Some(2))
            .unwrap();
        assert_eq!(de_la_abortada.resultado, ResultadoSincronizacion::Parcial);
        assert_eq!(
            cola.obtener(corre).unwrap().info.error.as_deref(),
            Some("servidor detenido")
        );
        assert!(cola.abortar_todas("otra vez").is_empty());
        assert!(cola.cerrar(corre).is_none(), "su motor ya no anota");
    }

    #[test]
    fn una_cancelada_en_cola_se_cierra_sin_motor() {
        let mut cola = Cola::default();
        let mut extras = extras_sincronizacion(Some(5), &[]);
        extras.deliberacion = Some(deliberacion_fijada(3));
        encolar(&mut cola, 1, "a");
        let espera = encolar_con(
            &mut cola,
            1,
            "/home/hector/web/a",
            extras,
            Some(EtiquetaTransferencia::Sincronizacion),
        );
        cola.despachar();
        assert!(cola.cancelar(espera));
        let cierres = cola.cerrar_sin_motor();
        assert_eq!(cierres.len(), 1);
        assert_eq!(cierres[0].resultado, ResultadoSincronizacion::Cancelada);
        assert_eq!(
            cierres[0].deliberacion.as_ref().map(|d| d.registro.id),
            Some(3),
            "la deliberación se cierra aunque no llegara a copiar"
        );
    }

    #[test]
    fn una_deliberacion_en_la_cola_cuenta_como_usada_aunque_haya_terminado() {
        let mut cola = Cola::default();
        let extras = ExtrasTransferencia {
            deliberacion: Some(deliberacion_fijada(8)),
            ..ExtrasTransferencia::default()
        };
        let id = encolar_con(&mut cola, 1, "a", extras, None);
        assert!(cola.deliberacion_usada(8));
        assert!(!cola.deliberacion_usada(9));
        cola.obtener_mut(id).unwrap().info.estado = EstadoTransferencia::Hecha;
        assert!(cola.deliberacion_usada(8));
    }

    #[test]
    fn las_rutas_se_comparan_por_componentes() {
        assert_eq!(
            componentes_absolutos("/var//www/app/"),
            Some(vec!["var", "www", "app"])
        );
        assert_eq!(componentes_absolutos("var/www"), None, "relativa");
        assert_eq!(componentes_absolutos("/var/www/../etc"), None);
        assert_eq!(componentes_absolutos("/var/./www"), None);
        let raiz = componentes_absolutos("/var/www/app").unwrap();
        assert!(cuelga_de("/var/www/app/a.css", &raiz).is_some());
        assert!(cuelga_de("/var/www/app", &raiz).is_none(), "estrictamente");
        assert!(
            cuelga_de("/var/www/app2/a.css", &raiz).is_none(),
            "no por texto"
        );
        assert!(cuelga_de("/var/www", &raiz).is_none());
    }

    #[test]
    fn una_sincronizacion_solo_escribe_y_borra_bajo_su_raiz() {
        let sinc = || Some(sincronizacion(None, None));
        let raiz = "/var/www/app";
        assert!(validar_peticion(&peticion(
            &["/var/www/app/viejo.css", "/var/www/app/tmp"],
            &["/var/www/app/nuevo.css"],
            sinc()
        ))
        .is_ok());
        // Solo borrados (el resto ya estaba al día) también vale.
        assert!(validar_peticion(&peticion(&["/var/www/app/viejo.css"], &[], sinc())).is_ok());
        for (borrar, destinos) in [
            (vec!["/var/www/app/../etc/passwd"], vec![]),
            (vec!["/var/www/app"], vec![]),
            (vec!["/var/www/otra/x"], vec![]),
            (vec!["viejo.css"], vec![]),
            (vec![], vec!["/etc/cron.d/x"]),
            (vec![], vec!["/var/www/app/./x"]),
            (vec!["/var/www/app/a"], vec!["/var/www/app//a"]),
        ] {
            assert!(
                validar_peticion(&peticion(&borrar, &destinos, sinc())).is_err(),
                "{borrar:?} {destinos:?} bajo {raiz}"
            );
        }
        let mut fuera = sincronizacion(None, None);
        fuera.raiz_destino = "/var/www/../app".to_string();
        assert!(validar_peticion(&peticion(&[], &["/app/x"], Some(fuera))).is_err());
    }

    #[test]
    fn borrar_al_terminar_exige_una_sincronizacion_coherente() {
        assert!(
            validar_peticion(&peticion(&["/var/www/app/x"], &["/var/www/app/y"], None)).is_err(),
            "sin sincronización no se borra nada"
        );
        assert!(
            validar_peticion(&peticion(&[], &[], None)).is_err(),
            "vacía"
        );
        assert!(validar_peticion(&peticion(&[], &["/cualquier/sitio"], None)).is_ok());
        let mut sin_etiqueta = peticion(&[], &["/var/www/app/y"], Some(sincronizacion(None, None)));
        sin_etiqueta.etiqueta = None;
        assert!(validar_peticion(&sin_etiqueta).is_err());
        let mut sin_datos = peticion(&[], &["/var/www/app/y"], None);
        sin_datos.etiqueta = Some(EtiquetaTransferencia::Sincronizacion);
        assert!(validar_peticion(&sin_datos).is_err());
    }

    #[tokio::test]
    async fn borrar_rutas_va_de_ficheros_a_directorios_del_mas_profundo_y_conserva_los_no_vacios() {
        let raiz = tempfile::tempdir().unwrap();
        let r = |relativa: &str| raiz.path().join(relativa).display().to_string();
        std::fs::create_dir_all(raiz.path().join("viejo/hondo")).unwrap();
        std::fs::write(raiz.path().join("viejo/hondo/a.txt"), b"a").unwrap();
        std::fs::write(raiz.path().join("viejo/b.txt"), b"b").unwrap();
        std::fs::create_dir_all(raiz.path().join("conserva")).unwrap();
        std::fs::write(raiz.path().join("conserva/excluido.log"), b"x").unwrap();
        std::fs::create_dir_all(raiz.path().join("fuera")).unwrap();
        std::fs::write(raiz.path().join("fuera/dentro.txt"), b"f").unwrap();
        std::os::unix::fs::symlink(raiz.path().join("fuera"), raiz.path().join("enlace")).unwrap();

        // Los directorios, a propósito, de menos profundo a más.
        let rutas = vec![
            r("viejo"),
            r("viejo/hondo"),
            r("conserva"),
            r("viejo/b.txt"),
            r("ya_no_esta.txt"),
            r("enlace"),
            r("viejo/hondo/a.txt"),
        ];
        let cancelada = AtomicBool::new(false);
        let escrito = AtomicBool::new(false);
        let resultado = borrar_rutas(
            &LadoDestino::Local,
            &raiz.path().display().to_string(),
            &rutas,
            &cancelada,
            &escrito,
            None,
        )
        .await;
        assert_eq!(
            resultado,
            ResultadoBorrado {
                borrados: 6,
                conservados: 1,
                fallos: Vec::new(),
                cancelado: false,
            }
        );
        assert!(escrito.load(Ordering::Relaxed));
        assert!(!raiz.path().join("viejo").exists());
        assert!(!raiz.path().join("enlace").exists());
        assert!(
            raiz.path().join("fuera/dentro.txt").exists(),
            "un enlace se borra como enlace, nunca se sigue"
        );
        assert!(raiz.path().join("conserva/excluido.log").exists());
    }

    #[tokio::test]
    async fn borrar_rutas_para_con_la_cancelacion() {
        let raiz = tempfile::tempdir().unwrap();
        std::fs::write(raiz.path().join("a"), b"a").unwrap();
        let rutas = vec![raiz.path().join("a").display().to_string()];
        let cancelada = AtomicBool::new(true);
        let escrito = AtomicBool::new(false);
        let resultado = borrar_rutas(
            &LadoDestino::Local,
            &raiz.path().display().to_string(),
            &rutas,
            &cancelada,
            &escrito,
            None,
        )
        .await;
        assert!(resultado.cancelado);
        assert_eq!(resultado.borrados, 0);
        assert!(raiz.path().join("a").exists());
    }

    /// Si un directorio del destino se cambió por un enlace después de
    /// planificar, lo que cuelga de él no se borra: estaría fuera del árbol.
    #[tokio::test]
    async fn borrar_rutas_no_sigue_un_enlace_puesto_entre_la_raiz_y_la_ruta() {
        let raiz = tempfile::tempdir().unwrap();
        let fuera = tempfile::tempdir().unwrap();
        std::fs::write(fuera.path().join("ajeno.txt"), b"no es nuestro").unwrap();
        std::fs::create_dir_all(raiz.path().join("sitio")).unwrap();
        std::os::unix::fs::symlink(fuera.path(), raiz.path().join("sitio/static")).unwrap();
        std::fs::write(raiz.path().join("sitio/viejo.txt"), b"v").unwrap();
        // La raíz misma sí puede ser un enlace (un `current -> releases/12`).
        let enlace_raiz = fuera.path().join("raiz");
        std::os::unix::fs::symlink(raiz.path(), &enlace_raiz).unwrap();
        let rutas: Vec<String> = ["sitio/static/ajeno.txt", "sitio/viejo.txt"]
            .iter()
            .map(|relativa| enlace_raiz.join(relativa).display().to_string())
            .collect();
        let cancelada = AtomicBool::new(false);
        let escrito = AtomicBool::new(false);
        let resultado = borrar_rutas(
            &LadoDestino::Local,
            &enlace_raiz.display().to_string(),
            &rutas,
            &cancelada,
            &escrito,
            None,
        )
        .await;
        assert_eq!(resultado.borrados, 1, "{resultado:?}");
        assert_eq!(resultado.fallos.len(), 1, "{resultado:?}");
        assert!(resultado.fallos[0].contains("cuelga de un enlace"));
        assert!(
            fuera.path().join("ajeno.txt").exists(),
            "fuera del árbol no se borra"
        );
        assert!(!raiz.path().join("sitio/viejo.txt").exists());
    }
}
