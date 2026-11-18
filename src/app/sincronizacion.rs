//! Estado de la sincronización del inventario en la TUI (Fase 10): vistas
//! SINCRONIZACIÓN y CONFLICTOS, indicador de la barra, diálogos y disparo de
//! las pasadas.
//!
//! El bucle de UI es el único escritor de SQLite, así que las esperas (clave
//! del llavero, cerrojo entre procesos, retraso de exportación y vigilancia de
//! la carpeta) viven en tareas de tokio; cuando el cerrojo está libre, la tarea
//! devuelve EventoInventario::Listo y es el bucle quien ejecuta
//! sincronizacion::pasada con su propia conexión.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::warn;

use crate::almacen::conflictos::Conflicto;
use crate::almacen::equipos::Equipo;
use crate::app::{App, Evento};
use crate::sincronizacion::carpeta::{
    CarpetaDisco, CarpetaSincronizacion, Cerrojo, EntradaCarpeta,
};
use crate::sincronizacion::cifrado::ClaveSincronizacion;
use crate::sincronizacion::{
    BorradosPendientes, EstadoPasada, PoliticaBorrados, ResumenImportacion,
};
use crate::ui::Vista;

/// Lo que la tarea de fondo devuelve al bucle de la UI.
pub enum EventoInventario {
    /// El cerrojo está libre: el bucle ya puede hacer la pasada con su
    /// conexión (es el único escritor de SQLite).
    Listo {
        cerrojo: Cerrojo,
        claves: Vec<ClaveSincronizacion>,
    },
    /// No se pudo empezar la pasada (llavero, carpeta o cerrojo ocupado).
    Error(String),
    /// La carpeta compartida ha cambiado (vigilancia o sondeo de respaldo).
    CarpetaCambiada { generacion: u64 },
}

/// Estado de la interfaz de sincronización. Vive en la App y sobrevive al
/// cambiar de vista.
pub struct EstadoSincronizacion {
    /// Filas de EQUIPOS, este equipo primero.
    pub equipos: Vec<Equipo>,
    /// Conflictos pendientes, del más antiguo al más nuevo.
    pub conflictos: Vec<Conflicto>,
    pub seleccion: usize,
    /// Primera fila visible de la lista de equipos (la fija el pintado).
    pub desplazamiento: usize,
    pub seleccion_conflicto: usize,
    /// Primera fila visible de la lista de conflictos (la fija el pintado).
    pub desplazamiento_conflicto: usize,
    /// Huella corta de la clave del llavero, para comparar a simple vista.
    pub huella: Option<String>,
    /// Estado de la última pasada.
    pub estado: EstadoPasada,
    /// Resumen de la última importación aplicada, si la hubo.
    pub resumen: Option<ResumenImportacion>,
    /// Avisos de la última pasada, legibles en la vista.
    pub avisos: Vec<String>,
    pub conflictos_pendientes: i64,
    /// Detalle del conflicto seleccionado (lo prepara la App al moverse).
    pub detalle: Vec<String>,
    /// Hay una pasada en marcha (esperando cerrojo, clave o aplicando).
    pub sincronizando: bool,
    /// Política con la que se pedirá la próxima pasada.
    pub politica: PoliticaBorrados,
    /// Borrados masivos esperando decisión: la sincronización está en pausa.
    pub pausa: Option<BorradosPendientes>,
    /// Se pidió otra pasada mientras una estaba en marcha.
    pub reintentar: bool,
    /// Cuándo terminó la última pasada (para «hace N»).
    pub terminada_en: Option<chrono::DateTime<chrono::Local>>,
    /// Último reloj híbrido visto de este equipo: detecta cambios locales.
    pub reloj_visto: Option<i64>,
    /// Primer instante en que se vio un cambio local pendiente de exportar.
    pub cambio_desde: Option<Instant>,
    /// Vigilancia de la carpeta en marcha; se sustituye al cambiar de carpeta.
    pub vigilancia: Option<JoinHandle<()>>,
    pub generacion_vigilancia: u64,
    /// Vista a la que vuelve q desde el área de sincronización.
    pub vista_previa: Option<Vista>,
    /// Cuántas pasadas se han pedido, se hagan o no (sin efectos externos no
    /// se hacen). Lo mira la prueba de que Termux pide una al volver a la
    /// vista; nada más.
    pub pasadas_pedidas: u64,
    /// Cada cuánto sondea la vigilancia en marcha: `Some` en Termux (en vez de
    /// notify), `None` en el resto. Lo mira la prueba de que Termux no usa
    /// notify; nada más.
    pub sondeo_vigilancia: Option<Duration>,
    /// Cuánto tarda una lectura de la carpeta en darse por colgada (la
    /// pasada que provoca un aviso corre en el hilo de la UI y no se pide
    /// mientras una lectura así siga sin responder). Lo cambia solo la prueba
    /// de que Termux no pide una pasada al volver a una carpeta colgada.
    pub plazo_lectura_carpeta: Duration,
}

impl Default for EstadoSincronizacion {
    fn default() -> Self {
        Self {
            equipos: Vec::new(),
            conflictos: Vec::new(),
            seleccion: 0,
            desplazamiento: 0,
            seleccion_conflicto: 0,
            desplazamiento_conflicto: 0,
            huella: None,
            estado: EstadoPasada::SinConfigurar,
            resumen: None,
            avisos: Vec::new(),
            conflictos_pendientes: 0,
            detalle: Vec::new(),
            sincronizando: false,
            politica: PoliticaBorrados::Preguntar,
            pausa: None,
            reintentar: false,
            terminada_en: None,
            reloj_visto: None,
            cambio_desde: None,
            vigilancia: None,
            generacion_vigilancia: 0,
            vista_previa: None,
            pasadas_pedidas: 0,
            sondeo_vigilancia: None,
            plazo_lectura_carpeta: PLAZO_FIRMA,
        }
    }
}

impl EstadoSincronizacion {
    /// Clava la selección de las dos listas dentro de lo que haya.
    pub fn ajustar_selecciones(&mut self) {
        if self.seleccion >= self.equipos.len() {
            self.seleccion = self.equipos.len().saturating_sub(1);
        }
        if self.seleccion_conflicto >= self.conflictos.len() {
            self.seleccion_conflicto = self.conflictos.len().saturating_sub(1);
        }
    }

    /// Equipo seleccionado.
    pub fn equipo_seleccionado(&self) -> Option<&Equipo> {
        self.equipos.get(self.seleccion)
    }

    /// Conflicto seleccionado.
    pub fn conflicto_seleccionado(&self) -> Option<&Conflicto> {
        self.conflictos.get(self.seleccion_conflicto)
    }

    /// Texto del estado de la pasada para la cabecera de la vista.
    pub fn texto_estado(&self) -> String {
        match &self.estado {
            EstadoPasada::SinConfigurar => "sin configurar".to_string(),
            EstadoPasada::SinClave => "falta la clave en el llavero".to_string(),
            EstadoPasada::AlDia => "al día".to_string(),
            EstadoPasada::Quitado => "quitado de la sincronización".to_string(),
            EstadoPasada::EsperandoClave(nombre) => {
                format!("esperando la clave nueva ({nombre})")
            }
            EstadoPasada::DecisionPendiente(_) => "en pausa: decidir los borrados".to_string(),
            EstadoPasada::Error(motivo) => format!("error: {motivo}"),
        }
    }
}

/// Espera máxima por el cerrojo desde la TUI: si otra ventana sincroniza, se
/// reintenta y, si no se libera, la pasada se deja para la siguiente.
pub const ESPERA_CERROJO_TUI: Duration = Duration::from_secs(10);
/// Cada cuánto se reintenta el cerrojo mientras está ocupado.
const REINTENTO_CERROJO: Duration = Duration::from_millis(400);
/// Agrupación de los cambios de la carpeta (§4.2: 2 s).
pub const AGRUPACION_CARPETA: Duration = Duration::from_secs(2);
/// Sondeo de respaldo cuando notify no puede vigilar la carpeta.
pub const SONDEO_CARPETA: Duration = Duration::from_secs(30);
/// En Termux, cada cuánto se pide una pasada completa aunque la firma de la
/// carpeta no haya cambiado (§4.3): una modificación que deje igual el nombre,
/// el tamaño y la fecha, o un cambio que el sondeo no vio, no puede quedar sin
/// importar para siempre.
pub const SEGURIDAD_CARPETA_TERMUX: Duration = Duration::from_secs(600);
/// Plazo de cada lectura de la carpeta compartida en el sondeo: el
/// almacenamiento compartido de Android puede ir muy despacio.
pub const PLAZO_FIRMA: Duration = Duration::from_secs(10);

/// Lanza la tarea que espera el cerrojo y las claves y avisa al bucle. La
/// pasada la ejecuta la UI, que es el único escritor de SQLite.
pub fn lanzar_espera(
    runtime: &tokio::runtime::Runtime,
    eventos: mpsc::UnboundedSender<Evento>,
    rutas: crate::config::Rutas,
    config: crate::config::Config,
    llavero: std::sync::Arc<dyn crate::llavero::LlaveroCliente>,
) {
    runtime.spawn(async move {
        let claves = match crate::sincronizacion::equipos::claves_para_leer(&config, &*llavero) {
            Ok(claves) => claves,
            Err(error) => {
                let _ = eventos.send(Evento::Inventario(EventoInventario::Error(error)));
                return;
            }
        };
        let limite = tokio::time::Instant::now() + ESPERA_CERROJO_TUI;
        loop {
            match crate::sincronizacion::carpeta::adquirir_cerrojo(&rutas, None) {
                Ok(Some(cerrojo)) => {
                    let _ = eventos.send(Evento::Inventario(EventoInventario::Listo {
                        cerrojo,
                        claves,
                    }));
                    return;
                }
                Ok(None) => {
                    if tokio::time::Instant::now() >= limite {
                        let _ = eventos.send(Evento::Inventario(EventoInventario::Error(
                            "otra ventana está sincronizando; se reintentará".to_string(),
                        )));
                        return;
                    }
                    tokio::time::sleep(REINTENTO_CERROJO).await;
                }
                Err(error) => {
                    let _ = eventos.send(Evento::Inventario(EventoInventario::Error(format!(
                        "no se pudo tomar el cerrojo de sincronización: {error}"
                    ))));
                    return;
                }
            }
        }
    });
}

/// Vigila la carpeta compartida: avisa de cada cambio agrupado 2 s. Con notify
/// si se puede; si no, sondea cada 30 s. La generación descarta los avisos de
/// una vigilancia que ya no está vigente.
pub async fn vigilar(carpeta: PathBuf, generacion: u64, eventos: mpsc::UnboundedSender<Evento>) {
    use notify::{RecursiveMode, Watcher};
    use tokio::sync::mpsc::unbounded_channel;

    let (avisos, mut cambios) = unbounded_channel::<()>();
    // El vigilante tiene que seguir vivo mientras dure la tarea: si se cae,
    // queda el sondeo de respaldo.
    let vigilante = notify::recommended_watcher(move |resultado: notify::Result<notify::Event>| {
        let Ok(evento) = resultado else {
            return;
        };
        if !evento
            .paths
            .iter()
            .any(|ruta| es_fichero_de_sincronizacion(ruta))
        {
            return;
        }
        let _ = avisos.send(());
    });
    let mut vigilante = match vigilante {
        Ok(mut vigilante) => match vigilante.watch(&carpeta, RecursiveMode::NonRecursive) {
            Ok(()) => Some(vigilante),
            Err(_) => None,
        },
        Err(_) => None,
    };
    if vigilante.is_some() {
        while cambios.recv().await.is_some() {
            tokio::time::sleep(AGRUPACION_CARPETA).await;
            while cambios.try_recv().is_ok() {}
            if eventos
                .send(Evento::Inventario(EventoInventario::CarpetaCambiada {
                    generacion,
                }))
                .is_err()
            {
                return;
            }
        }
        return;
    }
    // Respaldo: un sondeo cada 30 s mientras la tarea siga viva.
    let _ = &mut vigilante;
    loop {
        tokio::time::sleep(SONDEO_CARPETA).await;
        if eventos
            .send(Evento::Inventario(EventoInventario::CarpetaCambiada {
                generacion,
            }))
            .is_err()
        {
            return;
        }
    }
}

/// Vigila la carpeta compartida como corresponde a la plataforma: con
/// `sondeo_termux` (Termux, `[termux] sincronizacion_sondeo_seg`) la sondea
/// con una firma barata; sin él, la vigilancia de siempre ([`vigilar`]: notify
/// con respaldo de 30 s). Es lo único que cambia entre las dos: la pasada que
/// provoca el aviso es la misma.
pub async fn vigilar_carpeta(
    carpeta: PathBuf,
    generacion: u64,
    eventos: mpsc::UnboundedSender<Evento>,
    sondeo_termux: Option<Duration>,
) {
    match sondeo_termux {
        Some(periodo) => {
            sondear(
                carpeta,
                generacion,
                eventos,
                periodo,
                SEGURIDAD_CARPETA_TERMUX,
            )
            .await;
        }
        None => vigilar(carpeta, generacion, eventos).await,
    }
}

/// Lo que se compara entre dos sondeos de la carpeta: nombre, tamaño y fecha
/// de modificación de los ficheros de equipo de MAGI, sin abrirlos. `None`: la
/// carpeta no se pudo leer.
pub type FirmaCarpeta = Option<Vec<EntradaCarpeta>>;

/// La firma barata de la carpeta (§4.3): un listado y los metadatos de cada
/// fichero de MAGI, sin leerlos ni calcular su hash. Los temporales, las
/// copias en conflicto de la herramienta de sincronización, los enlaces y
/// cualquier fichero ajeno no cuentan (las reglas de `CarpetaDisco`, que son
/// las de la pasada).
pub fn firma_carpeta(carpeta: &Path) -> FirmaCarpeta {
    CarpetaDisco::nueva(carpeta).listar().ok()
}

/// Quién lee la firma de la carpeta y cómo se lanza esa lectura. Es el punto
/// que las pruebas sustituyen para tener una carpeta que no responde o un
/// sistema que no puede crear hilos; en el programa, siempre [`LectorReal`].
#[doc(hidden)]
pub trait LectorFirma: Send + Sync {
    /// La firma de la carpeta (bloquea lo que tarde el almacenamiento).
    fn leer(&self, carpeta: &Path) -> FirmaCarpeta {
        firma_carpeta(carpeta)
    }

    /// Lanza `trabajo` en un hilo propio. Falla si el sistema no puede crear
    /// más hilos (el límite de procesos de Android).
    fn lanzar(&self, trabajo: Box<dyn FnOnce() + Send + 'static>) -> std::io::Result<()> {
        std::thread::Builder::new()
            .name("magi-firma-carpeta".to_string())
            .spawn(trabajo)
            .map(|_| ())
    }
}

/// El lector de siempre: la firma real y un hilo del sistema.
struct LectorReal;

impl LectorFirma for LectorReal {}

/// Una lectura de la firma que todavía no ha terminado: su hilo sigue vivo.
struct LecturaEnVuelo {
    id: u64,
    carpeta: PathBuf,
    desde: Instant,
}

/// Las lecturas de la firma en vuelo de todo el proceso. La UI las consulta
/// antes de pedir una pasada (que corre en su hilo y lee la carpeta sin
/// plazo): con el almacenamiento colgado, esa pasada congelaría la TUI.
static LECTURAS_EN_VUELO: Mutex<Vec<LecturaEnVuelo>> = Mutex::new(Vec::new());
static SIGUIENTE_LECTURA: AtomicU64 = AtomicU64::new(0);

/// Anota una lectura en vuelo y la retira al soltarse, también si el hilo
/// termina en pánico o no llega a crearse.
struct MarcaDeLectura(u64);

impl MarcaDeLectura {
    fn nueva(carpeta: &Path) -> Self {
        let id = SIGUIENTE_LECTURA.fetch_add(1, Ordering::Relaxed);
        let mut lecturas = LECTURAS_EN_VUELO.lock().unwrap_or_else(|e| e.into_inner());
        lecturas.push(LecturaEnVuelo {
            id,
            carpeta: carpeta.to_path_buf(),
            desde: Instant::now(),
        });
        Self(id)
    }
}

impl Drop for MarcaDeLectura {
    fn drop(&mut self) {
        let mut lecturas = LECTURAS_EN_VUELO.lock().unwrap_or_else(|e| e.into_inner());
        lecturas.retain(|lectura| lectura.id != self.0);
    }
}

/// ¿Hay una lectura de la carpeta que lleva `plazo` o más sin responder?
/// Mientras la haya, no se pide ninguna pasada: leería la carpeta en el hilo
/// de la UI, sin plazo, y la congelaría.
pub fn lectura_de_carpeta_colgada(carpeta: &Path, plazo: Duration) -> bool {
    let lecturas = LECTURAS_EN_VUELO.lock().unwrap_or_else(|e| e.into_inner());
    lecturas
        .iter()
        .any(|lectura| lectura.carpeta == carpeta && lectura.desde.elapsed() >= plazo)
}

/// Sondea la carpeta cada `periodo` y avisa solo si la firma cambia, más una
/// pasada de seguridad cada `seguridad` aunque no haya cambiado. Todo el
/// trabajo de leer la carpeta va en un hilo aparte con plazo (la carpeta puede
/// estar en un almacenamiento lento) y el bucle de la UI solo recibe el aviso.
/// La primera firma es la línea base: no avisa (quien arranca la vigilancia ya
/// pide su pasada). Mientras una lectura no responda no avisa de nada, ni por
/// un cambio ni por la seguridad: la pasada que provocaría leería la carpeta
/// en el hilo de la UI, sin plazo.
pub async fn sondear(
    carpeta: PathBuf,
    generacion: u64,
    eventos: mpsc::UnboundedSender<Evento>,
    periodo: Duration,
    seguridad: Duration,
) {
    sondear_con(
        carpeta,
        generacion,
        eventos,
        periodo,
        seguridad,
        PLAZO_FIRMA,
        Arc::new(LectorReal),
    )
    .await;
}

/// [`sondear`] con el plazo de cada lectura y el lector inyectables
/// (superficie de pruebas).
#[doc(hidden)]
pub async fn sondear_con(
    carpeta: PathBuf,
    generacion: u64,
    eventos: mpsc::UnboundedSender<Evento>,
    periodo: Duration,
    seguridad: Duration,
    plazo: Duration,
    lector: Arc<dyn LectorFirma>,
) {
    let periodo = periodo.max(Duration::from_millis(10));
    let mut anterior: Option<FirmaCarpeta> = None;
    let mut ultima_pasada = tokio::time::Instant::now();
    let mut en_curso: Option<oneshot::Receiver<FirmaCarpeta>> = None;
    loop {
        let mut pedir = false;
        if let Some(firma) = leer_firma(&carpeta, &mut en_curso, plazo, &lector).await {
            if anterior.as_ref().is_some_and(|previa| *previa != firma) {
                pedir = true;
            }
            anterior = Some(firma);
        }
        if en_curso.is_some() {
            // La lectura no ha respondido en su plazo: el almacenamiento está
            // colgado. Ninguna pasada hasta que responda (la de seguridad
            // sigue debida y saldrá entonces).
            pedir = false;
        } else if ultima_pasada.elapsed() >= seguridad {
            pedir = true;
        }
        if pedir {
            ultima_pasada = tokio::time::Instant::now();
            if eventos
                .send(Evento::Inventario(EventoInventario::CarpetaCambiada {
                    generacion,
                }))
                .is_err()
            {
                return;
            }
        }
        tokio::time::sleep(periodo).await;
    }
}

/// Dice una sola vez por sesión que no se pudo crear el hilo de lectura.
static AVISADO_SIN_HILO: AtomicBool = AtomicBool::new(false);

/// Lee la firma en un hilo propio y con plazo. Si la lectura anterior sigue
/// sin terminar (almacenamiento colgado), no lanza otra encima: espera a esa.
/// `None`: todavía no hay firma nueva (plazo agotado, el hilo murió o no se
/// pudo crear). Con `en_curso` en `Some` al volver, la lectura sigue sin
/// responder.
///
/// Es un hilo suelto y no `spawn_blocking`: un almacenamiento que no responde
/// dejaría la tarea de bloqueo colgada y el runtime la esperaría al cerrarse
/// MAGI; un hilo suelto muere con el proceso.
///
/// Si el sistema no deja crear el hilo, se avisa en el log (una sola vez por
/// sesión y sin datos) y el sondeo sigue sin firma: solo queda la pasada de
/// seguridad, que no depende de la lectura.
async fn leer_firma(
    carpeta: &Path,
    en_curso: &mut Option<oneshot::Receiver<FirmaCarpeta>>,
    plazo: Duration,
    lector: &Arc<dyn LectorFirma>,
) -> Option<FirmaCarpeta> {
    if en_curso.is_none() {
        let (enviar, recibir) = oneshot::channel();
        let ruta = carpeta.to_path_buf();
        let marca = MarcaDeLectura::nueva(carpeta);
        let lector_del_hilo = Arc::clone(lector);
        let lanzado = lector.lanzar(Box::new(move || {
            let _marca = marca;
            let _ = enviar.send(lector_del_hilo.leer(&ruta));
        }));
        if let Err(error) = lanzado {
            if !AVISADO_SIN_HILO.swap(true, Ordering::Relaxed) {
                warn!(
                    "no se pudo crear el hilo que lee la carpeta de sincronización ({error}); \
                     solo se pedirá la pasada de seguridad"
                );
            }
            return None;
        }
        *en_curso = Some(recibir);
    }
    let recibir = en_curso.as_mut()?;
    match tokio::time::timeout(plazo, recibir).await {
        Ok(resultado) => {
            *en_curso = None;
            resultado.ok()
        }
        Err(_) => None,
    }
}

impl App {
    /// En Termux, al volver a la vista SINCRONIZACIÓN se pide una pasada: lo
    /// que haya llegado a la carpeta desde el último sondeo se ve ya (§4.3).
    /// No hace nada fuera de Termux.
    pub(crate) fn pedir_pasada_al_volver(&mut self) {
        let en_pausa = self.sincronizacion.pausa.is_some();
        if !debe_pedir_pasada_al_volver(
            crate::termux::es_termux(),
            &self.sincronizacion.estado,
            en_pausa,
        ) {
            return;
        }
        // Con la carpeta colgada (una lectura del sondeo sin responder), la
        // pasada leería de ella en este hilo, sin plazo: se espera a que
        // responda.
        let colgada = self
            .config
            .sincronizacion
            .ruta_carpeta(&self.rutas.hogar)
            .is_some_and(|carpeta| {
                lectura_de_carpeta_colgada(&carpeta, self.sincronizacion.plazo_lectura_carpeta)
            });
        if !colgada {
            self.solicitar_pasada();
        }
    }
}

/// ¿Se pide una pasada al volver a la vista? Solo en Termux y no cuando la
/// sincronización espera una decisión del usuario (borrados en pausa, clave
/// nueva o equipo quitado): una pasada más no la resuelve y reabriría el
/// diálogo.
pub fn debe_pedir_pasada_al_volver(termux: bool, estado: &EstadoPasada, en_pausa: bool) -> bool {
    termux
        && !en_pausa
        && !matches!(
            estado,
            EstadoPasada::Quitado | EstadoPasada::EsperandoClave(_)
        )
}

/// ¿El nombre del fichero es de un equipo de MAGI? Los temporales y las
/// copias en conflicto de la herramienta de sincronización se ignoran también
/// aquí, para no despertar la pasada por nada.
fn es_fichero_de_sincronizacion(ruta: &std::path::Path) -> bool {
    ruta.file_name()
        .and_then(|nombre| nombre.to_str())
        .and_then(crate::sincronizacion::carpeta::uuid_de_nombre)
        .is_some()
}

/// «hace 2 min» de una fecha RFC 3339, o None si no se puede leer.
pub fn hace(fecha: &str) -> Option<String> {
    let instante = chrono::DateTime::parse_from_rfc3339(fecha).ok()?;
    let segundos = (chrono::Local::now() - instante.with_timezone(&chrono::Local))
        .num_seconds()
        .max(0);
    Some(hace_segundos(segundos))
}

/// «hace N s/min/h/días» de una antigüedad en segundos.
pub fn hace_segundos(segundos: i64) -> String {
    match segundos {
        s if s < 60 => format!("hace {s} s"),
        s if s < 3600 => format!("hace {} min", s / 60),
        s if s < 86_400 => format!("hace {} h", s / 3600),
        86_400 => "hace 1 día".to_string(),
        s => format!("hace {} días", s / 86_400),
    }
}

/// Texto de una fecha absoluta corta (05/11 09:12).
pub fn fecha_corta(fecha: &str) -> String {
    let Ok(instante) = chrono::DateTime::parse_from_rfc3339(fecha) else {
        return "—".to_string();
    };
    instante
        .with_timezone(&chrono::Local)
        .format("%d/%m %H:%M")
        .to_string()
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn las_antiguedades_se_leen_en_castellano() {
        assert_eq!(hace_segundos(0), "hace 0 s");
        assert_eq!(hace_segundos(30), "hace 30 s");
        assert_eq!(hace_segundos(120), "hace 2 min");
        assert_eq!(hace_segundos(3_600), "hace 1 h");
        assert_eq!(hace_segundos(86_400), "hace 1 día");
        assert_eq!(hace_segundos(2 * 86_400), "hace 2 días");
    }

    #[test]
    fn una_fecha_ajena_no_rompe_la_vista() {
        assert_eq!(hace("no es una fecha"), None);
        assert_eq!(fecha_corta("no es una fecha"), "—");
    }

    #[test]
    fn solo_los_nombres_estrictos_despiertan_la_pasada() {
        let uuid = "2f9a1c3e-4b5d-4e6f-8a7b-9c0d1e2f3a4b";
        assert!(es_fichero_de_sincronizacion(std::path::Path::new(
            &format!("magi-{uuid}.age")
        )));
        assert!(!es_fichero_de_sincronizacion(std::path::Path::new(
            "magi-2f9a1c3e-4b5d-4e6f-8a7b-9c0d1e2f3a4b.sync-conflict-20260101.age"
        )));
        assert!(!es_fichero_de_sincronizacion(std::path::Path::new(
            "otro.txt"
        )));
    }
}
