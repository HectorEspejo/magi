//! Reenvíos remotos (`-R`) vistos desde la conexión: el registro de qué
//! `(host, dirección, puerto)` tiene un túnel detrás, los contadores de tráfico
//! de cada conexión aceptada y la copia bidireccional con búfer de 64 KiB.
//!
//! El registro es del proceso, no de la conexión: un túnel remoto puede vivir
//! sobre una conexión del pool que abrió una pestaña o el SFTP, así que el
//! handler que recibe el canal `forwarded-tcpip` no tiene por qué ser el de la
//! conexión que pidió el reenvío. Se busca por host.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tokio::sync::watch;
use tracing::warn;

/// Búfer de la copia: 64 KiB, que es lo que pide el rendimiento de la fase.
pub const TAM_BLOQUE: usize = 64 * 1024;

/// Plazo de las esperas de red de un túnel (conectar al destino local).
pub const PLAZO: Duration = Duration::from_secs(10);

/// Contadores en vivo de un túnel, compartidos con las tareas que copian.
#[derive(Default)]
pub struct Contadores {
    subidos: AtomicU64,
    bajados: AtomicU64,
    abiertas: AtomicU32,
    aceptadas: AtomicU64,
    /// Último error de una conexión suelta: no tumba el túnel, pero se enseña.
    ultimo_error: Mutex<Option<String>>,
}

impl Contadores {
    /// Una conexión queda establecida.
    pub fn abrir(&self) {
        self.abiertas.fetch_add(1, Ordering::Relaxed);
        self.aceptadas.fetch_add(1, Ordering::Relaxed);
    }

    /// La conexión se cerró. Nunca baja de cero.
    pub fn cerrar(&self) {
        let _ = self
            .abiertas
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |valor| {
                Some(valor.saturating_sub(1))
            });
    }

    pub fn anotar_error(&self, motivo: String) {
        if let Ok(mut guardado) = self.ultimo_error.lock() {
            *guardado = Some(motivo);
        }
    }

    pub fn error(&self) -> Option<String> {
        self.ultimo_error.lock().ok().and_then(|e| e.clone())
    }

    /// `(abiertas, aceptadas, subidos, bajados)`.
    pub fn instantanea(&self) -> (u32, u64, u64, u64) {
        (
            self.abiertas.load(Ordering::Relaxed),
            self.aceptadas.load(Ordering::Relaxed),
            self.subidos.load(Ordering::Relaxed),
            self.bajados.load(Ordering::Relaxed),
        )
    }
}

/// Copia hasta que la conexión se corte **o hasta que el túnel se pare**: al
/// parar hay que cortar las conexiones abiertas, no dejarlas llevando tráfico
/// de un túnel que ya no existe.
pub async fn copiar_hasta_que_paren<L, R>(
    local: &mut L,
    remoto: &mut R,
    contadores: &Contadores,
    aviso: &mut watch::Receiver<bool>,
) where
    L: AsyncRead + AsyncWrite + Unpin,
    R: AsyncRead + AsyncWrite + Unpin,
{
    tokio::select! {
        resultado = copiar(local, remoto, contadores) => {
            if let Err(error) = resultado {
                contadores.anotar_error(format!("se cortó la conexión: {error}"));
            }
        }
        // También llega aquí si se suelta el emisor sin avisar (el túnel
        // desapareció): en los dos casos, la copia se acaba.
        _ = aviso.changed() => {}
    }
}

/// Copia en los dos sentidos con búfer de 64 KiB.
///
/// `local` es el extremo de esta máquina (el `TcpStream` aceptado o conectado) y
/// `remoto` el canal SSH: lo que va del remoto al local cuenta como bajado y lo
/// que va del local al remoto, como subido. Cada mitad se cierra en cuanto la
/// otra llega a fin de fichero, como `copy_bidirectional`.
pub async fn copiar<L, R>(
    local: &mut L,
    remoto: &mut R,
    contadores: &Contadores,
) -> std::io::Result<()>
where
    L: AsyncRead + AsyncWrite + Unpin,
    R: AsyncRead + AsyncWrite + Unpin,
{
    let mut hacia_remoto = vec![0u8; TAM_BLOQUE];
    let mut hacia_local = vec![0u8; TAM_BLOQUE];
    let mut local_abierto = true;
    let mut remoto_abierto = true;
    while local_abierto || remoto_abierto {
        tokio::select! {
            leido = local.read(&mut hacia_remoto), if local_abierto => {
                let leido = leido?;
                if leido == 0 {
                    local_abierto = false;
                    remoto.shutdown().await?;
                } else {
                    remoto.write_all(&hacia_remoto[..leido]).await?;
                    contadores.subidos.fetch_add(leido as u64, Ordering::Relaxed);
                }
            }
            leido = remoto.read(&mut hacia_local), if remoto_abierto => {
                let leido = leido?;
                if leido == 0 {
                    remoto_abierto = false;
                    local.shutdown().await?;
                } else {
                    local.write_all(&hacia_local[..leido]).await?;
                    contadores.bajados.fetch_add(leido as u64, Ordering::Relaxed);
                }
            }
        }
    }
    Ok(())
}

/// Un reenvío remoto vivo: a qué túnel pertenece y a dónde va cada canal.
pub struct Reenvio {
    pub tunel_id: i64,
    pub destino: String,
    pub contadores: Arc<Contadores>,
    /// Aviso de que el túnel se ha parado: las copias en curso se cortan.
    pub cancelacion: watch::Receiver<bool>,
}

/// Clave de un reenvío registrado: el host, porque la dirección que manda el
/// host puede ser la que se pidió o la que él eligió.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Clave {
    host_id: i64,
    direccion: String,
    puerto: u32,
}

/// Reenvíos registrados por las conexiones vivas del proceso.
#[derive(Default)]
pub struct Reenvios {
    mapa: Mutex<HashMap<Clave, Arc<Reenvio>>>,
}

impl Reenvios {
    /// Registra el reenvío que acaba de aceptar el host.
    pub fn registrar(&self, host_id: i64, direccion: &str, puerto: u32, reenvio: Arc<Reenvio>) {
        if let Ok(mut mapa) = self.mapa.lock() {
            mapa.insert(
                Clave {
                    host_id,
                    direccion: direccion.to_string(),
                    puerto,
                },
                reenvio,
            );
        }
    }

    /// Quita el reenvío al parar el túnel. Devuelve si estaba.
    pub fn quitar(&self, host_id: i64, direccion: &str, puerto: u32) -> bool {
        self.mapa
            .lock()
            .map(|mut mapa| {
                mapa.remove(&Clave {
                    host_id,
                    direccion: direccion.to_string(),
                    puerto,
                })
                .is_some()
            })
            .unwrap_or(false)
    }

    /// Busca el reenvío de un canal que llega del host. Se prueba la clave
    /// exacta y, si no está, por puerto cuando en ese host solo haya uno: el
    /// host puede contestar con la dirección a la que escuchó de verdad
    /// (`0.0.0.0`, `localhost`) en vez de con la que se le pidió.
    pub fn buscar(&self, host_id: i64, direccion: &str, puerto: u32) -> Option<Arc<Reenvio>> {
        let mapa = self.mapa.lock().ok()?;
        if let Some(reenvio) = mapa.get(&Clave {
            host_id,
            direccion: direccion.to_string(),
            puerto,
        }) {
            return Some(reenvio.clone());
        }
        let mut candidatos = mapa
            .iter()
            .filter(|(clave, _)| clave.host_id == host_id && clave.puerto == puerto)
            .map(|(_, reenvio)| reenvio.clone());
        let primero = candidatos.next()?;
        match candidatos.next() {
            // Ambiguo: más de uno escuchando en el mismo puerto del mismo host.
            Some(_) => None,
            None => Some(primero),
        }
    }
}

/// Atiende un canal `forwarded-tcpip`: conecta con el destino local y copia.
/// Solo se llama con la clave ya registrada.
pub async fn atender(
    reenvio: Arc<Reenvio>,
    canal: russh::Channel<russh::client::Msg>,
    origen: String,
) {
    let mut flujo = canal.into_stream();
    let conexion = tokio::time::timeout(PLAZO, TcpStream::connect(&reenvio.destino)).await;
    let mut tcp = match conexion {
        Ok(Ok(tcp)) => tcp,
        Ok(Err(error)) => {
            warn!(destino = %reenvio.destino, origen = %origen, "no se pudo conectar el reenvío: {error}");
            reenvio.contadores.anotar_error(format!(
                "no se pudo conectar a {}: {error}",
                reenvio.destino
            ));
            let _ = flujo.shutdown().await;
            return;
        }
        Err(_) => {
            warn!(destino = %reenvio.destino, origen = %origen, "se agotó el plazo conectando el reenvío");
            reenvio.contadores.anotar_error(format!(
                "se agotó el plazo conectando a {}",
                reenvio.destino
            ));
            let _ = flujo.shutdown().await;
            return;
        }
    };
    let _ = tcp.set_nodelay(true);
    reenvio.contadores.abrir();
    let mut aviso = reenvio.cancelacion.clone();
    copiar_hasta_que_paren(&mut tcp, &mut flujo, &reenvio.contadores, &mut aviso).await;
    reenvio.contadores.cerrar();
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn los_contadores_de_conexiones_no_bajan_de_cero() {
        let contadores = Contadores::default();
        contadores.cerrar();
        assert_eq!(contadores.instantanea().0, 0);
        contadores.abrir();
        contadores.abrir();
        assert_eq!(contadores.instantanea(), (2, 2, 0, 0));
        contadores.cerrar();
        assert_eq!(contadores.instantanea(), (1, 2, 0, 0), "aceptadas no baja");
    }

    #[test]
    fn el_registro_encuentra_por_clave_exacta_y_por_puerto() {
        let reenvios = Reenvios::default();
        let (_tx, aviso) = watch::channel(false);
        let reenvio = Arc::new(Reenvio {
            tunel_id: 7,
            destino: "127.0.0.1:9000".to_string(),
            contadores: Arc::new(Contadores::default()),
            cancelacion: aviso,
        });
        reenvios.registrar(3, "127.0.0.1", 9000, reenvio.clone());
        assert_eq!(
            reenvios.buscar(3, "127.0.0.1", 9000).map(|r| r.tunel_id),
            Some(7)
        );
        // El host contesta con otra dirección: se encuentra por el puerto.
        assert_eq!(
            reenvios.buscar(3, "localhost", 9000).map(|r| r.tunel_id),
            Some(7)
        );
        // Otro host no lo ve.
        assert!(reenvios.buscar(9, "127.0.0.1", 9000).is_none());
        // Otro puerto tampoco.
        assert!(reenvios.buscar(3, "127.0.0.1", 9001).is_none());
        assert!(reenvios.quitar(3, "127.0.0.1", 9000));
        assert!(!reenvios.quitar(3, "127.0.0.1", 9000));
    }

    /// Con dos túneles del mismo host en el mismo puerto, una clave que no case
    /// exactamente es ambigua y se rechaza en vez de adivinar.
    #[test]
    fn una_clave_ambigua_no_se_atiende() {
        let reenvios = Reenvios::default();
        let (_tx, aviso) = watch::channel(false);
        for (direccion, tunel_id) in [("127.0.0.1", 1), ("::1", 2)] {
            reenvios.registrar(
                3,
                direccion,
                9000,
                Arc::new(Reenvio {
                    tunel_id,
                    destino: "127.0.0.1:9000".to_string(),
                    contadores: Arc::new(Contadores::default()),
                    cancelacion: aviso.clone(),
                }),
            );
        }
        assert!(reenvios.buscar(3, "localhost", 9000).is_none());
        assert_eq!(reenvios.buscar(3, "::1", 9000).map(|r| r.tunel_id), Some(2));
    }
}
