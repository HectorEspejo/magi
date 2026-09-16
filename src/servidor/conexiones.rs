//! Pool de conexiones `host_id → Handle` del servidor: con `multiplexar`
//! marcado, una segunda sesión al mismo host abre un canal nuevo sobre la
//! conexión ya autenticada, sin reautenticar. Una conexión del pool sin
//! canales se cierra tras la gracia de 30 s.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use russh::client::Handle;
use russh::Disconnect;
use tokio::sync::mpsc;
use tracing::warn;

use crate::conexion::cliente::Cliente;
use crate::conexion::salto::Transporte;
use crate::conexion::EventoConexion;
use crate::config::Rutas;
use crate::modelo::Host;

use super::EstadoServidor;

/// Gracia de una conexión del pool sin canales antes de cerrarla.
pub const GRACIA_SIN_CANALES: Duration = Duration::from_secs(30);

pub struct Entrada {
    pub handle: Arc<Handle<Cliente>>,
    /// Conexiones intermedias de la cadena de saltos, también compartidas.
    pub saltos: Vec<Arc<Handle<Cliente>>>,
    pub canales: u32,
    pub sin_canales_desde: Option<Instant>,
}

#[derive(Default)]
pub struct Pool {
    entradas: HashMap<i64, Entrada>,
}

/// Conexión vencida que devuelve `recoger_vencidas` para desconectarla.
pub type Vencida = (i64, Arc<Handle<Cliente>>, Vec<Arc<Handle<Cliente>>>);

/// Conexión lista para cerrar, con sus saltos.
pub type Cerrable = (Arc<Handle<Cliente>>, Vec<Arc<Handle<Cliente>>>);

impl Pool {
    /// Devuelve la conexión viva del host para abrir otro canal, contándolo.
    pub fn reutilizar(&mut self, host_id: i64) -> Option<Arc<Handle<Cliente>>> {
        let entrada = self.entradas.get_mut(&host_id)?;
        entrada.canales += 1;
        entrada.sin_canales_desde = None;
        Some(entrada.handle.clone())
    }

    /// Devuelve la conexión viva del host sin tocar su contador de canales.
    /// Solo para quien acaba de contar un canal con `reutilizar`.
    pub fn handle_de(&self, host_id: i64) -> Option<Arc<Handle<Cliente>>> {
        self.entradas
            .get(&host_id)
            .map(|entrada| entrada.handle.clone())
    }

    /// Guarda un transporte recién abierto como conexión del pool, con el
    /// canal que acaba de abrirse contado. Devuelve la conexión que hubiera
    /// antes, si la había, para que el llamador la desconecte: dejarla
    /// suelta sería una conexión huérfana que nadie cerraría.
    pub fn guardar(&mut self, host_id: i64, transporte: Transporte) -> Option<Cerrable> {
        let anterior = self
            .entradas
            .remove(&host_id)
            .map(|entrada| (entrada.handle, entrada.saltos));
        self.entradas.insert(
            host_id,
            Entrada {
                handle: transporte.handle,
                saltos: transporte.saltos.into_iter().map(Arc::new).collect(),
                canales: 1,
                sin_canales_desde: None,
            },
        );
        anterior
    }

    /// Descuenta un canal **solo si la entrada sigue siendo esa conexión**.
    /// Con dos conexiones al mismo host que se turnan en el pool, descontar a
    /// ciegas podría dejar a cero el contador de otra viva (y cerrarla).
    pub fn liberar_si_es(&mut self, host_id: i64, handle: &Arc<Handle<Cliente>>) {
        let Some(entrada) = self.entradas.get_mut(&host_id) else {
            return;
        };
        if !Arc::ptr_eq(&entrada.handle, handle) {
            return;
        }
        entrada.canales = entrada.canales.saturating_sub(1);
        if entrada.canales == 0 {
            entrada.sin_canales_desde = Some(Instant::now());
        }
    }

    /// Descuenta un canal; al llegar a cero arranca la gracia de cierre.
    pub fn liberar(&mut self, host_id: i64) {
        if let Some(entrada) = self.entradas.get_mut(&host_id) {
            entrada.canales = entrada.canales.saturating_sub(1);
            if entrada.canales == 0 {
                entrada.sin_canales_desde = Some(Instant::now());
            }
        }
    }

    /// Quita las conexiones sin canales que agotaron la gracia para que el
    /// llamador las desconecte fuera del bloqueo.
    pub fn recoger_vencidas(&mut self) -> Vec<Vencida> {
        let mut vencidas = Vec::new();
        let ids: Vec<i64> = self
            .entradas
            .iter()
            .filter(|(_, entrada)| {
                entrada.canales == 0
                    && entrada
                        .sin_canales_desde
                        .is_some_and(|desde| desde.elapsed() >= GRACIA_SIN_CANALES)
            })
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            if let Some(entrada) = self.entradas.remove(&id) {
                vencidas.push((id, entrada.handle, entrada.saltos));
            }
        }
        vencidas
    }

    /// Quita y devuelve todas las conexiones (para el apagado).
    pub fn vaciar(&mut self) -> Vec<Cerrable> {
        self.entradas
            .drain()
            .map(|(_, entrada)| (entrada.handle, entrada.saltos))
            .collect()
    }
}

/// Conexión para abrir un canal nuevo de este host: la del pool si la hay (el
/// canal cuenta como suyo, así que el pool no la cerrará mientras viva) o una
/// recién abierta con el flujo de la Fase 3, que dialoga con el solicitante.
///
/// Con `multiplexar` la conexión nueva queda en el pool para el siguiente
/// canal; sin él, se devuelve como propia y la cierra quien la pidió. Lo
/// comparten los canales SFTP, los túneles y las pestañas que multiplexan.
pub async fn conexion_para_canal(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    host: &Host,
    todos_los_hosts: &HashMap<i64, Host>,
    rutas: &Rutas,
    id_solicitud: u32,
    solicitante: u32,
) -> Result<(Arc<Handle<Cliente>>, Option<Transporte>), String> {
    let handle_del_pool = {
        let mut estado_bloqueado = estado.lock().await;
        estado_bloqueado.pool.reutilizar(host.id)
    };
    if let Some(handle) = handle_del_pool {
        return Ok((handle, None));
    }

    let reenvios = estado.lock().await.reenvios.clone();
    let (tx_eventos, rx_eventos) = mpsc::unbounded_channel::<EventoConexion>();
    // El puente traduce los diálogos (huella, frase, contraseña) al solicitante
    // y se deja vivo: el handler de russh conserva un clon del canal de eventos
    // mientras la conexión viva, así que muere solo cuando la conexión cae.
    tokio::spawn(crate::servidor::sesiones::puente_eventos(
        estado.clone(),
        id_solicitud,
        solicitante,
        rx_eventos,
    ));
    let cadena = crate::conexion::salto::construir_cadena(host, todos_los_hosts)
        .map_err(|error| error.to_string())?;
    let contexto = crate::conexion::cliente::Contexto {
        known_hosts: rutas.fichero_known_hosts(),
        dir_ssh: rutas.dir_ssh(),
        hogar: rutas.hogar.clone(),
        usuario_local: crate::conexion::usuario_local(),
        tx: tx_eventos.clone(),
        interactivo: true,
        fuente_contrasena: crate::conexion::FuenteContrasena::Solicitante,
        reenvios: Some(reenvios),
    };
    let transporte = crate::conexion::salto::conectar_cadena(&cadena, &contexto)
        .await
        .map_err(|error| error.to_string())?;
    if !host.multiplexar {
        return Ok((transporte.handle.clone(), Some(transporte)));
    }
    let handle = transporte.handle.clone();
    let desplazada = estado.lock().await.pool.guardar(host.id, transporte);
    // Si el pool ya tenía otra conexión para este host, se quedó fuera al
    // sustituirla: se cierra aquí, que nadie más lo va a hacer.
    if let Some((handle_viejo, saltos_viejos)) = desplazada {
        desconectar(handle_viejo, saltos_viejos).await;
    }
    Ok((handle, None))
}

/// Suelta lo que se hubiera tomado para un canal: la conexión propia se cierra
/// y la del pool se libera por identidad (nunca a ciegas).
pub async fn soltar(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    host_id: i64,
    handle: &Arc<Handle<Cliente>>,
    propia: Option<Transporte>,
) {
    match propia {
        Some(transporte) => {
            desconectar(
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

/// Desconecta una conexión del pool (destino y saltos) descartando errores:
/// puede que el remoto ya la haya cerrado.
pub async fn desconectar(handle: Arc<Handle<Cliente>>, saltos: Vec<Arc<Handle<Cliente>>>) {
    for salto in saltos {
        if let Err(error) = salto.disconnect(Disconnect::ByApplication, "", "").await {
            warn!("no se pudo cerrar una conexión de salto del pool: {error}");
        }
    }
    if let Err(error) = handle.disconnect(Disconnect::ByApplication, "", "").await {
        warn!("no se pudo cerrar una conexión del pool: {error}");
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn la_gracia_sin_canales_es_de_30_segundos() {
        assert_eq!(GRACIA_SIN_CANALES, Duration::from_secs(30));
    }
}
