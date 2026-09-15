//! Pool de conexiones `host_id → Handle` del servidor: con `multiplexar`
//! marcado, una segunda sesión al mismo host abre un canal nuevo sobre la
//! conexión ya autenticada, sin reautenticar. Una conexión del pool sin
//! canales se cierra tras la gracia de 30 s.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use russh::client::Handle;
use russh::Disconnect;
use tracing::warn;

use crate::conexion::cliente::Cliente;
use crate::conexion::salto::Transporte;

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
