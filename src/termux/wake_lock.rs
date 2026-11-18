//! Wake lock del servidor (§4.6, §7.5): el servidor toma el wake lock de
//! Termux mientras tiene trabajo y 30 s después, para que Android no duerma
//! MAGI con el móvil en el bolsillo.
//!
//! Dos piezas:
//!
//! - [`MaquinaWakeLock`], pura: recibe si hay trabajo y la hora (un
//!   `Duration` desde un origen cualquiera, sintético en las pruebas) y dice
//!   qué orden toca ejecutar. No sabe de tokio, de órdenes ni de ficheros.
//! - [`WakeLock`], el conductor: la alimenta desde `revisar_inactividad` (cada
//!   segundo), ejecuta `termux-wake-lock` y `termux-wake-unlock` con el plazo
//!   de `Ordenes`, una a la vez y sin que `revisar_inactividad` espere, y deja
//!   la marca `wake-lock` del estado y los avisos de la barra.
//!
//! Solo se suelta el wake lock que tomó MAGI: el que lleva su marca. Si el
//! servidor muere sin soltarlo, la marca queda y el siguiente lo suelta en
//! cuanto no tenga trabajo. `[termux] wake_lock = "nunca"` no toma nada; con
//! una marca de un servidor anterior, suelta solo esa (es de MAGI).
//!
//! Una marca previa solo dice que el wake lock **puede** estar tomado: si
//! Android cerró Termux (el caso para el que existe la marca), el wake lock
//! real ya no existe. Por eso, con trabajo, el servidor nuevo vuelve a tomarlo
//! (`termux-wake-lock` es idempotente) en vez de fiarse de la marca; sin
//! trabajo, lo suelta como si fuera suyo.
//!
//! Lo mismo vale para un `termux-wake-lock` que no responde a tiempo: la orden
//! pudo llegar a Termux antes de que se matara su grupo de procesos, así que
//! el wake lock **puede** estar tomado. El servidor se apaga (no lo reintenta)
//! y lo dice en la barra, pero conserva la marca y lo suelta como suyo: sin
//! trabajo, al terminar o, si muere, en el siguiente servidor.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use tracing::{info, warn};

use crate::config::Rutas;

use super::estado::{self, AvisoServidor, Marca};
use super::ordenes::{FalloOrden, Ordenes};

/// Cuánto sigue tomado el wake lock tras quedar el servidor sin trabajo
/// (§4.6): si vuelve el trabajo antes, sigue tomado.
pub const GRACIA_SOLTAR: Duration = Duration::from_secs(30);

/// Desde Android 12 (SDK 31) el sistema puede cerrar procesos de Termux en
/// segundo plano aunque tengan el wake lock (R166).
pub const SDK_LIMITE_PROCESOS: u32 = 31;

/// Intentos de soltar que se hacen seguidos antes de rendirse y dejar la
/// marca para que lo intente el siguiente servidor.
const INTENTOS_SOLTAR: u32 = 3;

/// Un instante, como el tiempo desde un origen cualquiera.
pub type Momento = Duration;

/// Qué orden de Termux toca ejecutar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrdenWakeLock {
    /// `termux-wake-lock`.
    Tomar,
    /// `termux-wake-unlock`.
    Soltar,
}

impl OrdenWakeLock {
    fn programa(self) -> &'static str {
        match self {
            Self::Tomar => "termux-wake-lock",
            Self::Soltar => "termux-wake-unlock",
        }
    }
}

/// La fase en la que está el wake lock (§3.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaseWakeLock {
    /// No lo tiene MAGI.
    Suelto,
    /// `termux-wake-lock` en marcha.
    Tomando,
    /// Lo tiene MAGI y hay trabajo.
    Tomado,
    /// Lo tiene MAGI y el servidor está sin trabajo: se suelta al pasar la
    /// gracia.
    Esperando,
    /// `termux-wake-unlock` en marcha.
    Soltando,
    /// No se pudo tomar: apagado hasta que el servidor reinicie.
    Apagado,
}

/// Cómo terminó una orden de soltar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultadoSoltar {
    /// Soltado: se quita la marca.
    Suelto,
    /// Falló: se reintenta pasada la gracia.
    Reintentara,
    /// Falló varias veces: se deja la marca para el siguiente servidor.
    Abandonado,
}

/// La máquina de estados del wake lock. Pura: ni tokio, ni órdenes, ni
/// ficheros.
#[derive(Debug, Clone)]
pub struct MaquinaWakeLock {
    /// `[termux] wake_lock = "auto"`.
    auto: bool,
    gracia: Duration,
    /// No se pudo tomar: no se vuelve a intentar en este servidor.
    apagado: bool,
    /// MAGI lo tiene tomado (su marca está puesta), o puede tenerlo: con
    /// `incierto` la marca es de un servidor anterior y no se sabe si el
    /// wake lock sigue existiendo.
    tomado: bool,
    /// La marca es de un servidor anterior: el wake lock puede haber dejado
    /// de existir (Android cerró Termux). Con trabajo se vuelve a tomar; sin
    /// él, se suelta. Se apaga al tomarlo o al soltarlo.
    incierto: bool,
    /// La orden que se está ejecutando, si hay una.
    en_vuelo: Option<OrdenWakeLock>,
    /// A partir de cuándo se suelta, si no vuelve el trabajo.
    soltar_en: Option<Momento>,
    fallos_soltar: u32,
    /// El servidor termina: no se toman órdenes nuevas.
    terminando: bool,
}

impl MaquinaWakeLock {
    /// Una máquina nueva. Con `marca_previa` (el servidor anterior murió sin
    /// soltar el wake lock) puede que siga tomado: es de MAGI y se suelta en
    /// cuanto no haya trabajo, y con trabajo se vuelve a tomar antes de darlo
    /// por bueno, porque tras un cierre de Android el real ya no existe (§7.5,
    /// con esa corrección).
    #[must_use]
    pub fn nueva(auto: bool, gracia: Duration, marca_previa: bool, ahora: Momento) -> Self {
        Self {
            auto,
            gracia,
            apagado: false,
            tomado: marca_previa,
            incierto: marca_previa,
            en_vuelo: None,
            soltar_en: marca_previa.then_some(ahora),
            fallos_soltar: 0,
            terminando: false,
        }
    }

    /// La fase actual.
    #[must_use]
    pub fn fase(&self) -> FaseWakeLock {
        match self.en_vuelo {
            Some(OrdenWakeLock::Tomar) => FaseWakeLock::Tomando,
            Some(OrdenWakeLock::Soltar) => FaseWakeLock::Soltando,
            None if self.tomado && self.soltar_en.is_some() => FaseWakeLock::Esperando,
            None if self.tomado => FaseWakeLock::Tomado,
            None if self.apagado => FaseWakeLock::Apagado,
            None => FaseWakeLock::Suelto,
        }
    }

    /// ¿MAGI lo tiene tomado?
    #[must_use]
    pub fn tomado(&self) -> bool {
        self.tomado
    }

    /// ¿Se apagó por no poder tomarlo?
    #[must_use]
    pub fn apagado(&self) -> bool {
        self.apagado
    }

    /// ¿El servidor ya está terminando?
    #[must_use]
    pub fn terminando(&self) -> bool {
        self.terminando
    }

    /// Un paso: dice si hay trabajo y la hora, y devuelve la orden que toca
    /// ejecutar, si toca alguna. Se llama cada segundo. Mientras hay una orden
    /// en marcha no sale otra: se espera a [`Self::tomar_hecho`] o
    /// [`Self::soltar_hecho`], y el siguiente paso vuelve a mirarlo todo.
    pub fn paso(&mut self, hay_trabajo: bool, ahora: Momento) -> Option<OrdenWakeLock> {
        if self.terminando || self.en_vuelo.is_some() {
            return None;
        }
        if hay_trabajo {
            // Con trabajo no se suelta nada: vuelve a empezar la cuenta.
            self.soltar_en = None;
            // Una marca previa no prueba que el wake lock exista: se toma
            // igualmente (idempotente en Termux) antes de darlo por tomado.
            if (!self.tomado || self.incierto) && self.auto && !self.apagado {
                self.en_vuelo = Some(OrdenWakeLock::Tomar);
                return Some(OrdenWakeLock::Tomar);
            }
            return None;
        }
        if self.tomado {
            let hasta = *self.soltar_en.get_or_insert(ahora + self.gracia);
            if ahora >= hasta {
                self.en_vuelo = Some(OrdenWakeLock::Soltar);
                return Some(OrdenWakeLock::Soltar);
            }
        }
        None
    }

    /// `termux-wake-lock` terminó. Con éxito pasa a tomado (y ya no es
    /// incierto); si falló, queda apagado en este servidor (no se reintenta
    /// en bucle). Un fallo no cambia `tomado`: con la marca de un servidor
    /// anterior sigue siendo «puede que lo tenga MAGI» y se soltará sin
    /// trabajo; sin ella, no hay nada que soltar.
    pub fn tomar_hecho(&mut self, bien: bool) {
        self.en_vuelo = None;
        self.incierto = false;
        if bien {
            self.tomado = true;
            self.soltar_en = None;
            self.fallos_soltar = 0;
        } else {
            self.apagado = true;
        }
    }

    /// `termux-wake-lock` no respondió a tiempo (corrección de cierre A3).
    /// Queda apagado en este servidor como con cualquier otro fallo, pero la
    /// orden pudo llegar a Termux antes de que se matara su grupo de procesos
    /// y el wake lock pudo tomarse: se trata como incierto, igual que una
    /// marca de un servidor anterior. La marca se conserva y, sin trabajo, se
    /// suelta pasada la gracia, al terminar el servidor o, si éste muere, en
    /// el siguiente. Con trabajo no se vuelve a tomar (está apagado).
    pub fn tomar_sin_respuesta(&mut self) {
        self.en_vuelo = None;
        self.apagado = true;
        self.tomado = true;
        self.incierto = true;
    }

    /// `termux-wake-unlock` terminó. Con éxito queda suelto; si falló, se
    /// reintenta pasada la gracia y, tras varios fallos seguidos, se rinde:
    /// la marca queda y lo intentará el siguiente servidor.
    pub fn soltar_hecho(&mut self, bien: bool, ahora: Momento) -> ResultadoSoltar {
        self.en_vuelo = None;
        if bien {
            self.tomado = false;
            self.incierto = false;
            self.soltar_en = None;
            self.fallos_soltar = 0;
            return ResultadoSoltar::Suelto;
        }
        self.fallos_soltar += 1;
        if self.fallos_soltar >= INTENTOS_SOLTAR {
            self.tomado = false;
            self.incierto = false;
            self.soltar_en = None;
            self.fallos_soltar = 0;
            ResultadoSoltar::Abandonado
        } else {
            self.soltar_en = Some(ahora + self.gracia);
            ResultadoSoltar::Reintentara
        }
    }

    /// El servidor termina: no sale ninguna orden más y, si MAGI lo tiene
    /// tomado, devuelve la de soltarlo.
    pub fn terminar(&mut self) -> Option<OrdenWakeLock> {
        self.terminando = true;
        self.en_vuelo = None;
        if self.tomado {
            self.en_vuelo = Some(OrdenWakeLock::Soltar);
            Some(OrdenWakeLock::Soltar)
        } else {
            None
        }
    }
}

/// ¿Toca dar el aviso único del límite de procesos de Android 12 o posterior?
/// Con el SDK conocido y mayor o igual que 31, y sin que ya se haya dado
/// (marca `segundo-plano`): una sola vez en la vida del equipo.
#[must_use]
pub fn toca_aviso_limite_procesos(sdk: Option<u32>, ya_avisado: bool) -> bool {
    !ya_avisado && sdk.is_some_and(|sdk| sdk >= SDK_LIMITE_PROCESOS)
}

/// Cómo dice el conductor un aviso de la barra (lo escribe el servidor en el
/// fichero de estado que lee el cliente).
pub type DiceAvisoBarra = Arc<dyn Fn(AvisoServidor) + Send + Sync>;

/// El conductor tokio del wake lock.
pub struct WakeLock {
    maquina: Mutex<MaquinaWakeLock>,
    /// Una sola orden a la vez; el cierre del servidor espera a la que haya.
    turno: tokio::sync::Mutex<()>,
    ordenes: Arc<Ordenes>,
    rutas: Rutas,
    origen: Instant,
    sdk: Option<u32>,
    decir: DiceAvisoBarra,
}

impl WakeLock {
    /// Un conductor. Mira la marca `wake-lock` del estado (un servidor
    /// anterior murió sin soltarlo): sin trabajo lo suelta; con trabajo lo
    /// vuelve a tomar, porque la marca no prueba que siga existiendo.
    #[must_use]
    pub fn nuevo(
        ordenes: Arc<Ordenes>,
        rutas: Rutas,
        auto: bool,
        gracia: Duration,
        sdk: Option<u32>,
        decir: DiceAvisoBarra,
    ) -> Arc<Self> {
        let marca_previa = estado::hay_marca(&rutas, Marca::WakeLock);
        if marca_previa {
            info!(
                "el servidor anterior dejó la marca del wake lock: se soltará sin trabajo \
                 y se volverá a tomar con trabajo"
            );
        }
        Arc::new(Self {
            maquina: Mutex::new(MaquinaWakeLock::nueva(
                auto,
                gracia,
                marca_previa,
                Duration::ZERO,
            )),
            turno: tokio::sync::Mutex::new(()),
            ordenes,
            rutas,
            origen: Instant::now(),
            sdk,
            decir,
        })
    }

    fn maquina(&self) -> MutexGuard<'_, MaquinaWakeLock> {
        // El estado es de valores sueltos: uno envenenado no está a medias.
        self.maquina
            .lock()
            .unwrap_or_else(|envenenado| envenenado.into_inner())
    }

    fn ahora(&self) -> Momento {
        self.origen.elapsed()
    }

    /// La fase actual (para las pruebas y el diagnóstico).
    #[must_use]
    pub fn fase(&self) -> FaseWakeLock {
        self.maquina().fase()
    }

    /// Dice si el servidor tiene trabajo. No espera a ninguna orden: si toca
    /// tomar o soltar, la ejecuta en otra tarea. Se llama cada segundo desde
    /// `revisar_inactividad`, que no cambia por esto cuándo se apaga el
    /// servidor.
    pub fn alimentar(self: &Arc<Self>, hay_trabajo: bool) {
        let ahora = self.ahora();
        let orden = self.maquina().paso(hay_trabajo, ahora);
        if let Some(orden) = orden {
            let conductor = Arc::clone(self);
            tokio::spawn(async move {
                let _turno = conductor.turno.lock().await;
                // El servidor pudo empezar a terminar mientras esperaba su
                // turno: lo que quede en marcha lo cierra `terminar`.
                if conductor.maquina().terminando() {
                    return;
                }
                match orden {
                    OrdenWakeLock::Tomar => conductor.tomar().await,
                    OrdenWakeLock::Soltar => conductor.soltar().await,
                }
            });
        }
    }

    /// Termina: espera a la orden en marcha y, si MAGI tiene tomado el wake
    /// lock, lo suelta. Quien llama le pone el plazo (el de la orden y 3 s
    /// más, §7.5).
    pub async fn terminar(&self) {
        let _turno = self.turno.lock().await;
        let orden = self.maquina().terminar();
        if orden == Some(OrdenWakeLock::Soltar) {
            self.soltar().await;
        }
    }

    async fn ejecutar(&self, orden: OrdenWakeLock) -> Result<(), FalloOrden> {
        self.ordenes
            .ejecutar_async_avisando(vec![orden.programa().to_string()], None)
            .await
            .0
            .map(|_| ())
    }

    /// Una operación de la marca, fuera de los hilos del runtime.
    async fn con_marca<F>(&self, accion: F)
    where
        F: FnOnce(&Rutas) -> anyhow::Result<()> + Send + 'static,
    {
        let rutas = self.rutas.clone();
        match tokio::task::spawn_blocking(move || accion(&rutas)).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => warn!("no se pudo actualizar el estado de Termux: {error:#}"),
            Err(error) => warn!("no se pudo actualizar el estado de Termux: {error}"),
        }
    }

    /// `termux-wake-lock`. Primero la marca y luego la orden: si el servidor
    /// muere entre las dos, el siguiente suelta un wake lock que no existe
    /// (inocuo), no uno que se quedó sin dueño. Con la marca de un servidor
    /// anterior también se ejecuta: no prueba que el wake lock exista.
    async fn tomar(&self) {
        self.con_marca(|rutas| estado::poner_marca(rutas, Marca::WakeLock))
            .await;
        match self.ejecutar(OrdenWakeLock::Tomar).await {
            Ok(()) => {
                self.maquina().tomar_hecho(true);
                info!("wake lock tomado");
                self.avisar_limite_de_procesos().await;
            }
            Err(fallo) => {
                let sigue_siendo_de_magi = {
                    let mut maquina = self.maquina();
                    // Una orden que no respondió pudo haber llegado a Termux
                    // antes de que se matara su grupo: el wake lock pudo
                    // tomarse y la marca se conserva (A3). Los demás fallos
                    // (código distinto de 0, sin instalar…) no lo tomaron.
                    if matches!(fallo, FalloOrden::SinRespuesta) {
                        maquina.tomar_sin_respuesta();
                    } else {
                        maquina.tomar_hecho(false);
                    }
                    maquina.tomado()
                };
                // Con la marca de un servidor anterior, o con una orden sin
                // respuesta, el wake lock puede existir: la marca se queda
                // para que se suelte sin trabajo (o lo haga el siguiente
                // servidor).
                if !sigue_siendo_de_magi {
                    self.con_marca(|rutas| estado::quitar_marca(rutas, Marca::WakeLock))
                        .await;
                }
                // La salida de la orden es entrada no fiable: no va al log.
                warn!(
                    "no se pudo tomar el wake lock ({}): Android puede dormir MAGI",
                    describir(&fallo)
                );
                (self.decir)(AvisoServidor::SinWakeLock);
            }
        }
    }

    /// `termux-wake-unlock`.
    async fn soltar(&self) {
        let resultado = self.ejecutar(OrdenWakeLock::Soltar).await;
        let ahora = self.ahora();
        let bien = resultado.is_ok();
        // El cerrojo de la máquina no puede vivir durante los `.await` de las
        // ramas: se suelta antes de mirar el resultado.
        let desenlace = self.maquina().soltar_hecho(bien, ahora);
        match desenlace {
            ResultadoSoltar::Suelto => {
                self.con_marca(|rutas| estado::quitar_marca(rutas, Marca::WakeLock))
                    .await;
                info!("wake lock soltado");
            }
            ResultadoSoltar::Reintentara | ResultadoSoltar::Abandonado => {
                if let Err(fallo) = resultado {
                    warn!("no se pudo soltar el wake lock ({})", describir(&fallo));
                }
            }
        }
    }

    /// El aviso único de Android 12 o posterior (§4.6), tras el primer
    /// wake lock tomado.
    async fn avisar_limite_de_procesos(&self) {
        let ya = estado::hay_marca(&self.rutas, Marca::SegundoPlano);
        if !toca_aviso_limite_procesos(self.sdk, ya) {
            return;
        }
        self.con_marca(|rutas| estado::poner_marca(rutas, Marca::SegundoPlano))
            .await;
        (self.decir)(AvisoServidor::LimiteProcesos);
    }
}

/// Qué falló, sin la salida del programa (entrada no fiable: no va al log).
fn describir(fallo: &FalloOrden) -> String {
    match fallo {
        FalloOrden::ApiApagada => "Termux:API apagada".to_string(),
        FalloOrden::NoInstalada(programa) => format!("{programa} no está instalado"),
        FalloOrden::SinRespuesta => "no respondió a tiempo".to_string(),
        FalloOrden::Codigo { codigo, .. } => match codigo {
            Some(codigo) => format!("terminó con el código {codigo}"),
            None => "terminó sin código".to_string(),
        },
        FalloOrden::Io(_) => "no se pudo lanzar".to_string(),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    const GRACIA: Duration = Duration::from_secs(30);

    fn seg(n: u64) -> Momento {
        Duration::from_secs(n)
    }

    fn maquina() -> MaquinaWakeLock {
        MaquinaWakeLock::nueva(true, GRACIA, false, seg(0))
    }

    #[test]
    fn con_trabajo_se_toma_una_vez_y_se_mantiene() {
        let mut m = maquina();
        assert_eq!(m.fase(), FaseWakeLock::Suelto);
        assert_eq!(m.paso(false, seg(1)), None, "sin trabajo no se toma");
        assert_eq!(m.paso(true, seg(2)), Some(OrdenWakeLock::Tomar));
        assert_eq!(m.fase(), FaseWakeLock::Tomando);
        assert_eq!(m.paso(true, seg(3)), None, "una orden a la vez");
        m.tomar_hecho(true);
        assert_eq!(m.fase(), FaseWakeLock::Tomado);
        assert!(m.tomado());
        assert_eq!(m.paso(true, seg(4)), None, "tomado no se vuelve a tomar");
        assert_eq!(m.paso(true, seg(400)), None);
    }

    #[test]
    fn se_suelta_treinta_segundos_despues_de_quedar_sin_trabajo() {
        let mut m = maquina();
        m.paso(true, seg(0));
        m.tomar_hecho(true);
        assert_eq!(m.paso(false, seg(10)), None, "empieza la gracia");
        assert_eq!(m.fase(), FaseWakeLock::Esperando);
        assert_eq!(m.paso(false, seg(39)), None, "falta 1 s: 30 s desde el 10");
        assert_eq!(m.paso(false, seg(40)), Some(OrdenWakeLock::Soltar));
        assert_eq!(m.fase(), FaseWakeLock::Soltando);
        assert_eq!(m.soltar_hecho(true, seg(41)), ResultadoSoltar::Suelto);
        assert_eq!(m.fase(), FaseWakeLock::Suelto);
        assert!(!m.tomado());
        assert_eq!(m.paso(false, seg(100)), None, "suelto no se suelta más");
    }

    #[test]
    fn si_vuelve_el_trabajo_antes_de_treinta_segundos_sigue_tomado() {
        let mut m = maquina();
        m.paso(true, seg(0));
        m.tomar_hecho(true);
        assert_eq!(m.paso(false, seg(10)), None);
        assert_eq!(m.paso(true, seg(35)), None, "vuelve el trabajo");
        assert_eq!(m.fase(), FaseWakeLock::Tomado);
        // La cuenta empieza de nuevo al volver a quedar sin trabajo.
        assert_eq!(m.paso(false, seg(36)), None);
        assert_eq!(
            m.paso(false, seg(65)),
            None,
            "30 s desde el 36, no desde el 10"
        );
        assert_eq!(m.paso(false, seg(66)), Some(OrdenWakeLock::Soltar));
    }

    #[test]
    fn con_wake_lock_nunca_no_se_toma_nada() {
        let mut m = MaquinaWakeLock::nueva(false, GRACIA, false, seg(0));
        assert_eq!(m.paso(true, seg(1)), None);
        assert_eq!(m.paso(false, seg(2)), None);
        assert_eq!(m.paso(true, seg(100)), None);
        assert_eq!(m.fase(), FaseWakeLock::Suelto);
        assert_eq!(m.terminar(), None, "no tiene nada que soltar");
    }

    #[test]
    fn si_no_se_puede_tomar_se_apaga_y_no_se_reintenta() {
        let mut m = maquina();
        assert_eq!(m.paso(true, seg(0)), Some(OrdenWakeLock::Tomar));
        m.tomar_hecho(false);
        assert_eq!(m.fase(), FaseWakeLock::Apagado);
        assert!(m.apagado());
        assert!(!m.tomado());
        for segundo in 1..100 {
            assert_eq!(m.paso(true, seg(segundo)), None, "ni en bucle ni después");
        }
        assert_eq!(m.terminar(), None, "no soltó lo que no tomó");
    }

    #[test]
    fn la_marca_de_un_servidor_anterior_se_suelta_en_cuanto_no_hay_trabajo() {
        let mut m = MaquinaWakeLock::nueva(true, GRACIA, true, seg(5));
        assert!(m.tomado());
        assert_eq!(m.paso(false, seg(6)), Some(OrdenWakeLock::Soltar));
        assert_eq!(m.soltar_hecho(true, seg(7)), ResultadoSoltar::Suelto);
        assert_eq!(m.fase(), FaseWakeLock::Suelto);
    }

    #[test]
    fn con_la_marca_previa_y_trabajo_se_vuelve_a_tomar_una_sola_vez() {
        // La marca de un servidor anterior no prueba que el wake lock exista
        // (Android pudo cerrar Termux): con trabajo se toma igualmente.
        let mut m = MaquinaWakeLock::nueva(true, GRACIA, true, seg(0));
        assert_eq!(m.paso(true, seg(1)), Some(OrdenWakeLock::Tomar));
        assert_eq!(m.fase(), FaseWakeLock::Tomando);
        assert_eq!(m.paso(true, seg(2)), None, "una orden a la vez");
        m.tomar_hecho(true);
        assert_eq!(m.fase(), FaseWakeLock::Tomado);
        assert_eq!(m.paso(true, seg(3)), None, "ya tomado: no se repite");
        assert_eq!(m.paso(true, seg(400)), None);
        // Y se suelta con su gracia como cualquier otro.
        assert_eq!(m.paso(false, seg(410)), None);
        assert_eq!(m.paso(false, seg(440)), Some(OrdenWakeLock::Soltar));
        assert_eq!(m.soltar_hecho(true, seg(441)), ResultadoSoltar::Suelto);
        assert_eq!(m.paso(true, seg(450)), Some(OrdenWakeLock::Tomar));
    }

    #[test]
    fn con_la_marca_previa_sin_trabajo_se_suelta_sin_tomar_nada() {
        let mut m = MaquinaWakeLock::nueva(true, GRACIA, true, seg(0));
        assert_eq!(m.paso(false, seg(1)), Some(OrdenWakeLock::Soltar));
        assert_eq!(m.soltar_hecho(true, seg(2)), ResultadoSoltar::Suelto);
        assert_eq!(m.paso(false, seg(100)), None, "ni se toma ni se suelta más");
    }

    #[test]
    fn si_falla_tomar_con_la_marca_previa_sigue_siendo_de_magi_y_se_suelta_sin_trabajo() {
        let mut m = MaquinaWakeLock::nueva(true, GRACIA, true, seg(0));
        assert_eq!(m.paso(true, seg(1)), Some(OrdenWakeLock::Tomar));
        m.tomar_hecho(false);
        assert!(m.apagado(), "no se reintenta en este servidor");
        assert!(m.tomado(), "la marca de antes sigue siendo de MAGI");
        assert_eq!(m.paso(true, seg(2)), None, "ni en bucle");
        assert_eq!(m.paso(false, seg(3)), None);
        assert_eq!(m.paso(false, seg(33)), Some(OrdenWakeLock::Soltar));
    }

    #[test]
    fn con_la_marca_previa_y_el_modo_nunca_con_trabajo_no_se_toma_nada() {
        let mut m = MaquinaWakeLock::nueva(false, GRACIA, true, seg(0));
        assert_eq!(m.paso(true, seg(1)), None);
        assert_eq!(m.paso(true, seg(100)), None);
    }

    #[test]
    fn la_marca_previa_se_suelta_aunque_el_modo_sea_nunca() {
        // Es el wake lock que tomó MAGI: se suelta, pero nunca se toma otro.
        let mut m = MaquinaWakeLock::nueva(false, GRACIA, true, seg(0));
        assert_eq!(m.paso(false, seg(1)), Some(OrdenWakeLock::Soltar));
        m.soltar_hecho(true, seg(2));
        assert_eq!(m.paso(true, seg(3)), None);
    }

    #[test]
    fn un_soltar_que_falla_se_reintenta_y_luego_se_rinde_dejando_la_marca() {
        let mut m = maquina();
        m.paso(true, seg(0));
        m.tomar_hecho(true);
        assert_eq!(m.paso(false, seg(1)), None);
        assert_eq!(m.paso(false, seg(31)), Some(OrdenWakeLock::Soltar));
        assert_eq!(m.soltar_hecho(false, seg(32)), ResultadoSoltar::Reintentara);
        assert!(m.tomado(), "sigue siendo de MAGI");
        assert_eq!(m.paso(false, seg(40)), None, "espera otra gracia");
        assert_eq!(m.paso(false, seg(62)), Some(OrdenWakeLock::Soltar));
        assert_eq!(m.soltar_hecho(false, seg(63)), ResultadoSoltar::Reintentara);
        assert_eq!(m.paso(false, seg(93)), Some(OrdenWakeLock::Soltar));
        assert_eq!(m.soltar_hecho(false, seg(94)), ResultadoSoltar::Abandonado);
        assert!(!m.tomado(), "no se queda reintentando para siempre");
    }

    #[test]
    fn una_orden_de_soltar_en_marcha_no_se_duplica_ni_se_pisa_con_tomar() {
        let mut m = maquina();
        m.paso(true, seg(0));
        m.tomar_hecho(true);
        m.paso(false, seg(1));
        assert_eq!(m.paso(false, seg(31)), Some(OrdenWakeLock::Soltar));
        assert_eq!(m.paso(false, seg(32)), None);
        assert_eq!(m.paso(true, seg(33)), None, "no toma mientras suelta");
        m.soltar_hecho(true, seg(34));
        assert_eq!(m.paso(true, seg(35)), Some(OrdenWakeLock::Tomar));
    }

    #[test]
    fn al_terminar_suelta_lo_tomado_y_ya_no_acepta_ordenes() {
        let mut m = maquina();
        m.paso(true, seg(0));
        m.tomar_hecho(true);
        assert_eq!(m.terminar(), Some(OrdenWakeLock::Soltar));
        assert!(m.terminando());
        assert_eq!(m.paso(true, seg(1)), None);
        assert_eq!(m.paso(false, seg(100)), None);
        m.soltar_hecho(true, seg(2));
        assert_eq!(m.fase(), FaseWakeLock::Suelto);
    }

    #[test]
    fn al_terminar_con_la_gracia_a_medias_tambien_suelta() {
        let mut m = maquina();
        m.paso(true, seg(0));
        m.tomar_hecho(true);
        m.paso(false, seg(5));
        assert_eq!(m.fase(), FaseWakeLock::Esperando);
        assert_eq!(m.terminar(), Some(OrdenWakeLock::Soltar));
    }

    #[test]
    fn el_aviso_del_limite_de_procesos_es_de_android_12_y_una_sola_vez() {
        assert!(toca_aviso_limite_procesos(Some(31), false));
        assert!(toca_aviso_limite_procesos(Some(35), false));
        assert!(!toca_aviso_limite_procesos(Some(30), false), "Android 11");
        assert!(!toca_aviso_limite_procesos(None, false), "SDK desconocido");
        assert!(!toca_aviso_limite_procesos(Some(35), true), "ya se dio");
    }
}
