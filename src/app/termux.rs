//! Lo de Termux que vive en la App (Fase 17): las órdenes `termux-*` del
//! cliente (portapapeles y enlaces) y lo que el servidor dice por la barra.
//!
//! Ninguna orden se espera en el bucle de la interfaz: van en tareas tokio
//! (`Ordenes::ejecutar_async_avisando`, que usa `spawn_blocking` y lleva el
//! plazo de `[termux] api_plazo_seg`) y su resultado vuelve por
//! `Evento::Termux`. La terminal solo la escribe el hilo de la interfaz (el
//! OSC 52 de una copia), nunca una tarea.
//!
//! El servidor no puede hablar con la barra por el protocolo (congelado en la
//! versión 10): deja sus avisos en el fichero `servidor` del estado de Termux
//! y el cliente lo mira cada [`RITMO_ESTADO_SERVIDOR`], solo en Termux y solo
//! tras la `Bienvenida` del servidor (hasta entonces el fichero puede ser de
//! otro), y los dice una sola vez por servidor (por su `arranque`). Lo que dice
//! el fichero es entrada no fiable (otro proceso lo escribió): acotado y sin
//! pánicos. Cada lectura lleva su plazo ([`PLAZO_LECTURA_ESTADO`]) y no se
//! lanza otra mientras la anterior siga viva en su hilo.
//!
//! Los avisos que salen de la cola de la barra se ven al menos
//! [`VISIBILIDAD_AVISO`]: ni el borrado por teclas ni el vencimiento normal de
//! los mensajes los quitan antes (solo otro mensaje los pisa).

use std::collections::{BTreeSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::ThreadId;
use std::time::{Duration, Instant};

use zeroize::Zeroizing;

use crate::app::portapapeles::ParaQue;
use crate::app::{App, Evento};
use crate::config::Rutas;
use crate::termux::estado::{self, AvisoServidor, EstadoServidorTermux};
use crate::termux::{ApiProceso, EntornoTermux, FalloOrden, Ordenes};

/// Cada cuánto mira el cliente el fichero de estado del servidor (solo en
/// Termux, §5.2 y desviación 2).
pub const RITMO_ESTADO_SERVIDOR: Duration = Duration::from_secs(5);

/// Cuánto se espera al evento de una lectura antes de darlo por perdido y
/// permitir otra (red de seguridad: con [`PLAZO_LECTURA_ESTADO`] el evento
/// llega siempre; y no se lanza otra lectura mientras el hilo siga vivo).
const LECTURA_PERDIDA: Duration = Duration::from_secs(30);

/// Cuánto se espera a una lectura del estado del servidor antes de dar el
/// evento por vacío y liberar la bandera de lectura en curso. El hilo que lee
/// no se puede cancelar: mientras siga vivo no se lanza otra lectura.
pub const PLAZO_LECTURA_ESTADO: Duration = Duration::from_secs(2);

/// Lo mínimo que se ve en la barra un aviso que salió de su cola (el del
/// servidor o el de Termux:API apagada): el borrado por teclas y el
/// vencimiento de 4 s de los mensajes lo respetan. Se marcan como dichos al
/// leerlos y no se repiten mientras viva el servidor.
pub const VISIBILIDAD_AVISO: Duration = Duration::from_secs(6);

/// Órdenes `termux-clipboard-set` en vuelo a la vez: una. Las copias se
/// serializan para que el portapapeles quede con lo último que se pidió (una
/// orden lenta y vieja no puede escribir después de una nueva): la copia
/// siguiente espera su turno, y de las que esperan solo se hace la última.
/// Entre la que corre y la que espera, el texto en memoria es como mucho el de
/// dos copias (hasta 512 KiB cada una).
pub const COPIAS_A_LA_VEZ: usize = 1;

/// Mensajes que esperan su turno en la barra (los más viejos se descartan).
const COLA_BARRA_MAX: usize = 8;

/// El estado de Termux de una ventana de MAGI.
pub struct EstadoTermuxApp {
    /// ¿Se porta esta ventana como Termux? Sale de `es_termux()` al crearla;
    /// las pruebas lo fijan a mano para probar una rama sin depender del
    /// proceso. Todo lo propio de Termux de la `App` cuelga de aquí.
    pub activo: bool,
    /// Las órdenes `termux-*` del cliente, con su Termux:API apagable por
    /// proceso. Se pueden sustituir en las pruebas por unas con dobles.
    pub ordenes: Arc<Ordenes>,
    /// Qué servidor (su `arranque`) dijo ya cuáles de sus avisos en la barra:
    /// cada uno se dice una sola vez por servidor.
    pub avisos_dichos: Option<(i64, BTreeSet<AvisoServidor>)>,
    /// Última vez que se leyó el fichero de estado del servidor.
    pub ultima_lectura: Option<Instant>,
    /// Hay una lectura del estado del servidor en curso.
    pub leyendo_servidor: bool,
    /// Lo que espera su turno en la barra, que lleva un solo mensaje a la vez:
    /// avisos del servidor y de Termux:API, en orden. Nunca pisan un mensaje
    /// que se está viendo.
    pub cola_barra: VecDeque<(String, bool)>,
    /// Copias en vuelo por `termux-clipboard-set` (0 o [`COPIAS_A_LA_VEZ`]).
    pub copias_en_curso: usize,
    /// La última copia que espera a que termine la que está en vuelo; la nueva
    /// sustituye a la anterior, que se da por no hecha.
    pub copia_pendiente: Option<CopiaPendiente>,
    /// El mensaje de la barra que salió de la cola de avisos y hasta cuándo se
    /// protege.
    #[doc(hidden)]
    pub aviso_fijado: Option<AvisoFijado>,
    /// Cómo se lee el estado del servidor: `estado::leer_servidor`. Las pruebas
    /// ponen una lectura lenta o que no termina.
    #[doc(hidden)]
    pub lector_estado: Arc<LectorEstado>,
    /// Hay un hilo leyendo el estado del servidor (puede seguir vivo aunque su
    /// plazo ya venciera: no se lanza otra lectura hasta que acabe).
    #[doc(hidden)]
    pub lectura_viva: Arc<AtomicBool>,
    /// Los OSC 52 que la ventana habría escrito en la terminal, con el hilo
    /// que los escribió: solo cuando no hay efectos externos (las pruebas no
    /// tocan la terminal real).
    #[doc(hidden)]
    pub osc52_de_prueba: Vec<(ThreadId, Vec<u8>)>,
}

impl EstadoTermuxApp {
    /// Con el entorno del proceso y el plazo de `[termux] api_plazo_seg`.
    #[must_use]
    pub fn nuevo(plazo: Duration) -> Self {
        Self::con(
            crate::termux::es_termux(),
            Ordenes::nuevas(
                &EntornoTermux::del_proceso(),
                plazo,
                Arc::new(ApiProceso::default()),
            ),
        )
    }

    /// Con las órdenes y la plataforma que se den (las pruebas lo usan para
    /// poner unas órdenes con dobles sin tocar el entorno del proceso).
    #[must_use]
    pub fn con(activo: bool, ordenes: Ordenes) -> Self {
        Self {
            activo,
            ordenes: Arc::new(ordenes),
            avisos_dichos: None,
            ultima_lectura: None,
            leyendo_servidor: false,
            cola_barra: VecDeque::new(),
            copias_en_curso: 0,
            copia_pendiente: None,
            aviso_fijado: None,
            lector_estado: Arc::new(estado::leer_servidor),
            lectura_viva: Arc::new(AtomicBool::new(false)),
            osc52_de_prueba: Vec::new(),
        }
    }
}

/// Cómo se lee el estado del servidor (en un hilo bloqueante).
pub type LectorEstado = dyn Fn(&Rutas) -> Option<EstadoServidorTermux> + Send + Sync;

/// Una copia que espera su turno detrás de la que está en vuelo.
pub struct CopiaPendiente {
    pub texto: Zeroizing<String>,
    pub para: ParaQue,
}

/// Un aviso de la cola que se ve en la barra y no se quita hasta `hasta`.
#[derive(Debug, Clone)]
pub struct AvisoFijado {
    pub texto: String,
    pub error: bool,
    pub hasta: Instant,
}

/// Marca que hay un hilo leyendo el estado del servidor: se apaga al acabar el
/// hilo (o si entra en pánico), no cuando vence el plazo de espera.
struct LecturaViva(Arc<AtomicBool>);

impl LecturaViva {
    fn marcar(bandera: &Arc<AtomicBool>) -> Self {
        bandera.store(true, Ordering::Release);
        Self(Arc::clone(bandera))
    }
}

impl Drop for LecturaViva {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// El resultado de una copia por `termux-clipboard-set`, de vuelta en el
/// bucle de la interfaz. Lleva el texto para poder seguir con el OSC 52 si la
/// orden falló, y para qué se copiaba.
pub struct CopiaTerminada {
    pub texto: Zeroizing<String>,
    pub para: ParaQue,
    pub resultado: Result<(), FalloOrden>,
    /// Esta llamada acaba de apagar Termux:API (la primera vez del proceso):
    /// la barra lo dice una sola vez.
    pub recien_apagada: bool,
}

/// Lo que las tareas de Termux le cuentan al bucle de la interfaz.
pub enum EventoTermux {
    /// Respondió (o no) `termux-clipboard-set`.
    Copia(CopiaTerminada),
    /// Respondió (o no) la orden que abre un enlace de la sesión.
    Enlace(Result<(), FalloOrden>),
    /// Respondió (o no) la orden que abre el visor de un escritorio. Lleva los
    /// datos sobre los que actuaba: el enlace y su escritorio.
    Visor {
        escritorio_id: u32,
        enlace: String,
        resultado: Result<(), FalloOrden>,
    },
    /// Lo que el servidor dejó en `termux/servidor` (`None`: nada o ilegible).
    EstadoServidor(Option<EstadoServidorTermux>),
    /// La fila de teclas de Termux: la propuesta, la escritura de
    /// `termux.properties` y la recarga de Termux (`app/termux_teclas.rs`).
    Teclas(super::termux_teclas::EventoTeclas),
}

/// Los avisos de `estado` que este cliente aún no ha dicho, en el orden del
/// fichero y sin repetidos. Un servidor nuevo (otro `arranque`) empieza de
/// cero: vuelve a decir los suyos. Pura, para probarla.
#[must_use]
pub fn avisos_nuevos(
    dichos: &mut Option<(i64, BTreeSet<AvisoServidor>)>,
    estado: &EstadoServidorTermux,
) -> Vec<AvisoServidor> {
    let (arranque, ya) = dichos.get_or_insert_with(|| (estado.arranque, BTreeSet::new()));
    if *arranque != estado.arranque {
        *arranque = estado.arranque;
        ya.clear();
    }
    estado
        .avisos
        .iter()
        .copied()
        .filter(|aviso| ya.insert(*aviso))
        .collect()
}

impl App {
    /// Un mensaje para la barra, que lleva uno solo a la vez: si ya se está
    /// viendo uno, este espera su turno (nunca pisa un error del usuario).
    pub(crate) fn decir_en_barra(&mut self, texto: impl Into<String>, error: bool) {
        if self.termux.cola_barra.len() >= COLA_BARRA_MAX {
            self.termux.cola_barra.pop_front();
        }
        self.termux.cola_barra.push_back((texto.into(), error));
    }

    /// ¿Es el mensaje que se ve en la barra un aviso de la cola que aún está
    /// en su visibilidad mínima? Solo lo es el que `tick_termux` fijó y
    /// mientras siga siendo ese mismo mensaje: cualquier otro lo pisa.
    pub(crate) fn aviso_vigente_en_la_barra(&self) -> bool {
        match (&self.termux.aviso_fijado, &self.mensaje) {
            (Some(fijado), Some(mensaje)) => {
                Instant::now() < fijado.hasta
                    && mensaje.texto == fijado.texto
                    && mensaje.error == fijado.error
            }
            _ => false,
        }
    }

    /// Lo que hace cada tecla con el mensaje de la barra: quitarlo, salvo un
    /// aviso de la cola dentro de su visibilidad mínima.
    pub(crate) fn borrar_mensaje_por_tecla(&mut self) {
        if !self.aviso_vigente_en_la_barra() {
            self.mensaje = None;
        }
    }

    /// ¿Habla este cliente con un servidor que ya lo recibió (su `Bienvenida`)?
    /// Hasta entonces el fichero de estado puede ser de otro servidor.
    fn servidor_que_ha_dado_la_bienvenida(&self) -> bool {
        self.servidor_pid.is_some() && !self.servidor_caido
    }

    /// Cada tick, solo en Termux: da el turno al siguiente mensaje de la cola
    /// si la barra está libre (y lo protege [`VISIBILIDAD_AVISO`]) y, con la
    /// `Bienvenida` del servidor ya recibida, mira cada
    /// [`RITMO_ESTADO_SERVIDOR`] lo que él dejó en el estado (en una tarea,
    /// con plazo, nunca en el bucle).
    pub(crate) fn tick_termux(&mut self) {
        if !self.termux.activo {
            return;
        }
        if self.mensaje.is_none() {
            if let Some((texto, error)) = self.termux.cola_barra.pop_front() {
                self.termux.aviso_fijado = Some(AvisoFijado {
                    texto: texto.clone(),
                    error,
                    hasta: Instant::now() + VISIBILIDAD_AVISO,
                });
                self.mensaje(texto, error);
                self.sucio = true;
            }
        }
        if !self.servidor_que_ha_dado_la_bienvenida() {
            return;
        }
        let ahora = Instant::now();
        let toca = self
            .termux
            .ultima_lectura
            .is_none_or(|ultima| ahora.duration_since(ultima) >= RITMO_ESTADO_SERVIDOR);
        let perdida = self
            .termux
            .ultima_lectura
            .is_some_and(|ultima| ahora.duration_since(ultima) >= LECTURA_PERDIDA);
        if !toca || (self.termux.leyendo_servidor && !perdida) {
            return;
        }
        // Un hilo que no volvió (un sistema de ficheros colgado) no se puede
        // cancelar: hasta que acabe no se lanza otra lectura detrás.
        if self.termux.lectura_viva.load(Ordering::Acquire) {
            return;
        }
        self.termux.ultima_lectura = Some(ahora);
        self.termux.leyendo_servidor = true;
        let rutas = self.rutas.clone();
        let eventos = self.eventos_tx.clone();
        let lector = Arc::clone(&self.termux.lector_estado);
        let viva = LecturaViva::marcar(&self.termux.lectura_viva);
        self.runtime.spawn(async move {
            let lectura = tokio::task::spawn_blocking(move || {
                let _viva = viva;
                lector(&rutas)
            });
            // Pasado el plazo se da la lectura por vacía y se libera la
            // bandera del cliente; el hilo sigue marcado como vivo.
            let estado = match tokio::time::timeout(PLAZO_LECTURA_ESTADO, lectura).await {
                Ok(Ok(estado)) => estado,
                _ => None,
            };
            let _ = eventos.send(Evento::Termux(EventoTermux::EstadoServidor(estado)));
        });
    }

    /// Lo que vuelve de una tarea de Termux.
    pub(crate) fn evento_termux(&mut self, evento: EventoTermux) {
        match evento {
            EventoTermux::Copia(copia) => self.copia_terminada(copia),
            EventoTermux::Enlace(Ok(())) => {}
            EventoTermux::Enlace(Err(fallo)) => {
                self.mensaje(format!("no se pudo abrir el enlace: {fallo}"), true);
            }
            EventoTermux::Visor {
                escritorio_id,
                enlace,
                resultado,
            } => self.visor_abierto_en_termux(escritorio_id, enlace, resultado),
            EventoTermux::EstadoServidor(estado) => {
                self.termux.leyendo_servidor = false;
                if let Some(estado) = estado {
                    for aviso in avisos_nuevos(&mut self.termux.avisos_dichos, &estado) {
                        self.decir_en_barra(aviso.texto(), true);
                    }
                }
            }
            EventoTermux::Teclas(evento) => self.evento_teclas(evento),
        }
    }

    /// Abre un enlace con `programa` (`[terminal] abrir_enlaces`) por las
    /// órdenes de Termux: la URL como único argumento, sin shell, con plazo y
    /// en una tarea. Si falla, el resultado vuelve por `Evento::Termux`.
    pub(crate) fn abrir_enlace_en_termux(&mut self, programa: String, url: &str) {
        let ordenes = Arc::clone(&self.termux.ordenes);
        let eventos = self.eventos_tx.clone();
        let argumentos = vec![programa, url.to_string()];
        self.runtime.spawn(async move {
            let (resultado, _) = ordenes.ejecutar_async_avisando(argumentos, None).await;
            let _ = eventos.send(Evento::Termux(EventoTermux::Enlace(resultado.map(|_| ()))));
        });
        self.mensaje(format!("abriendo {url}"), false);
    }

    /// Abre el visor de un escritorio con la orden que resolvió
    /// `[escritorio] navegador = auto` (`termux-open-url <enlace>`), por las
    /// órdenes de Termux y en una tarea.
    pub(crate) fn abrir_visor_en_termux(
        &mut self,
        escritorio_id: u32,
        enlace: &str,
        argumentos: Vec<String>,
    ) {
        let ordenes = Arc::clone(&self.termux.ordenes);
        let eventos = self.eventos_tx.clone();
        let enlace_propio = enlace.to_string();
        self.runtime.spawn(async move {
            let (resultado, _) = ordenes.ejecutar_async_avisando(argumentos, None).await;
            let _ = eventos.send(Evento::Termux(EventoTermux::Visor {
                escritorio_id,
                enlace: enlace_propio,
                resultado: resultado.map(|_| ()),
            }));
        });
        self.mensaje("abriendo el visor…", false);
        self.sucio = true;
    }

    /// La orden del visor volvió: abierto, o el enlace a la vista para copiarlo
    /// (lo mismo que hace el escritorio con su navegador).
    fn visor_abierto_en_termux(
        &mut self,
        escritorio_id: u32,
        enlace: String,
        resultado: Result<(), FalloOrden>,
    ) {
        match resultado {
            Ok(()) => {
                self.enlace_visible = None;
                self.mensaje("visor abierto en el navegador", false);
            }
            Err(fallo) => {
                self.enlace_visible = Some((escritorio_id, enlace, Instant::now()));
                self.mensaje(
                    format!("no se pudo abrir el navegador: {fallo}; copia el enlace con y"),
                    true,
                );
            }
        }
        self.sucio = true;
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn estado_de(arranque: i64, avisos: &[AvisoServidor]) -> EstadoServidorTermux {
        EstadoServidorTermux {
            arranque,
            avisos: avisos.to_vec(),
        }
    }

    #[test]
    fn cada_aviso_se_dice_una_vez_por_servidor() {
        let mut dichos = None;
        let primero = estado_de(
            100,
            &[AvisoServidor::ApiApagada, AvisoServidor::SinWakeLock],
        );
        assert_eq!(
            avisos_nuevos(&mut dichos, &primero),
            vec![AvisoServidor::ApiApagada, AvisoServidor::SinWakeLock]
        );
        assert!(
            avisos_nuevos(&mut dichos, &primero).is_empty(),
            "el mismo servidor no repite"
        );
        // El mismo servidor suma un aviso nuevo: solo ese.
        let mas = estado_de(
            100,
            &[
                AvisoServidor::ApiApagada,
                AvisoServidor::SinWakeLock,
                AvisoServidor::LimiteProcesos,
            ],
        );
        assert_eq!(
            avisos_nuevos(&mut dichos, &mas),
            vec![AvisoServidor::LimiteProcesos]
        );
        // Otro arranque es otro servidor: vuelve a decirlos todos.
        let otro = estado_de(200, &[AvisoServidor::ApiApagada]);
        assert_eq!(
            avisos_nuevos(&mut dichos, &otro),
            vec![AvisoServidor::ApiApagada]
        );
        assert!(avisos_nuevos(&mut dichos, &otro).is_empty());
        assert_eq!(dichos.as_ref().map(|(arranque, _)| *arranque), Some(200));
    }

    #[test]
    fn un_fichero_con_repetidos_o_vacio_no_da_avisos_de_mas() {
        let mut dichos = None;
        let repetido = estado_de(
            1,
            &[
                AvisoServidor::SinWakeLock,
                AvisoServidor::SinWakeLock,
                AvisoServidor::SinWakeLock,
            ],
        );
        assert_eq!(
            avisos_nuevos(&mut dichos, &repetido),
            vec![AvisoServidor::SinWakeLock]
        );
        assert!(avisos_nuevos(&mut dichos, &estado_de(1, &[])).is_empty());
        // Un arranque absurdo (negativo, extremo) es solo otro servidor.
        for arranque in [i64::MIN, -1, 0, i64::MAX] {
            let estado = estado_de(arranque, &[AvisoServidor::LimiteProcesos]);
            assert_eq!(avisos_nuevos(&mut dichos, &estado).len(), 1);
        }
    }

    #[test]
    fn el_estado_nuevo_empieza_sin_nada_dicho_ni_leido() {
        let temporal = tempfile::tempdir().unwrap();
        let estado = EstadoTermuxApp::con(
            true,
            Ordenes::nuevas(
                &EntornoTermux {
                    home: Some(temporal.path().to_path_buf()),
                    ..EntornoTermux::default()
                },
                Duration::from_secs(5),
                Arc::new(ApiProceso::default()),
            ),
        );
        assert!(estado.activo);
        assert!(estado.avisos_dichos.is_none());
        assert!(estado.ultima_lectura.is_none());
        assert!(!estado.leyendo_servidor);
        assert!(estado.cola_barra.is_empty());
        assert_eq!(estado.copias_en_curso, 0);
    }
}
