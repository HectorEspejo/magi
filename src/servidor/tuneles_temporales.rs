//! Túneles temporales del servidor (Fase 15): un enlace a un localhost del
//! host a través de la conexión de su pestaña.
//!
//! Escuchan siempre en `127.0.0.1` (puerto libre), solo admiten como destino
//! `localhost`, `127.0.0.1` o `::1` del host, son trabajo del servidor
//! (`TrabajoServidor.tuneles_temporales`) y se cierran con su pestaña. La
//! mecánica de cada conexión la pone `magi_nucleo::tuneles`, compartida con los
//! túneles de `TUNELES`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::sync::{oneshot, watch};
use tracing::warn;

use magi_nucleo::conexion::reenvios::Contadores;
use magi_nucleo::tuneles::escucha::escucha_de;
use magi_nucleo::tuneles::local::servir_local;

use crate::modelo::ResultadoRegistro;
use crate::protocolo::{InfoTunelTemporal, MensajeServidor};

use super::conexiones::{self, ConexionTomada, PoliticaConexion, UsoCanal};
use super::{difusion, EstadoServidor};

/// Plazo para abrir el túnel (la conexión y la escucha) (`5.2: 10 s).
const PLAZO: Duration = Duration::from_secs(10);

/// Cada cuánto se revisan las conexiones caídas y los contadores.
const REVISION: Duration = Duration::from_millis(500);

/// Destinos admitidos: solo el propio equipo del host.
pub const DESTINOS: [&str; 3] = ["localhost", "127.0.0.1", "::1"];

/// Un túnel temporal abierto.
struct TunelTemporal {
    id: u32,
    sesion_id: u32,
    sesion_nombre: String,
    host_id: i64,
    host_nombre: String,
    puerto_local: u16,
    destino: String,
    puerto: u16,
    contadores: Arc<Contadores>,
    conexion: Option<ConexionTomada>,
    parada: Option<oneshot::Sender<()>>,
    cancelacion: watch::Sender<bool>,
}

impl TunelTemporal {
    fn info(&self) -> InfoTunelTemporal {
        let (conexiones, _, _, _) = self.contadores.instantanea();
        InfoTunelTemporal {
            id: self.id,
            sesion_id: self.sesion_id,
            host_id: self.host_id,
            host_nombre: self.host_nombre.clone(),
            puerto_local: self.puerto_local,
            destino: self.destino.clone(),
            puerto: self.puerto,
            conexiones,
        }
    }
}

/// Los túneles temporales del servidor.
#[derive(Default)]
pub struct Temporales {
    activos: HashMap<u32, TunelTemporal>,
    siguiente_id: u32,
    /// Último recuento de conexiones difundido por túnel, para no repetir.
    contadas: HashMap<u32, u32>,
}

impl Temporales {
    fn nuevo_id(&mut self) -> u32 {
        self.siguiente_id += 1;
        self.siguiente_id
    }

    /// Cuántos túneles temporales hay abiertos.
    pub fn cuenta(&self) -> u32 {
        self.activos.len() as u32
    }
}

/// Lista completa de los túneles temporales, para la difusión y la bienvenida.
pub fn lista(estado: &EstadoServidor) -> Vec<InfoTunelTemporal> {
    let mut lista: Vec<InfoTunelTemporal> = estado
        .tuneles_temporales
        .activos
        .values()
        .map(TunelTemporal::info)
        .collect();
    lista.sort_by_key(|tunel| tunel.id);
    lista
}

/// Cuántos hay (para `TrabajoServidor` y el aviso de parada).
pub fn cuenta(estado: &EstadoServidor) -> u32 {
    estado.tuneles_temporales.cuenta()
}

/// Difunde la lista completa en cada cambio (T27).
pub fn difundir(estado: &EstadoServidor) {
    let lista = lista(estado);
    difusion::difundir(
        &estado.clientes,
        MensajeServidor::TunelesTemporales { lista },
    );
}

/// `AbrirTunelTemporal`: escucha en `127.0.0.1` y reenvía al localhost del
/// host por la conexión viva de su pestaña (T42). Responde `TunelTemporal` o
/// `Error`.
pub async fn manejar_abrir(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    peticion_id: u64,
    sesion_id: u32,
    destino: String,
    puerto: u16,
) {
    if !DESTINOS.contains(&destino.as_str()) {
        fallar(
            estado,
            cliente_id,
            peticion_id,
            None,
            "el destino de un túnel temporal tiene que ser localhost del host",
        )
        .await;
        return;
    }
    if puerto == 0 {
        fallar(estado, cliente_id, peticion_id, None, "puerto no válido").await;
        return;
    }
    let Some((host_id, host_nombre, sesion_nombre, host, todos_los_hosts, rutas)) =
        datos(estado, sesion_id).await
    else {
        fallar(
            estado,
            cliente_id,
            peticion_id,
            None,
            "la pestaña ya no está abierta",
        )
        .await;
        return;
    };
    // La conexión de la pestaña, prestada o del pool: nunca abre otra ni
    // dialoga (la pestaña ya la tiene).
    let tomada = match tokio::time::timeout(
        PLAZO,
        conexiones::conexion_para_canal(
            estado,
            &host,
            &todos_los_hosts,
            &rutas,
            PoliticaConexion::SoloViva,
            UsoCanal::Subsistema,
        ),
    )
    .await
    {
        Ok(Ok(tomada)) => tomada,
        Ok(Err(motivo)) => {
            fallar(estado, cliente_id, peticion_id, Some(host_id), &motivo).await;
            return;
        }
        Err(_) => {
            fallar(
                estado,
                cliente_id,
                peticion_id,
                Some(host_id),
                "se agotó el plazo abriendo el túnel temporal",
            )
            .await;
            return;
        }
    };
    let escucha = match TcpListener::bind(("127.0.0.1", 0)).await {
        Ok(escucha) => escucha,
        Err(error) => {
            tomada.soltar().await;
            fallar(
                estado,
                cliente_id,
                peticion_id,
                Some(host_id),
                &format!("no se pudo escuchar en 127.0.0.1: {error}"),
            )
            .await;
            return;
        }
    };
    let puerto_local = match escucha_de(&escucha, "127.0.0.1").ok().and_then(|texto| {
        magi_nucleo::modelo::partir_direccion_puerto(&texto).map(|(_, puerto)| puerto)
    }) {
        Some(puerto) => puerto,
        None => {
            tomada.soltar().await;
            fallar(
                estado,
                cliente_id,
                peticion_id,
                Some(host_id),
                "no se pudo leer el puerto local del túnel temporal",
            )
            .await;
            return;
        }
    };
    let contadores = Arc::new(Contadores::default());
    let (cancelacion, aviso) = watch::channel(false);
    let remoto = magi_nucleo::modelo::juntar_direccion_puerto(&destino, puerto);
    let parada = servir_local(
        escucha,
        tomada.handle.clone(),
        remoto,
        contadores.clone(),
        aviso,
    );
    let id = {
        let mut estado_bloqueado = estado.lock().await;
        // La pestaña pudo cerrarse o la ventana que lo pidió irse mientras se
        // abría: en cualquiera de los dos casos el túnel no tiene a quién
        // servir y no se publica.
        let peticion_viva = estado_bloqueado
            .sesiones
            .get(&sesion_id)
            .is_some_and(|sesion| sesion.abierta())
            && estado_bloqueado.clientes.contains_key(&cliente_id);
        if !peticion_viva {
            drop(estado_bloqueado);
            let _ = parada;
            let _ = cancelacion.send(true);
            tomada.soltar().await;
            fallar(
                estado,
                cliente_id,
                peticion_id,
                Some(host_id),
                "la pestaña o la ventana que pidió el túnel se cerró",
            )
            .await;
            return;
        }
        let id = estado_bloqueado.tuneles_temporales.nuevo_id();
        estado_bloqueado.tuneles_temporales.activos.insert(
            id,
            TunelTemporal {
                id,
                sesion_id,
                sesion_nombre: sesion_nombre.clone(),
                host_id,
                host_nombre: host_nombre.clone(),
                puerto_local,
                destino: destino.clone(),
                puerto,
                contadores,
                conexion: Some(tomada),
                parada: Some(parada),
                cancelacion,
            },
        );
        let bd = estado_bloqueado.bd.clone();
        let _ = bd.send(super::OrdenBd::Anotar {
            tipo: crate::registro::TUNEL_ABIERTO.to_string(),
            host_id: Some(host_id),
            identidad_id: None,
            detalle: format!(
                "temporal · {host_nombre} · 127.0.0.1:{puerto_local} → {destino}:{puerto} · pestaña «{sesion_nombre}»"
            ),
            resultado: ResultadoRegistro::Ok,
        });
        id
    };
    {
        let estado_bloqueado = estado.lock().await;
        difundir(&estado_bloqueado);
    }
    responder(
        estado,
        cliente_id,
        MensajeServidor::TunelTemporal {
            peticion_id,
            id,
            puerto_local,
        },
    )
    .await;
}

/// `CerrarTunelTemporal`: cierra el túnel y su escucha.
pub async fn cerrar(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>, cliente_id: u32, id: u32) {
    let _ = cliente_id;
    cerrar_con(estado, id, "cerrado por el usuario").await;
}

/// La pestaña de un túnel se cerró o su conexión cayó: se cierra el túnel.
pub async fn sesion_acabo(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    sesion_id: u32,
    motivo: &str,
) {
    let ids: Vec<u32> = {
        let estado_bloqueado = estado.lock().await;
        estado_bloqueado
            .tuneles_temporales
            .activos
            .values()
            .filter(|tunel| tunel.sesion_id == sesion_id)
            .map(|tunel| tunel.id)
            .collect()
    };
    for id in ids {
        cerrar_con(estado, id, motivo).await;
    }
}

/// Cierra todos los túneles temporales (apagado del servidor).
pub async fn apagar(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>) {
    let ids: Vec<u32> = {
        let estado_bloqueado = estado.lock().await;
        estado_bloqueado
            .tuneles_temporales
            .activos
            .keys()
            .copied()
            .collect()
    };
    for id in ids {
        cerrar_con(estado, id, "el servidor se apaga").await;
    }
}

async fn cerrar_con(estado: &Arc<tokio::sync::Mutex<EstadoServidor>>, id: u32, motivo: &str) {
    let tunel = {
        let mut estado_bloqueado = estado.lock().await;
        estado_bloqueado.tuneles_temporales.activos.remove(&id)
    };
    let Some(mut tunel) = tunel else {
        return;
    };
    if let Some(parada) = tunel.parada.take() {
        let _ = parada.send(());
    }
    let _ = tunel.cancelacion.send(true);
    if let Some(conexion) = tunel.conexion.take() {
        conexion.soltar().await;
    }
    {
        let estado_bloqueado = estado.lock().await;
        let bd = estado_bloqueado.bd.clone();
        let _ = bd.send(super::OrdenBd::Anotar {
            tipo: crate::registro::TUNEL_CERRADO.to_string(),
            host_id: Some(tunel.host_id),
            identidad_id: None,
            detalle: format!(
                "temporal · {} · 127.0.0.1:{} → {}:{} · pestaña «{}» · {motivo}",
                tunel.host_nombre,
                tunel.puerto_local,
                tunel.destino,
                tunel.puerto,
                tunel.sesion_nombre
            ),
            resultado: ResultadoRegistro::Ok,
        });
        difundir(&estado_bloqueado);
    }
}

/// Revisora: cierra los túneles cuya conexión del host cayó y difunde los
/// cambios de contadores (sin repetir lo mismo cada 500 ms).
pub async fn revisar(estado: Arc<tokio::sync::Mutex<EstadoServidor>>) {
    loop {
        tokio::time::sleep(REVISION).await;
        let (cerrados, cambios) = {
            let estado_bloqueado = estado.lock().await;
            let mut cerrados: Vec<u32> = Vec::new();
            let mut cambios = false;
            let mut contadas: HashMap<u32, u32> = HashMap::new();
            for tunel in estado_bloqueado.tuneles_temporales.activos.values() {
                if tunel
                    .conexion
                    .as_ref()
                    .is_some_and(|conexion| conexion.handle.is_closed())
                {
                    cerrados.push(tunel.id);
                    continue;
                }
                let (conexiones, _, _, _) = tunel.contadores.instantanea();
                contadas.insert(tunel.id, conexiones);
                if estado_bloqueado
                    .tuneles_temporales
                    .contadas
                    .get(&tunel.id)
                    .is_none_or(|antes| *antes != conexiones)
                {
                    cambios = true;
                }
            }
            (cerrados, (cambios, contadas))
        };
        for id in cerrados {
            cerrar_con(&estado, id, "la conexión del host cayó").await;
        }
        let (cambios, contadas) = cambios;
        {
            let mut estado_bloqueado = estado.lock().await;
            if cambios {
                estado_bloqueado.tuneles_temporales.contadas = contadas;
                difundir(&estado_bloqueado);
            }
        }
    }
}

/// Datos de la pestaña y del host para abrir el túnel.
async fn datos(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    sesion_id: u32,
) -> Option<(
    i64,
    String,
    String,
    crate::modelo::Host,
    HashMap<i64, crate::modelo::Host>,
    crate::config::Rutas,
)> {
    let (host_id, host_nombre, sesion_nombre, lectura, rutas) = {
        let estado_bloqueado = estado.lock().await;
        let sesion = estado_bloqueado.sesiones.get(&sesion_id)?;
        if !sesion.abierta() {
            return None;
        }
        (
            sesion.host_id,
            sesion.host_nombre.clone(),
            sesion.nombre.clone(),
            estado_bloqueado.lectura.clone(),
            estado_bloqueado.rutas.clone(),
        )
    };
    let (host, todos_los_hosts) = {
        let lectura = lectura.lock().ok()?;
        let host = lectura.obtener_host(host_id).ok()?;
        let todos = lectura
            .listar_hosts()
            .unwrap_or_default()
            .into_iter()
            .map(|host| (host.id, host))
            .collect();
        (host, todos)
    };
    Some((
        host_id,
        host_nombre,
        sesion_nombre,
        host,
        todos_los_hosts,
        rutas,
    ))
}

async fn responder(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    mensaje: MensajeServidor,
) {
    let estado_bloqueado = estado.lock().await;
    if let Some(cliente) = estado_bloqueado.clientes.get(&cliente_id) {
        difusion::enviar(cliente, mensaje);
    }
}

/// Un fallo al abrir un túnel temporal: se anota en REGISTRO como
/// `tunel_fallido` con «temporal» y el motivo (el host, si ya se conoce) y se
/// contesta el error al solicitante.
async fn fallar(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    peticion_id: u64,
    host_id: Option<i64>,
    motivo: &str,
) {
    {
        let estado_bloqueado = estado.lock().await;
        let bd = estado_bloqueado.bd.clone();
        let _ = bd.send(super::OrdenBd::Anotar {
            tipo: crate::registro::TUNEL_FALLIDO.to_string(),
            host_id,
            identidad_id: None,
            detalle: format!("temporal · {motivo}"),
            resultado: ResultadoRegistro::Error,
        });
    }
    responder_error(estado, cliente_id, peticion_id, motivo).await;
}

async fn responder_error(
    estado: &Arc<tokio::sync::Mutex<EstadoServidor>>,
    cliente_id: u32,
    peticion_id: u64,
    motivo: &str,
) {
    warn!(cliente = cliente_id, "túnel temporal: {motivo}");
    responder(
        estado,
        cliente_id,
        MensajeServidor::Error {
            mensaje: motivo.to_string(),
            peticion_id: Some(peticion_id),
        },
    )
    .await;
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn solo_se_admiten_los_localhost_del_host() {
        assert!(DESTINOS.contains(&"localhost"));
        assert!(DESTINOS.contains(&"127.0.0.1"));
        assert!(DESTINOS.contains(&"::1"));
        assert!(!DESTINOS.contains(&"0.0.0.0"));
        assert!(!DESTINOS.contains(&"ejemplo.org"));
    }
}
