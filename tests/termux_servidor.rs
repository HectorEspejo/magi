//! El servidor en Termux (Fase 17, Sprint 2, pista del servidor): el wake lock,
//! los avisos de Android, el vigilante de las sesiones persistentes sin
//! ventanas, lo que el servidor le dice a la barra del cliente y
//! `magi termux aviso`.
//!
//! Todo con el servidor real en el mismo proceso, los dobles de las órdenes
//! `termux-*` de `tests/fijos/termux` (anotan sus argumentos y saben fallar o
//! colgarse), el servidor SSH de pruebas de `tests/comun` y, para las fichas,
//! el tmux falso de `tests/fijos/terminal`. Sin red externa, sin llavero y sin
//! tocar el HOME: cada prueba tiene su directorio.
//!
//! `es_termux()` se decide una sola vez por proceso y aquí es `false`: el
//! servidor recibe su Termux por `arrancar_con` (lo que `arrancar` saca del
//! proceso solo existe en Termux). Los tiempos se acortan por la misma vía
//! (`gracia_wake_lock` de 30 s y `tic_avisos` de 3 s) y por
//! `[persistentes] vigilar_ms_pruebas`.

mod comun;
#[path = "fijos/termux/mod.rs"]
mod dobles;

use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;
use std::time::Duration;

use tokio::io::AsyncWriteExt as _;
use tokio::time::Instant;

use magi::config::{Config, Rutas};
use magi::protocolo::{
    EstadoSesionRemota, MensajeCliente, MensajeServidor, PeticionTmux, ServidorTmux,
    VERSION_PROTOCOLO,
};
use magi::servidor::android::OpcionesTermux;
use magi::termux::avisos::ACCION;
use magi::termux::estado::{self, AvisoServidor, Marca};
use magi::termux::Propiedades;
use magi_nucleo::sesiones_persistentes::{DatosFicha, OrigenFicha};

use comun::{escenario, Escenario, OpcionesEscenario};
use dobles::{Dobles, Modo};

const PLAZO: Duration = Duration::from_secs(15);

/// El tmux falso del repositorio.
const TMUX_FALSO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fijos/terminal/tmux");

/// El `tmux` que ven los `exec` del host de pruebas: el falso, con el estado
/// de cada usuario del host (`MAGI_USUARIO`).
const GUION_TMUX: &str = r#"#!/bin/sh
usuario=${MAGI_USUARIO:-ninguno}
base=${MAGI_PRUEBA_BASE:?}
MAGI_TMUX_ESTADO="$base/estado.$usuario"
MAGI_TMUX_LLAMADAS="$base/llamadas.$usuario"
export MAGI_TMUX_ESTADO MAGI_TMUX_LLAMADAS
exec "${MAGI_TMUX_FALSO:?}" "$@"
"#;

// ------------------------------------------------------------------- apoyo

/// El directorio de una prueba: los dobles de Termux, el tmux falso y su
/// estado.
struct Banco {
    base: tempfile::TempDir,
    dobles: Dobles,
}

impl Banco {
    fn nuevo() -> Self {
        let base = tempfile::tempdir().expect("directorio de la prueba");
        let bin = base.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let guion = bin.join("tmux");
        std::fs::write(&guion, GUION_TMUX).unwrap();
        std::fs::set_permissions(&guion, std::fs::Permissions::from_mode(0o755)).unwrap();
        let termux = base.path().join("termux");
        std::fs::create_dir_all(&termux).unwrap();
        let dobles = Dobles::nuevos(&termux);
        Self { base, dobles }
    }

    /// Lo de Termux del servidor: los dobles en el `PATH`, el SDK de Android
    /// que se pida y los tiempos acortados (wake lock a 1,5 s en vez de 30 s y
    /// avisos cada 100 ms en vez de 3 s).
    fn termux(&self, sdk: &str) -> OpcionesTermux {
        let mut opciones = OpcionesTermux::nuevas(
            self.dobles.entorno(&self.base.path().join("home")),
            Propiedades::fijas(&[("ro.build.version.sdk", sdk)]),
        );
        opciones.gracia_wake_lock = Duration::from_millis(1500);
        opciones.tic_avisos = Duration::from_millis(100);
        opciones
    }

    fn binarios(&self) -> PathBuf {
        self.base.path().join("bin")
    }

    fn entorno_del_host(&self, extra: &[(&str, &str)]) -> Vec<(String, String)> {
        let mut entorno = vec![
            (
                "MAGI_PRUEBA_BASE".to_string(),
                self.base.path().display().to_string(),
            ),
            ("MAGI_TMUX_FALSO".to_string(), TMUX_FALSO.to_string()),
        ];
        entorno.extend(
            extra
                .iter()
                .map(|(clave, valor)| ((*clave).to_string(), (*valor).to_string())),
        );
        entorno
    }

    /// Las sesiones del tmux propio del host número `n`, una línea cada una.
    fn magi(&self, n: usize, lineas: &[String]) {
        std::fs::write(
            self.base.path().join(format!("estado.usuario{n}.magi")),
            lineas.concat(),
        )
        .unwrap();
    }

    /// Las veces que ha empezado una comprobación de `estado` en el host `n`
    /// (cada una empieza con un `tmux -V`).
    fn comprobaciones(&self, n: usize) -> usize {
        std::fs::read_to_string(self.base.path().join(format!("llamadas.usuario{n}")))
            .unwrap_or_default()
            .lines()
            .filter(|linea| linea.trim() == "-V")
            .count()
    }

    fn llamadas(&self, programa: &str) -> usize {
        self.dobles.llamadas(programa).len()
    }
}

fn ahora() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Una sesión en el formato del tmux falso: nombre, clientes, ventanas,
/// creada, equipo, actividad, campana, título y creación.
fn linea(nombre: &str, campana: bool, titulo: &str) -> String {
    format!(
        "{nombre}\t0\t1\tx\tportatil\t{}\t{}\t{titulo}\t{}\n",
        ahora(),
        u8::from(campana),
        ahora()
    )
}

/// La configuración del servidor de las pruebas: sin apagarse por falta de
/// ventanas y con el plazo mínimo de las órdenes de Termux (2 s).
fn configuracion() -> Config {
    let mut config = Config::default();
    config.servidor.gracia_apagado_seg = 600;
    config.termux.api_plazo_seg = 2;
    config
}

/// Espera, sin leer del servidor, a que se cumpla la condición.
async fn esperar_que(plazo: Duration, mut condicion: impl FnMut() -> bool) -> bool {
    let fin = Instant::now() + plazo;
    loop {
        if condicion() {
            return true;
        }
        if Instant::now() >= fin {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}

fn hay_marca(rutas: &Rutas, marca: Marca) -> bool {
    estado::hay_marca(rutas, marca)
}

/// Lo que el servidor dijo en la barra, o nada si no hay fichero.
fn barra(rutas: &Rutas) -> Vec<AvisoServidor> {
    estado::leer_servidor(rutas)
        .map(|estado| estado.avisos)
        .unwrap_or_default()
}

/// El directorio de estado de Termux.
fn dir_estado(rutas: &Rutas) -> PathBuf {
    estado::directorio(rutas)
}

/// Opciones del escenario con Termux y, si se pide, con el tmux falso.
fn opciones(banco: &Banco, termux: Option<OpcionesTermux>, config: Config) -> OpcionesEscenario {
    OpcionesEscenario {
        hosts: 1,
        sftp: false,
        binarios: Some(banco.binarios()),
        entorno_extra: banco.entorno_del_host(&[]),
        config: Some(config),
        termux,
        ..Default::default()
    }
}

/// Para el servidor del escenario y espera a que termine; devuelve cuánto
/// tardó.
async fn parar(esc: &mut Escenario) -> Duration {
    let inicio = Instant::now();
    esc.enviar(&MensajeCliente::Parar).await;
    let fin = tokio::time::timeout(Duration::from_secs(30), &mut esc.tarea_magi).await;
    assert!(fin.is_ok(), "el servidor no terminó en 30 s");
    inicio.elapsed()
}

/// Para el servidor desde otra ventana (el cliente del escenario ya se fue).
async fn parar_desde_otra_ventana(esc: &mut Escenario) {
    let mut ventana = esc.otra_ventana().await;
    ventana.enviar(&MensajeCliente::Parar).await;
    let fin = tokio::time::timeout(Duration::from_secs(30), &mut esc.tarea_magi).await;
    assert!(fin.is_ok(), "el servidor no terminó en 30 s");
}

/// Quita al cliente del escenario: el servidor se queda sin ventanas.
async fn irse(esc: &mut Escenario) {
    esc.escritura.shutdown().await.unwrap();
    // El servidor ve el fin de la conexión y suelta al cliente.
    tokio::time::sleep(Duration::from_millis(600)).await;
}

/// Crea una ficha como lo haría el cliente (el servidor nunca escribe en la
/// tabla) y devuelve su uuid.
fn crear_ficha(esc: &Escenario, host_id: i64, nombre: &str) -> String {
    let almacen = magi::almacen::abrir(&esc.rutas().base_datos()).unwrap();
    let id = magi_nucleo::almacen::sesiones_persistentes::crear(
        almacen.conexion(),
        host_id,
        &DatosFicha {
            nombre: nombre.to_string(),
            carpeta: None,
            comando: Some("claude --secreto-de-la-prueba".to_string()),
            comando_reanudar: None,
            nota: Some("nota-privada-de-la-prueba".to_string()),
            origen: OrigenFicha::Manual,
            creada_por: "portatil".to_string(),
        },
    )
    .unwrap();
    let uuid: String = almacen
        .conexion()
        .query_row(
            "SELECT uuid FROM SESIONES_PERSISTENTES WHERE id = ?1",
            [id],
            |fila| fila.get(0),
        )
        .unwrap();
    almacen.cerrar().unwrap();
    uuid
}

/// El id del aviso de una ficha.
fn id_de(uuid: &str) -> String {
    format!("magi-{}", &uuid[..8])
}

/// Los argumentos exactos de `termux-notification` de un aviso.
fn args_de_aviso(id: &str, titulo: &str, texto: &str) -> Vec<String> {
    [
        "--id",
        id,
        "--group",
        "magi",
        "--alert-once",
        "-t",
        titulo,
        "-c",
        texto,
        "--action",
        ACCION,
    ]
    .map(String::from)
    .to_vec()
}

/// Una pestaña cuyo programa suelta una campana cada vez que recibe una línea.
fn programa_con_campana() -> Option<Vec<String>> {
    Some(vec![
        "sh".to_string(),
        "-c".to_string(),
        "while IFS= read -r linea; do printf '\\007'; done".to_string(),
    ])
}

async fn adjuntar(esc: &mut Escenario, sesion_id: u32) {
    esc.enviar(&MensajeCliente::Adjuntar {
        sesion_id,
        cols: 80,
        filas: 24,
        colores: None,
    })
    .await;
    esc.esperar(|mensaje| {
        matches!(
            mensaje,
            MensajeServidor::PantallaCompleta { sesion_id: id, .. } if *id == sesion_id
        )
    })
    .await;
}

// ------------------------------------------------- servidor sin sesiones

/// Un servidor de Termux sin SSH ni sesiones: para lo que se prueba al
/// arrancar y al terminar. `antes` prepara el estado antes de arrancarlo.
struct Suelto {
    entorno: comun::Entorno,
    tarea: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl Suelto {
    async fn arrancar(
        termux: Option<OpcionesTermux>,
        config: Config,
        antes: impl FnOnce(&Rutas),
    ) -> Self {
        let entorno = comun::entorno();
        antes(&entorno.rutas);
        let tarea = tokio::spawn(magi::servidor::arrancar_con(
            entorno.rutas.clone(),
            config,
            termux,
        ));
        comun::esperar_socket(&magi::servidor::ruta_socket(&entorno.rutas)).await;
        Self { entorno, tarea }
    }

    fn rutas(&self) -> &Rutas {
        &self.entorno.rutas
    }

    /// Lo para con `Parar` y espera a que termine.
    async fn parar(&mut self) {
        let mut lector = comun::cliente(
            &magi::servidor::ruta_socket(self.rutas()),
            VERSION_PROTOCOLO,
        )
        .await;
        comun::enviar(lector.get_mut(), &MensajeCliente::Parar).await;
        let fin = tokio::time::timeout(Duration::from_secs(30), &mut self.tarea).await;
        assert!(fin.is_ok(), "el servidor no terminó en 30 s");
    }
}

impl Drop for Suelto {
    fn drop(&mut self) {
        self.tarea.abort();
    }
}

// ====================================================================
// Wake lock
// ====================================================================

/// AC: un trabajo que se abre y se cierra → `termux-wake-lock` y
/// `termux-wake-unlock` una vez cada una, esta a los 30 s (aquí 1,5 s) sin
/// trabajo, y la marca del estado mientras está tomado.
#[tokio::test]
async fn un_trabajo_que_empieza_y_acaba_toma_y_suelta_el_wake_lock_una_vez_cada_vez() {
    let banco = Banco::nuevo();
    let mut esc = escenario(opciones(&banco, Some(banco.termux("35")), configuracion())).await;
    let rutas = esc.rutas().clone();
    assert_eq!(banco.llamadas("termux-wake-lock"), 0, "sin trabajo, nada");

    let sesion = esc.abrir_sesion(esc.hosts[0]).await;
    assert!(
        esc.hasta(PLAZO, |_| banco.llamadas("termux-wake-lock") == 1)
            .await,
        "no se tomó el wake lock con trabajo"
    );
    assert!(hay_marca(&rutas, Marca::WakeLock), "marca del estado");
    // Con trabajo no se suelta, aunque pase la gracia (1,5 s) con creces.
    esc.escuchar(Duration::from_millis(2800)).await;
    assert_eq!(banco.llamadas("termux-wake-unlock"), 0);
    assert_eq!(
        banco.llamadas("termux-wake-lock"),
        1,
        "no se vuelve a tomar"
    );

    esc.enviar(&MensajeCliente::Cerrar { sesion_id: sesion })
        .await;
    assert!(
        esc.hasta(PLAZO, |_| banco.llamadas("termux-wake-unlock") == 1)
            .await,
        "no se soltó el wake lock al quedar sin trabajo"
    );
    assert!(
        esc.hasta(PLAZO, |_| !hay_marca(&rutas, Marca::WakeLock))
            .await,
        "la marca sigue puesta tras soltarlo"
    );
    // Ni una vez más de cada.
    esc.escuchar(Duration::from_millis(3000)).await;
    assert_eq!(banco.llamadas("termux-wake-lock"), 1);
    assert_eq!(banco.llamadas("termux-wake-unlock"), 1);
    // Las órdenes van sin argumentos ni entrada.
    let toma = &banco.dobles.llamadas("termux-wake-lock")[0];
    assert!(toma.args.is_empty() && toma.entrada.is_empty(), "{toma:?}");
    let suelta = &banco.dobles.llamadas("termux-wake-unlock")[0];
    assert!(
        suelta.args.is_empty() && suelta.entrada.is_empty(),
        "{suelta:?}"
    );
    // Y al apagar el servidor no hay un segundo unlock: ya estaba suelto.
    parar(&mut esc).await;
    assert_eq!(banco.llamadas("termux-wake-unlock"), 1);
}

/// Un trabajo que vuelve antes de la gracia mantiene el wake lock tomado.
#[tokio::test]
async fn si_vuelve_el_trabajo_antes_de_la_gracia_no_se_suelta() {
    let banco = Banco::nuevo();
    let mut esc = escenario(opciones(&banco, Some(banco.termux("35")), configuracion())).await;
    let primera = esc.abrir_sesion(esc.hosts[0]).await;
    assert!(
        esc.hasta(PLAZO, |_| banco.llamadas("termux-wake-lock") == 1)
            .await
    );
    esc.enviar(&MensajeCliente::Cerrar { sesion_id: primera })
        .await;
    // Sin trabajo unos instantes (menos que la gracia) y vuelve.
    esc.hasta(PLAZO, |e| e.sesiones.is_empty()).await;
    let segunda = esc.abrir_sesion(esc.hosts[0]).await;
    esc.escuchar(Duration::from_millis(3500)).await;
    assert_eq!(banco.llamadas("termux-wake-unlock"), 0, "sigue tomado");
    assert_eq!(
        banco.llamadas("termux-wake-lock"),
        1,
        "sin volver a tomarlo"
    );
    esc.enviar(&MensajeCliente::Cerrar { sesion_id: segunda })
        .await;
    assert!(
        esc.hasta(PLAZO, |_| banco.llamadas("termux-wake-unlock") == 1)
            .await
    );
}

/// Cae la conexión de una pestaña: sin reconexión automática queda «caída»
/// hasta que el usuario la cierre o la reconecte a mano.
async fn pestana_caida(esc: &mut Escenario) -> u32 {
    let sesion = esc.abrir_sesion(esc.hosts[0]).await;
    esc.observado.tirar_conexiones().await;
    assert!(
        esc.hasta(PLAZO, |e| e
            .sesiones
            .iter()
            .any(|s| s.id == sesion && s.estado == EstadoSesionRemota::Caida))
            .await,
        "la pestaña no se enteró de la caída: {:?}",
        esc.sesiones
    );
    sesion
}

/// A1: una pestaña caída (reconexión desactivada) que el usuario no ha cerrado
/// sigue en el servidor, pero no es trabajo que mantenga despierto el móvil: a
/// los 30 s (aquí 1,5 s) se suelta el wake lock y desaparece la marca. Si se
/// reconecta, vuelve a contar.
#[tokio::test]
async fn una_pestana_caida_no_mantiene_el_wake_lock_y_al_reconectarla_vuelve_a_contar() {
    let banco = Banco::nuevo();
    let mut opciones = opciones(&banco, Some(banco.termux("35")), configuracion());
    opciones.reconectar = Some(false);
    let mut esc = escenario(opciones).await;
    let rutas = esc.rutas().clone();
    let sesion = esc.abrir_sesion(esc.hosts[0]).await;
    assert!(
        esc.hasta(PLAZO, |_| banco.llamadas("termux-wake-lock") == 1)
            .await,
        "no se tomó el wake lock con la pestaña viva"
    );
    assert!(hay_marca(&rutas, Marca::WakeLock));

    // La conexión cae y la pestaña queda caída, sin cerrar.
    esc.observado.tirar_conexiones().await;
    assert!(
        esc.hasta(PLAZO, |e| e
            .sesiones
            .iter()
            .any(|s| s.id == sesion && s.estado == EstadoSesionRemota::Caida))
            .await,
        "la pestaña no se enteró de la caída: {:?}",
        esc.sesiones
    );
    assert!(
        esc.hasta(PLAZO, |_| banco.llamadas("termux-wake-unlock") == 1)
            .await,
        "una pestaña caída mantiene el wake lock tomado"
    );
    assert!(
        esc.hasta(PLAZO, |_| !hay_marca(&rutas, Marca::WakeLock))
            .await,
        "la marca sigue puesta con una pestaña caída"
    );
    // La pestaña sigue ahí (el servidor la custodia hasta que se cierre) y no
    // se vuelve a tomar ni a soltar nada.
    esc.escuchar(Duration::from_millis(3000)).await;
    assert!(esc
        .sesiones
        .iter()
        .any(|s| s.id == sesion && s.estado == EstadoSesionRemota::Caida));
    assert_eq!(banco.llamadas("termux-wake-lock"), 1);
    assert_eq!(banco.llamadas("termux-wake-unlock"), 1);

    // Reconectada a mano, vuelve a ser trabajo: se toma de nuevo y no se suelta.
    esc.enviar(&MensajeCliente::Reconectar {
        sesion_id: sesion,
        compartir: false,
    })
    .await;
    assert!(
        esc.hasta(PLAZO, |_| banco.llamadas("termux-wake-lock") == 2)
            .await,
        "una pestaña reconectada no volvió a tomar el wake lock"
    );
    esc.escuchar(Duration::from_millis(3000)).await;
    assert_eq!(banco.llamadas("termux-wake-unlock"), 1, "sigue tomado");
    assert!(hay_marca(&rutas, Marca::WakeLock));
    esc.enviar(&MensajeCliente::Cerrar { sesion_id: sesion })
        .await;
    assert!(
        esc.hasta(PLAZO, |_| banco.llamadas("termux-wake-unlock") == 2)
            .await
    );
    // La marca se quita después de la orden de soltar: se espera, no se supone.
    assert!(
        esc.hasta(PLAZO, |_| !hay_marca(&rutas, Marca::WakeLock))
            .await,
        "la marca sigue puesta tras soltar el wake lock"
    );
}

/// A1: una pestaña que está reconectando (`Abriendo` con `reconectando`) sigue
/// siendo trabajo: el wake lock sigue tomado mientras dura la reconexión, y se
/// suelta al cerrarla.
#[tokio::test]
async fn una_pestana_reconectando_sigue_contando_para_el_wake_lock() {
    let banco = Banco::nuevo();
    let mut esc = escenario(opciones(&banco, Some(banco.termux("35")), configuracion())).await;
    let rutas = esc.rutas().clone();
    let sesion = esc.abrir_sesion(esc.hosts[0]).await;
    assert!(
        esc.hasta(PLAZO, |_| banco.llamadas("termux-wake-lock") == 1)
            .await
    );
    // El host se cae del todo: la reconexión automática no puede volver.
    esc.apagar_servidor_ssh().await;
    assert!(
        esc.hasta(PLAZO, |e| e.sesiones.iter().any(|s| s.id == sesion
            && s.estado == EstadoSesionRemota::Abriendo
            && s.reconexion.is_some()))
            .await,
        "la pestaña no empezó a reconectar: {:?}",
        esc.sesiones
    );
    // Pasa la gracia (1,5 s) con creces: sigue tomado.
    esc.escuchar(Duration::from_millis(3500)).await;
    assert!(esc
        .sesiones
        .iter()
        .any(|s| s.id == sesion && s.estado == EstadoSesionRemota::Abriendo));
    assert_eq!(
        banco.llamadas("termux-wake-unlock"),
        0,
        "reconectar es trabajo: no se suelta"
    );
    assert!(hay_marca(&rutas, Marca::WakeLock));
    esc.enviar(&MensajeCliente::Cerrar { sesion_id: sesion })
        .await;
    assert!(
        esc.hasta(PLAZO, |_| banco.llamadas("termux-wake-unlock") == 1)
            .await,
        "no se soltó al cerrar la pestaña que reconectaba"
    );
}

/// `[termux] wake_lock = "nunca"`: no se toca nada.
#[tokio::test]
async fn con_wake_lock_nunca_no_se_ejecuta_ninguna_orden() {
    let banco = Banco::nuevo();
    let mut config = configuracion();
    config.termux.wake_lock = "nunca".to_string();
    let mut esc = escenario(opciones(&banco, Some(banco.termux("35")), config)).await;
    let rutas = esc.rutas().clone();
    let sesion = esc.abrir_sesion(esc.hosts[0]).await;
    esc.escuchar(Duration::from_millis(3000)).await;
    assert_eq!(banco.llamadas("termux-wake-lock"), 0);
    assert_eq!(banco.llamadas("termux-wake-unlock"), 0);
    assert!(!hay_marca(&rutas, Marca::WakeLock));
    assert!(!hay_marca(&rutas, Marca::SegundoPlano), "ni su aviso");
    assert_eq!(barra(&rutas), Vec::<AvisoServidor>::new());
    esc.enviar(&MensajeCliente::Cerrar { sesion_id: sesion })
        .await;
    esc.escuchar(Duration::from_millis(2500)).await;
    parar(&mut esc).await;
    assert_eq!(banco.llamadas("termux-wake-unlock"), 0);
}

/// El servidor anterior murió con el wake lock tomado (marca puesta): el
/// siguiente lo suelta en cuanto no tiene trabajo.
#[tokio::test]
async fn con_la_marca_de_un_servidor_anterior_suelta_el_wake_lock_al_arrancar() {
    let banco = Banco::nuevo();
    let mut servidor = Suelto::arrancar(Some(banco.termux("35")), configuracion(), |rutas| {
        estado::poner_marca(rutas, Marca::WakeLock).unwrap();
    })
    .await;
    assert!(
        esperar_que(PLAZO, || banco.llamadas("termux-wake-unlock") == 1).await,
        "no soltó el wake lock que dejó el servidor anterior"
    );
    assert!(
        esperar_que(PLAZO, || !hay_marca(servidor.rutas(), Marca::WakeLock)).await,
        "la marca sigue puesta"
    );
    assert_eq!(banco.llamadas("termux-wake-lock"), 0, "no toma nada");
    // Ni una más: ya es suyo y ya lo soltó.
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert_eq!(banco.llamadas("termux-wake-unlock"), 1);
    servidor.parar().await;
    assert_eq!(banco.llamadas("termux-wake-unlock"), 1);
}

/// Sin la marca, el servidor nunca suelta un wake lock que no tomó.
#[tokio::test]
async fn sin_marca_y_sin_trabajo_no_se_suelta_nada() {
    let banco = Banco::nuevo();
    let mut servidor = Suelto::arrancar(Some(banco.termux("35")), configuracion(), |_| {}).await;
    tokio::time::sleep(Duration::from_millis(3500)).await;
    assert_eq!(banco.llamadas("termux-wake-lock"), 0);
    assert_eq!(banco.llamadas("termux-wake-unlock"), 0);
    servidor.parar().await;
    assert_eq!(banco.llamadas("termux-wake-unlock"), 0, "ni al terminar");
}

/// Si `termux-wake-lock` falla, el servidor deja de intentarlo, no deja
/// marca y lo dice en la barra («sin wake lock»).
#[tokio::test]
async fn si_falla_el_wake_lock_se_apaga_y_lo_dice_la_barra() {
    let banco = Banco::nuevo();
    banco.dobles.programar("termux-wake-lock", Modo::Fallar);
    let mut esc = escenario(opciones(&banco, Some(banco.termux("35")), configuracion())).await;
    let rutas = esc.rutas().clone();
    assert_eq!(barra(&rutas), Vec::<AvisoServidor>::new(), "empieza limpia");
    let sesion = esc.abrir_sesion(esc.hosts[0]).await;
    assert!(
        esc.hasta(PLAZO, |_| barra(&rutas)
            .contains(&AvisoServidor::SinWakeLock))
            .await,
        "la barra no dice «sin wake lock»: {:?}",
        barra(&rutas)
    );
    // Seguir con trabajo no lo reintenta en bucle.
    esc.escuchar(Duration::from_millis(3500)).await;
    assert_eq!(banco.llamadas("termux-wake-lock"), 1, "un solo intento");
    assert!(!hay_marca(&rutas, Marca::WakeLock), "no tomó nada");
    assert!(
        !barra(&rutas).contains(&AvisoServidor::LimiteProcesos),
        "el aviso de Android 12 es tras tomarlo"
    );
    esc.enviar(&MensajeCliente::Cerrar { sesion_id: sesion })
        .await;
    esc.escuchar(Duration::from_millis(2500)).await;
    parar(&mut esc).await;
    assert_eq!(
        banco.llamadas("termux-wake-unlock"),
        0,
        "no se suelta lo que no se tomó"
    );
}

/// Un `termux-wake-lock` que no responde: a los 2 s del plazo, apagado y con
/// su aviso; el proceso colgado y lo que lanzó mueren. La marca se conserva
/// (puede estar tomado) y se suelta sin trabajo (A3).
#[tokio::test]
async fn un_wake_lock_que_no_responde_vence_su_plazo_y_se_mata() {
    let banco = Banco::nuevo();
    banco.dobles.programar("termux-wake-lock", Modo::Colgar);
    let mut esc = escenario(opciones(&banco, Some(banco.termux("35")), configuracion())).await;
    let rutas = esc.rutas().clone();
    let inicio = Instant::now();
    let sesion = esc.abrir_sesion(esc.hosts[0]).await;
    assert!(
        esc.hasta(PLAZO, |_| barra(&rutas)
            .contains(&AvisoServidor::SinWakeLock))
            .await,
        "no se dio por apagado el wake lock: {:?}",
        barra(&rutas)
    );
    assert!(
        inicio.elapsed() < Duration::from_secs(9),
        "esperó más que el plazo de la orden"
    );
    let llamada = &banco.dobles.llamadas("termux-wake-lock")[0];
    assert!(!llamada.pids.is_empty(), "el doble anotó sus procesos");
    for pid in &llamada.pids {
        assert!(
            !dobles::sigue_vivo_tras(*pid, Duration::from_secs(3)),
            "el proceso {pid} de la orden colgada sigue vivo"
        );
    }
    // A3: la orden pudo llegar a Termux antes de que se la matara, así que el
    // wake lock puede estar tomado: la marca se conserva (como con un aviso
    // sin respuesta, que se anota para retirarlo).
    assert!(
        hay_marca(&rutas, Marca::WakeLock),
        "una orden sin respuesta pudo tomarlo: la marca se queda"
    );
    // Apagado: seguir con trabajo no la reintenta.
    esc.escuchar(Duration::from_millis(2800)).await;
    assert_eq!(banco.llamadas("termux-wake-lock"), 1, "un solo intento");
    assert_eq!(banco.llamadas("termux-wake-unlock"), 0, "hay trabajo");
    // Sin trabajo, pasada la gracia, se suelta (por si llegó a tomarse) y la
    // marca se quita.
    esc.enviar(&MensajeCliente::Cerrar { sesion_id: sesion })
        .await;
    assert!(
        esc.hasta(PLAZO, |_| banco.llamadas("termux-wake-unlock") == 1)
            .await,
        "no se soltó el wake lock que pudo tomarse"
    );
    assert!(
        esc.hasta(PLAZO, |_| !hay_marca(&rutas, Marca::WakeLock))
            .await,
        "la marca sigue puesta tras soltarlo"
    );
    parar(&mut esc).await;
    assert_eq!(banco.llamadas("termux-wake-unlock"), 1, "ni uno más");
}

/// A3: el servidor que termina con un `termux-wake-lock` sin respuesta suelta
/// el wake lock que pudo tomarse (sin esperar a que lo haga el siguiente).
#[tokio::test]
async fn al_terminar_con_un_wake_lock_sin_respuesta_se_suelta_el_que_pudo_tomarse() {
    let banco = Banco::nuevo();
    banco.dobles.programar("termux-wake-lock", Modo::Colgar);
    let mut esc = escenario(opciones(&banco, Some(banco.termux("35")), configuracion())).await;
    let rutas = esc.rutas().clone();
    esc.abrir_sesion(esc.hosts[0]).await;
    assert!(
        esc.hasta(PLAZO, |_| barra(&rutas)
            .contains(&AvisoServidor::SinWakeLock))
            .await,
        "no se dio por apagado el wake lock: {:?}",
        barra(&rutas)
    );
    assert!(hay_marca(&rutas, Marca::WakeLock));
    assert_eq!(banco.llamadas("termux-wake-unlock"), 0);
    parar(&mut esc).await;
    assert_eq!(
        banco.llamadas("termux-wake-unlock"),
        1,
        "no soltó al terminar el wake lock que pudo tomarse"
    );
    assert!(!hay_marca(&rutas, Marca::WakeLock));
}

/// Android 12 o posterior: aviso único sobre el límite de procesos en segundo
/// plano, con su marca; el siguiente servidor no lo repite.
#[tokio::test]
async fn en_android_12_se_avisa_del_limite_de_procesos_una_sola_vez() {
    let banco = Banco::nuevo();
    let config = configuracion();
    let mut esc = escenario(opciones(&banco, Some(banco.termux("35")), config.clone())).await;
    let rutas = esc.rutas().clone();
    let host = esc.hosts[0];
    esc.abrir_sesion(host).await;
    assert!(
        esc.hasta(PLAZO, |_| barra(&rutas)
            .contains(&AvisoServidor::LimiteProcesos))
            .await,
        "la barra no dice nada del límite de procesos: {:?}",
        barra(&rutas)
    );
    assert!(hay_marca(&rutas, Marca::SegundoPlano));
    assert!(
        !barra(&rutas).contains(&AvisoServidor::SinWakeLock),
        "el wake lock se tomó bien"
    );
    parar(&mut esc).await;
    assert_eq!(banco.llamadas("termux-wake-lock"), 1);

    // Otro servidor del mismo equipo (misma marca): vuelve a tomar el wake
    // lock, pero el aviso ya se dio en la vida del equipo.
    let tarea = tokio::spawn(magi::servidor::arrancar_con(
        rutas.clone(),
        config,
        Some(banco.termux("35")),
    ));
    comun::esperar_socket(&magi::servidor::ruta_socket(&rutas)).await;
    let mut ventana = esc.otra_ventana().await;
    ventana
        .enviar(&MensajeCliente::AbrirSesion {
            comandos_iniciales: Vec::new(),
            host_id: host,
            cols: 80,
            filas: 24,
            orden: None,
            tmux: None,
        })
        .await;
    ventana
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::Sesiones { lista }
                    if lista.iter().any(|s| s.estado == EstadoSesionRemota::Abierta)
            )
        })
        .await;
    assert!(
        esperar_que(PLAZO, || banco.llamadas("termux-wake-lock") == 2).await,
        "el segundo servidor no tomó el wake lock"
    );
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(
        barra(&rutas),
        Vec::<AvisoServidor>::new(),
        "el aviso único no se repite"
    );
    ventana.enviar(&MensajeCliente::Parar).await;
    let _ = tokio::time::timeout(Duration::from_secs(30), tarea).await;
}

/// Android 11 o anterior: nada del límite de procesos.
#[tokio::test]
async fn en_android_11_no_hay_aviso_del_limite_de_procesos() {
    let banco = Banco::nuevo();
    let mut esc = escenario(opciones(&banco, Some(banco.termux("30")), configuracion())).await;
    let rutas = esc.rutas().clone();
    esc.abrir_sesion(esc.hosts[0]).await;
    assert!(
        esc.hasta(PLAZO, |_| banco.llamadas("termux-wake-lock") == 1)
            .await
    );
    esc.escuchar(Duration::from_millis(1200)).await;
    assert!(!hay_marca(&rutas, Marca::SegundoPlano));
    assert_eq!(barra(&rutas), Vec::<AvisoServidor>::new());
}

/// Al terminar el servidor con el wake lock tomado, lo suelta, y quita su
/// marca y el fichero de la barra.
#[tokio::test]
async fn al_terminar_el_servidor_suelta_el_wake_lock_tomado() {
    let banco = Banco::nuevo();
    let mut esc = escenario(opciones(&banco, Some(banco.termux("35")), configuracion())).await;
    let rutas = esc.rutas().clone();
    esc.abrir_sesion(esc.hosts[0]).await;
    assert!(
        esc.hasta(PLAZO, |_| banco.llamadas("termux-wake-lock") == 1)
            .await
    );
    assert!(hay_marca(&rutas, Marca::WakeLock));
    assert!(dir_estado(&rutas).join("servidor").exists());
    let tardo = parar(&mut esc).await;
    assert_eq!(banco.llamadas("termux-wake-unlock"), 1, "lo soltó al salir");
    assert!(!hay_marca(&rutas, Marca::WakeLock));
    assert!(
        !dir_estado(&rutas).join("servidor").exists(),
        "el fichero de la barra no sobrevive al servidor"
    );
    assert!(tardo < Duration::from_secs(8), "el apagado tardó {tardo:?}");
}

/// Un `termux-wake-unlock` que se cuelga no alarga el apagado más que el
/// plazo de la orden y 3 s.
#[tokio::test]
async fn el_apagado_no_espera_mas_de_lo_razonable_a_un_unlock_colgado() {
    let banco = Banco::nuevo();
    banco.dobles.programar("termux-wake-unlock", Modo::Colgar);
    let mut esc = escenario(opciones(&banco, Some(banco.termux("35")), configuracion())).await;
    let rutas = esc.rutas().clone();
    esc.abrir_sesion(esc.hosts[0]).await;
    assert!(
        esc.hasta(PLAZO, |_| banco.llamadas("termux-wake-lock") == 1)
            .await
    );
    let tardo = parar(&mut esc).await;
    assert!(
        tardo < Duration::from_secs(7),
        "el apagado tardó {tardo:?} esperando al wake lock"
    );
    assert_eq!(banco.llamadas("termux-wake-unlock"), 1);
    // Sin soltarlo queda la marca: el siguiente servidor lo intentará.
    assert!(hay_marca(&rutas, Marca::WakeLock));
    let llamada = &banco.dobles.llamadas("termux-wake-unlock")[0];
    for pid in &llamada.pids {
        assert!(!dobles::sigue_vivo_tras(*pid, Duration::from_secs(3)));
    }
}

/// El servidor del escritorio (sin Termux) no escribe ni lanza nada.
#[tokio::test]
async fn el_servidor_del_escritorio_no_toca_nada_de_termux() {
    let banco = Banco::nuevo();
    let mut esc = escenario(opciones(&banco, None, configuracion())).await;
    let rutas = esc.rutas().clone();
    esc.abrir_sesion(esc.hosts[0]).await;
    esc.escuchar(Duration::from_millis(3000)).await;
    for programa in dobles::ORDENES {
        assert_eq!(banco.llamadas(programa), 0, "{programa}");
    }
    assert!(
        !dir_estado(&rutas).exists(),
        "el escritorio no crea el estado de Termux"
    );
    parar(&mut esc).await;
}

// ====================================================================
// La barra: lo que el servidor le dice al cliente
// ====================================================================

#[tokio::test]
async fn el_servidor_escribe_su_arranque_sin_avisos_y_lo_quita_al_terminar() {
    let banco = Banco::nuevo();
    let antes = ahora();
    let mut servidor = Suelto::arrancar(Some(banco.termux("35")), configuracion(), |rutas| {
        // Lo que dejó un servidor anterior no se hereda.
        estado::escribir_servidor(
            rutas,
            &estado::EstadoServidorTermux {
                arranque: 1,
                avisos: vec![AvisoServidor::ApiApagada],
            },
        )
        .unwrap();
    })
    .await;
    let leido = estado::leer_servidor(servidor.rutas()).expect("el servidor escribió su estado");
    assert!(leido.arranque >= antes, "es el arranque de este servidor");
    assert!(leido.avisos.is_empty(), "sin avisos al arrancar");
    let modo = std::fs::metadata(dir_estado(servidor.rutas()).join("servidor"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(modo & 0o777, 0o600);
    servidor.parar().await;
    assert_eq!(estado::leer_servidor(servidor.rutas()), None);
}

// ====================================================================
// Avisos de Android: sesiones persistentes (vigilante)
// ====================================================================

/// AC: una sesión persistente toca la campana con la ventana cerrada → un
/// aviso «◆ agente-api pide atención» con «prueba-1 · MAGI» y nada más; se
/// retira al quitarse la atención y al terminar el servidor. El vigilante
/// corre sin ventanas.
#[tokio::test]
async fn una_ficha_con_campana_publica_su_aviso_sin_ventanas_y_se_retira() {
    let banco = Banco::nuevo();
    let mut config = configuracion();
    config.persistentes.vigilar_ms_pruebas = 150;
    let mut esc = escenario(opciones(&banco, Some(banco.termux("35")), config)).await;
    let rutas = esc.rutas().clone();
    let host = esc.hosts[0];
    let uuid = crear_ficha(&esc, host, "agente-api");
    let id = id_de(&uuid);
    // Sin campana: el vigilante la ve viva y no avisa.
    banco.magi(1, &[linea("agente-api", false, "TITULO-REMOTO")]);
    assert!(
        esc.hasta(PLAZO, |_| banco.comprobaciones(1) >= 2).await,
        "el vigilante no comprobó el host"
    );
    assert_eq!(banco.llamadas("termux-notification"), 0);

    // La ventana se cierra: nadie conectado.
    irse(&mut esc).await;
    let comprobadas = banco.comprobaciones(1);

    // Suena la campana en el host.
    banco.magi(1, &[linea("agente-api", true, "TITULO-REMOTO")]);
    assert!(
        esperar_que(PLAZO, || banco.llamadas("termux-notification") == 1).await,
        "no hubo aviso de Android con la ventana cerrada"
    );
    // Una comprobación que ya estaba en marcha al salir la ventana pudo
    // traer la campana: lo que prueba el vigilante sin ventanas es que sigue
    // empezando comprobaciones nuevas después de irse.
    assert!(
        esperar_que(PLAZO, || banco.comprobaciones(1) > comprobadas).await,
        "el vigilante no trabajó sin ventanas"
    );
    let publicado = &banco.dobles.llamadas("termux-notification")[0];
    assert_eq!(
        publicado.args,
        args_de_aviso(&id, "◆ agente-api pide atención", "prueba-1 · MAGI")
    );
    assert!(publicado.entrada.is_empty());
    // Nada más: ni el título remoto, ni el comando, ni la nota de la ficha.
    let todo = publicado.args.join("\n");
    for secreto in ["TITULO-REMOTO", "secreto-de-la-prueba", "nota-privada"] {
        assert!(!todo.contains(secreto), "el aviso lleva «{secreto}»");
    }
    assert_eq!(
        estado::leer_avisos(&rutas),
        [id.clone()].into(),
        "el id queda anotado para retirarlo si el servidor muere"
    );
    // Misma atención en la siguiente comprobación: no se vuelve a publicar.
    tokio::time::sleep(Duration::from_millis(900)).await;
    assert_eq!(banco.llamadas("termux-notification"), 1);

    // Se quita la atención → se retira.
    banco.magi(1, &[linea("agente-api", false, "TITULO-REMOTO")]);
    assert!(
        esperar_que(PLAZO, || banco.llamadas("termux-notification-remove") == 1).await,
        "no se retiró el aviso al quitarse la atención"
    );
    assert_eq!(
        banco.dobles.llamadas("termux-notification-remove")[0].args,
        vec![id.clone()]
    );
    assert!(
        esperar_que(PLAZO, || estado::leer_avisos(&rutas).is_empty()).await,
        "el id sigue en el estado"
    );
    assert!(!dir_estado(&rutas).join("avisos").exists());

    // Vuelve a sonar y el servidor termina con el aviso publicado: lo retira.
    banco.magi(1, &[linea("agente-api", true, "TITULO-REMOTO")]);
    assert!(esperar_que(PLAZO, || banco.llamadas("termux-notification") == 2).await);
    parar_desde_otra_ventana(&mut esc).await;
    assert_eq!(
        banco.llamadas("termux-notification-remove"),
        2,
        "no retiró el aviso al terminar"
    );
    assert_eq!(
        banco.dobles.llamadas("termux-notification-remove")[1].args,
        vec![id]
    );
    assert!(!dir_estado(&rutas).join("avisos").exists());
}

/// Borrar la ficha (la borra el cliente de la tabla; el servidor solo la
/// lee) retira su aviso aunque la sesión siga con la campana en el host.
#[tokio::test]
async fn al_borrar_la_ficha_se_retira_su_aviso() {
    let banco = Banco::nuevo();
    let mut config = configuracion();
    config.persistentes.vigilar_ms_pruebas = 150;
    let mut esc = escenario(opciones(&banco, Some(banco.termux("35")), config)).await;
    let rutas = esc.rutas().clone();
    let host = esc.hosts[0];
    let uuid = crear_ficha(&esc, host, "agente-api");
    banco.magi(1, &[linea("agente-api", true, "")]);
    irse(&mut esc).await;
    assert!(
        esperar_que(PLAZO, || banco.llamadas("termux-notification") == 1).await,
        "no hubo aviso"
    );
    assert_eq!(estado::leer_avisos(&rutas), [id_de(&uuid)].into());

    {
        let almacen = magi::almacen::abrir(&rutas.base_datos()).unwrap();
        let ficha =
            magi_nucleo::almacen::sesiones_persistentes::por_uuid(almacen.conexion(), &uuid)
                .unwrap()
                .expect("la ficha está");
        magi_nucleo::almacen::sesiones_persistentes::borrar(almacen.conexion(), ficha.id).unwrap();
        almacen.cerrar().unwrap();
    }
    assert!(
        esperar_que(PLAZO, || banco.llamadas("termux-notification-remove") == 1).await,
        "no se retiró el aviso de la ficha borrada"
    );
    assert_eq!(
        banco.dobles.llamadas("termux-notification-remove")[0].args,
        vec![id_de(&uuid)]
    );
    assert!(esperar_que(PLAZO, || estado::leer_avisos(&rutas).is_empty()).await);
    // La campana sigue sonando en el host, pero ya no hay ficha: no vuelve.
    tokio::time::sleep(Duration::from_millis(900)).await;
    assert_eq!(banco.llamadas("termux-notification"), 1);
    parar_desde_otra_ventana(&mut esc).await;
    assert_eq!(banco.llamadas("termux-notification-remove"), 1);
}

/// El caso del AC tal cual: la ventana de MAGI cerrada y la pestaña de la
/// sesión viva en el servidor (que es lo que lo mantiene en marcha). El aviso
/// es uno solo: el de la ficha.
#[tokio::test]
async fn con_la_pestana_de_la_sesion_abierta_y_sin_ventanas_el_aviso_es_uno_solo() {
    let banco = Banco::nuevo();
    let mut config = configuracion();
    config.persistentes.vigilar_ms_pruebas = 150;
    let mut esc = escenario(opciones(&banco, Some(banco.termux("35")), config)).await;
    let host = esc.hosts[0];
    let uuid = crear_ficha(&esc, host, "agente-api");
    banco.magi(1, &[linea("agente-api", false, "")]);
    // La pestaña del servidor, enganchada a la sesión persistente.
    esc.enviar(&MensajeCliente::AbrirSesion {
        comandos_iniciales: Vec::new(),
        host_id: host,
        cols: 80,
        filas: 24,
        orden: None,
        tmux: Some(PeticionTmux {
            servidor: ServidorTmux::Magi,
            nombre: "agente-api".to_string(),
            crear: None,
            compartir: true,
        }),
    })
    .await;
    esc.esperar_sesion_nueva(&HashSet::new()).await;
    assert!(esc.hasta(PLAZO, |_| banco.comprobaciones(1) >= 2).await);
    irse(&mut esc).await;

    banco.magi(1, &[linea("agente-api", true, "")]);
    assert!(
        esperar_que(PLAZO, || banco.llamadas("termux-notification") >= 1).await,
        "no hubo aviso con la ventana cerrada y la pestaña viva"
    );
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let publicadas = banco.dobles.llamadas("termux-notification");
    assert_eq!(publicadas.len(), 1, "un solo aviso para la sesión");
    assert_eq!(
        publicadas[0].args,
        args_de_aviso(
            &id_de(&uuid),
            "◆ agente-api pide atención",
            "prueba-1 · MAGI"
        )
    );
    assert_eq!(banco.llamadas("termux-notification-remove"), 0);
    parar_desde_otra_ventana(&mut esc).await;
    assert_eq!(banco.llamadas("termux-notification-remove"), 1);
}

/// Con `[termux] avisos = false` no se publica nada y el vigilante no trabaja
/// sin ventanas: la barra sigue como en el escritorio.
#[tokio::test]
async fn con_avisos_false_no_se_publica_y_el_vigilante_espera_a_las_ventanas() {
    let banco = Banco::nuevo();
    let mut config = configuracion();
    config.termux.avisos = false;
    config.persistentes.vigilar_ms_pruebas = 150;
    let mut esc = escenario(opciones(&banco, Some(banco.termux("35")), config)).await;
    let host = esc.hosts[0];
    crear_ficha(&esc, host, "agente-api");
    banco.magi(1, &[linea("agente-api", true, "")]);
    assert!(
        esc.hasta(PLAZO, |_| banco.comprobaciones(1) >= 2).await,
        "con ventana, el vigilante trabaja como siempre"
    );
    irse(&mut esc).await;
    let al_irse = banco.comprobaciones(1);
    tokio::time::sleep(Duration::from_millis(1800)).await;
    assert!(
        banco.comprobaciones(1) <= al_irse + 1,
        "sin ventanas y sin avisos, el vigilante no debe trabajar ({} → {})",
        al_irse,
        banco.comprobaciones(1)
    );
    assert_eq!(banco.llamadas("termux-notification"), 0);
    parar_desde_otra_ventana(&mut esc).await;
    assert_eq!(banco.llamadas("termux-notification-remove"), 0);
}

/// A1: sin ventanas, una pestaña caída que nadie ha cerrado es lo único que
/// mantiene vivo al servidor: el vigilante no sondea hosts por SSH para nadie.
/// En cuanto hay otra ventana, vuelve a trabajar.
#[tokio::test]
async fn con_solo_una_pestana_caida_y_sin_ventanas_el_vigilante_no_sondea() {
    let banco = Banco::nuevo();
    let mut config = configuracion();
    config.persistentes.vigilar_ms_pruebas = 150;
    let mut opciones = opciones(&banco, Some(banco.termux("35")), config);
    opciones.reconectar = Some(false);
    let mut esc = escenario(opciones).await;
    let host = esc.hosts[0];
    crear_ficha(&esc, host, "agente-api");
    banco.magi(1, &[linea("agente-api", false, "")]);
    assert!(
        esc.hasta(PLAZO, |_| banco.comprobaciones(1) >= 2).await,
        "con ventana, el vigilante trabaja como siempre"
    );
    pestana_caida(&mut esc).await;

    // La ventana se va: solo queda la pestaña caída.
    irse(&mut esc).await;
    let al_irse = banco.comprobaciones(1);
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert!(
        banco.comprobaciones(1) <= al_irse + 1,
        "con solo una pestaña caída el vigilante no debe sondear ({} → {})",
        al_irse,
        banco.comprobaciones(1)
    );
    // Ni siquiera la campana de la ficha se avisa: nadie lo pide.
    banco.magi(1, &[linea("agente-api", true, "")]);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(banco.llamadas("termux-notification"), 0);

    // Una ventana nueva lo despierta.
    let mut otra = esc.otra_ventana().await;
    assert!(
        esperar_que(PLAZO, || banco.comprobaciones(1) > al_irse + 1).await,
        "con una ventana el vigilante no volvió a trabajar"
    );
    otra.enviar(&MensajeCliente::Parar).await;
    let fin = tokio::time::timeout(Duration::from_secs(30), &mut esc.tarea_magi).await;
    assert!(fin.is_ok(), "el servidor no terminó en 30 s");
}

/// El servidor del escritorio sigue sin vigilar sin ventanas.
#[tokio::test]
async fn el_servidor_del_escritorio_no_vigila_sin_ventanas() {
    let banco = Banco::nuevo();
    let mut config = configuracion();
    config.persistentes.vigilar_ms_pruebas = 150;
    let mut esc = escenario(opciones(&banco, None, config)).await;
    let host = esc.hosts[0];
    crear_ficha(&esc, host, "agente-api");
    banco.magi(1, &[linea("agente-api", true, "")]);
    assert!(esc.hasta(PLAZO, |_| banco.comprobaciones(1) >= 2).await);
    irse(&mut esc).await;
    let al_irse = banco.comprobaciones(1);
    tokio::time::sleep(Duration::from_millis(1800)).await;
    assert!(
        banco.comprobaciones(1) <= al_irse + 1,
        "el escritorio no vigila sin ventanas ({} → {})",
        al_irse,
        banco.comprobaciones(1)
    );
    parar_desde_otra_ventana(&mut esc).await;
}

/// Un Termux:API que falla apaga los avisos en este servidor: un solo intento,
/// el aviso en la barra y, sin ventanas, el vigilante deja de trabajar (ya no
/// puede avisar de nada).
#[tokio::test]
async fn si_termux_api_falla_se_apagan_los_avisos_y_lo_dice_la_barra() {
    let banco = Banco::nuevo();
    banco.dobles.programar("termux-notification", Modo::Fallar);
    let mut config = configuracion();
    config.persistentes.vigilar_ms_pruebas = 150;
    let mut esc = escenario(opciones(&banco, Some(banco.termux("35")), config)).await;
    let rutas = esc.rutas().clone();
    let host = esc.hosts[0];
    let uuid = crear_ficha(&esc, host, "agente-api");
    banco.magi(1, &[linea("agente-api", false, "")]);
    assert!(esc.hasta(PLAZO, |_| banco.comprobaciones(1) >= 2).await);
    irse(&mut esc).await;
    banco.magi(1, &[linea("agente-api", true, "")]);
    assert!(
        esperar_que(PLAZO, || barra(&rutas).contains(&AvisoServidor::ApiApagada)).await,
        "la barra no dice que Termux:API está apagada: {:?}",
        barra(&rutas)
    );
    assert_eq!(banco.llamadas("termux-notification"), 1, "un solo intento");
    assert!(
        estado::leer_avisos(&rutas).is_empty(),
        "no quedó anotado un aviso que no se publicó ({})",
        id_de(&uuid)
    );
    // Sin más órdenes de la API, y el vigilante se calla sin ventanas.
    tokio::time::sleep(Duration::from_millis(900)).await;
    let reposo = banco.comprobaciones(1);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(banco.llamadas("termux-notification"), 1);
    assert!(
        banco.comprobaciones(1) <= reposo + 1,
        "el vigilante sigue trabajando sin poder avisar"
    );
    parar_desde_otra_ventana(&mut esc).await;
    assert_eq!(banco.llamadas("termux-notification-remove"), 0);
}

/// Los avisos que dejó un servidor muerto (ids del estado) se retiran al
/// arrancar el siguiente; lo que no es un id de MAGI se ignora.
#[tokio::test]
async fn al_arrancar_retira_los_avisos_de_un_servidor_anterior() {
    let banco = Banco::nuevo();
    let mut servidor = Suelto::arrancar(Some(banco.termux("35")), configuracion(), |rutas| {
        let dir = estado::directorio(rutas);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("avisos"),
            "magi-7c1e4a90\nmagi-p7\n$(reboot)\nmagi-a;b\n../../x\n",
        )
        .unwrap();
    })
    .await;
    assert!(
        esperar_que(PLAZO, || banco.llamadas("termux-notification-remove") == 2).await,
        "no retiró los avisos del servidor anterior"
    );
    let mut ids: Vec<String> = banco
        .dobles
        .llamadas("termux-notification-remove")
        .iter()
        .map(|llamada| llamada.args.join(" "))
        .collect();
    ids.sort();
    assert_eq!(ids, ["magi-7c1e4a90", "magi-p7"], "solo ids válidos");
    assert!(
        esperar_que(PLAZO, || !dir_estado(servidor.rutas())
            .join("avisos")
            .exists())
        .await,
        "el estado sigue con ids retirados"
    );
    assert_eq!(banco.llamadas("termux-notification"), 0);
    servidor.parar().await;
}

/// Con Termux:API sin responder al arrancar, los avisos de antes se quedan en
/// el estado (los retirará otro servidor) y la barra lo dice.
#[tokio::test]
async fn si_no_responde_la_api_al_retirar_los_de_antes_quedan_en_el_estado() {
    let banco = Banco::nuevo();
    banco
        .dobles
        .programar("termux-notification-remove", Modo::Fallar);
    let mut servidor = Suelto::arrancar(Some(banco.termux("35")), configuracion(), |rutas| {
        let dir = estado::directorio(rutas);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("avisos"), "magi-7c1e4a90\nmagi-p7\n").unwrap();
    })
    .await;
    assert!(
        esperar_que(PLAZO, || barra(servidor.rutas())
            .contains(&AvisoServidor::ApiApagada))
        .await,
        "la barra no lo dice: {:?}",
        barra(servidor.rutas())
    );
    assert_eq!(
        banco.llamadas("termux-notification-remove"),
        1,
        "apagada la API no se insiste con el resto"
    );
    assert_eq!(
        estado::leer_avisos(servidor.rutas()),
        ["magi-7c1e4a90".to_string(), "magi-p7".to_string()].into(),
        "siguen anotados para el siguiente servidor"
    );
    servidor.parar().await;
}

// ====================================================================
// Avisos de Android: pestañas
// ====================================================================

/// Una pestaña cuyo remoto toca la campana avisa aunque esté delante (el foco
/// no es fiable, D379); el aviso se retira al escribir en la pestaña, al
/// enfocarla y al cerrarla. Lleva el nombre de la pestaña y el del host.
#[tokio::test]
async fn una_pestana_con_campana_avisa_y_se_retira_al_escribir_enfocar_o_cerrar() {
    let banco = Banco::nuevo();
    let mut opciones = opciones(&banco, Some(banco.termux("35")), configuracion());
    opciones.programa_shell = programa_con_campana();
    let mut esc = escenario(opciones).await;
    let rutas = esc.rutas().clone();
    let sesion = esc.abrir_sesion(esc.hosts[0]).await;
    adjuntar(&mut esc, sesion).await;
    let nombre = esc
        .sesiones
        .iter()
        .find(|s| s.id == sesion)
        .map(|s| s.nombre.clone())
        .expect("la pestaña está en la lista");
    let id = format!("magi-p{sesion}");
    let esperado = args_de_aviso(&id, &format!("! {nombre} pide atención"), "prueba-1 · MAGI");

    // Delante y con foco: con el foco poco fiable avisa igual.
    esc.enviar(&MensajeCliente::Foco {
        sesion_id: sesion,
        enfocada: true,
    })
    .await;
    esc.enviar(&MensajeCliente::Teclas {
        sesion_id: sesion,
        bytes: b"campana\n".to_vec(),
    })
    .await;
    assert!(
        esc.hasta(PLAZO, |_| banco.llamadas("termux-notification") == 1)
            .await,
        "la campana del remoto no dio aviso"
    );
    assert_eq!(
        banco.dobles.llamadas("termux-notification")[0].args,
        esperado
    );
    assert_eq!(estado::leer_avisos(&rutas), [id.clone()].into());

    // Escribir en la pestaña la atiende: se retira.
    esc.enviar(&MensajeCliente::Teclas {
        sesion_id: sesion,
        bytes: b"x".to_vec(),
    })
    .await;
    assert!(
        esc.hasta(PLAZO, |_| banco.llamadas("termux-notification-remove") == 1)
            .await,
        "no se retiró al escribir en la pestaña"
    );
    assert_eq!(
        banco.dobles.llamadas("termux-notification-remove")[0].args,
        vec![id.clone()]
    );
    assert!(estado::leer_avisos(&rutas).is_empty());

    // Otra campana: avisa de nuevo; enfocarla de nuevo la atiende.
    esc.enviar(&MensajeCliente::Teclas {
        sesion_id: sesion,
        bytes: b"campana\n".to_vec(),
    })
    .await;
    assert!(
        esc.hasta(PLAZO, |_| banco.llamadas("termux-notification") == 2)
            .await
    );
    esc.enviar(&MensajeCliente::Foco {
        sesion_id: sesion,
        enfocada: false,
    })
    .await;
    esc.escuchar(Duration::from_millis(500)).await;
    assert_eq!(
        banco.llamadas("termux-notification-remove"),
        1,
        "perder el foco no la atiende"
    );
    esc.enviar(&MensajeCliente::Foco {
        sesion_id: sesion,
        enfocada: true,
    })
    .await;
    assert!(
        esc.hasta(PLAZO, |_| banco.llamadas("termux-notification-remove") == 2)
            .await,
        "no se retiró al enfocar la pestaña"
    );

    // Otra campana y se cierra la pestaña: el aviso se retira.
    esc.enviar(&MensajeCliente::Teclas {
        sesion_id: sesion,
        bytes: b"campana\n".to_vec(),
    })
    .await;
    assert!(
        esc.hasta(PLAZO, |_| banco.llamadas("termux-notification") == 3)
            .await
    );
    esc.enviar(&MensajeCliente::Cerrar { sesion_id: sesion })
        .await;
    assert!(
        esc.hasta(PLAZO, |_| banco.llamadas("termux-notification-remove") == 3)
            .await,
        "no se retiró al cerrar la pestaña"
    );
    assert!(estado::leer_avisos(&rutas).is_empty());
    parar(&mut esc).await;
    assert_eq!(banco.llamadas("termux-notification-remove"), 3);
}

/// Una pestaña cuyo programa suelta, según la línea que recibe, el progreso de
/// ConEmu (`OSC 9;4;1;50`) o un aviso de verdad (`OSC 9;3 tests fallan`).
fn programa_con_osc_9() -> Option<Vec<String>> {
    Some(vec![
        "sh".to_string(),
        "-c".to_string(),
        r"while IFS= read -r linea; do case $linea in progreso) printf '\033]9;4;1;50\007';; tests) printf '\033]9;3 tests fallan\007';; esac; done".to_string(),
    ])
}

/// A2: el progreso de ConEmu (`OSC 9;4;estado;porcentaje`) llega al emulador
/// como un aviso («4 · 1 · 50») y se sigue enseñando en la pestaña, pero no
/// pide atención ni publica un aviso de Android; un aviso real (`OSC 9;3 tests
/// fallan`, que empieza por un dígito) sí.
#[tokio::test]
async fn el_progreso_de_conemu_no_publica_un_aviso_de_android_pero_un_aviso_real_si() {
    let banco = Banco::nuevo();
    let mut opciones = opciones(&banco, Some(banco.termux("35")), configuracion());
    opciones.programa_shell = programa_con_osc_9();
    let mut esc = escenario(opciones).await;
    let rutas = esc.rutas().clone();
    let sesion = esc.abrir_sesion(esc.hosts[0]).await;
    adjuntar(&mut esc, sesion).await;

    esc.enviar(&MensajeCliente::Teclas {
        sesion_id: sesion,
        bytes: b"progreso\n".to_vec(),
    })
    .await;
    assert!(
        esc.hasta(PLAZO, |e| e.sesiones.iter().any(|s| s.id == sesion
            && s.aviso
                .as_ref()
                .is_some_and(|aviso| aviso.texto == "4 · 1 · 50")))
            .await,
        "el emulador no guardó el progreso como aviso: {:?}",
        esc.sesiones
    );
    esc.escuchar(Duration::from_millis(1500)).await;
    assert_eq!(
        banco.llamadas("termux-notification"),
        0,
        "el progreso de ConEmu no pide atención a nadie"
    );
    assert!(estado::leer_avisos(&rutas).is_empty());

    // Un aviso real, aunque empiece por un dígito, sí.
    esc.enviar(&MensajeCliente::Teclas {
        sesion_id: sesion,
        bytes: b"tests\n".to_vec(),
    })
    .await;
    assert!(
        esc.hasta(PLAZO, |_| banco.llamadas("termux-notification") == 1)
            .await,
        "«3 tests fallan» no dio aviso de Android"
    );
    let id = format!("magi-p{sesion}");
    assert_eq!(estado::leer_avisos(&rutas), [id].into());
    parar(&mut esc).await;
}

/// Un aviso que el propio servidor guarda en la pestaña (aquí, tmux antiguo en
/// un host marcado) no es una atención del remoto y no publica nada.
#[tokio::test]
async fn un_aviso_interno_del_servidor_no_publica_nada_en_android() {
    let banco = Banco::nuevo();
    let mut opciones = opciones(&banco, Some(banco.termux("35")), configuracion());
    opciones.entorno_extra = banco.entorno_del_host(&[("MAGI_TMUX_VERSION", "tmux 2.9a")]);
    let mut esc = escenario(opciones).await;
    let host = esc.hosts[0];
    {
        let almacen = magi::almacen::abrir(&esc.rutas().base_datos()).unwrap();
        magi::almacen::hosts::fijar_sesiones_persistentes(almacen.conexion(), host, true).unwrap();
        almacen.cerrar().unwrap();
    }
    esc.abrir_sesion(host).await;
    assert!(
        esc.hasta(PLAZO, |e| e.sesiones.iter().any(|s| s
            .aviso
            .as_ref()
            .is_some_and(|aviso| aviso.texto.contains("antiguo"))))
            .await,
        "el servidor no avisó en la pestaña del tmux antiguo: {:?}",
        esc.sesiones
    );
    esc.escuchar(Duration::from_millis(1500)).await;
    assert_eq!(
        banco.llamadas("termux-notification"),
        0,
        "un aviso interno no es una atención del remoto"
    );
}

/// Con `avisos = false` una pestaña con campana no publica nada.
#[tokio::test]
async fn con_avisos_false_una_pestana_con_campana_no_publica() {
    let banco = Banco::nuevo();
    let mut config = configuracion();
    config.termux.avisos = false;
    let mut opciones = opciones(&banco, Some(banco.termux("35")), config);
    opciones.programa_shell = programa_con_campana();
    let mut esc = escenario(opciones).await;
    let sesion = esc.abrir_sesion(esc.hosts[0]).await;
    adjuntar(&mut esc, sesion).await;
    esc.enviar(&MensajeCliente::Teclas {
        sesion_id: sesion,
        bytes: b"campana\n".to_vec(),
    })
    .await;
    esc.escuchar(Duration::from_millis(2500)).await;
    assert_eq!(banco.llamadas("termux-notification"), 0);
    parar(&mut esc).await;
    assert_eq!(banco.llamadas("termux-notification-remove"), 0);
}

// ====================================================================
// magi termux aviso
// ====================================================================

fn ordenes_de(banco: &Banco, plazo: Duration) -> magi::termux::Ordenes {
    magi::termux::Ordenes::nuevas(
        &banco.dobles.entorno(&banco.base.path().join("home")),
        plazo,
        std::sync::Arc::new(magi::termux::ApiProceso::default()),
    )
}

#[test]
fn el_aviso_de_prueba_se_publica_y_se_retira() {
    let banco = Banco::nuevo();
    let ordenes = ordenes_de(&banco, Duration::from_secs(5));
    let (mut salida, mut error) = (Vec::new(), Vec::new());
    let codigo = magi::termux::cli::aviso_de_prueba(
        &ordenes,
        Duration::from_millis(300),
        &mut salida,
        &mut error,
    );
    assert_eq!(codigo, 0, "{}", String::from_utf8_lossy(&error));
    let publicadas = banco.dobles.llamadas("termux-notification");
    assert_eq!(publicadas.len(), 1);
    assert_eq!(
        publicadas[0].args,
        args_de_aviso("magi-test", "◆ prueba pide atención", "MAGI · prueba")
    );
    let retiradas = banco.dobles.llamadas("termux-notification-remove");
    assert_eq!(retiradas.len(), 1);
    assert_eq!(retiradas[0].args, vec!["magi-test".to_string()]);
    let salida = String::from_utf8_lossy(&salida);
    assert!(salida.contains("publicado"), "{salida}");
    assert!(salida.contains("retirado"), "{salida}");
    assert!(error.is_empty());
}

#[test]
fn el_aviso_de_prueba_dice_por_que_si_termux_api_falla() {
    let banco = Banco::nuevo();
    banco.dobles.programar("termux-notification", Modo::Fallar);
    let ordenes = ordenes_de(&banco, Duration::from_secs(5));
    let (mut salida, mut error) = (Vec::new(), Vec::new());
    let codigo = magi::termux::cli::aviso_de_prueba(
        &ordenes,
        Duration::from_millis(100),
        &mut salida,
        &mut error,
    );
    assert_eq!(codigo, 1);
    let error = String::from_utf8_lossy(&error);
    assert!(error.contains("Termux:API"), "{error}");
    assert!(error.contains("batería"), "dice qué hacer: {error}");
    assert_eq!(
        banco.llamadas("termux-notification-remove"),
        0,
        "no retira lo que no publicó"
    );
    assert!(salida.is_empty());
}

#[test]
fn el_aviso_de_prueba_dice_que_falta_el_paquete_si_no_esta_la_orden() {
    let banco = Banco::nuevo();
    banco.dobles.quitar("termux-notification");
    let ordenes = ordenes_de(&banco, Duration::from_secs(5));
    let (mut salida, mut error) = (Vec::new(), Vec::new());
    let codigo = magi::termux::cli::aviso_de_prueba(
        &ordenes,
        Duration::from_millis(100),
        &mut salida,
        &mut error,
    );
    assert_eq!(codigo, 1);
    let error = String::from_utf8_lossy(&error);
    assert!(error.contains("termux-notification"), "{error}");
    assert!(error.contains("pkg install termux-api"), "{error}");
}

#[test]
fn el_aviso_de_prueba_con_la_api_colgada_no_espera_mas_que_el_plazo() {
    let banco = Banco::nuevo();
    banco.dobles.programar("termux-notification", Modo::Colgar);
    let ordenes = ordenes_de(&banco, Duration::from_secs(2));
    let (mut salida, mut error) = (Vec::new(), Vec::new());
    let inicio = std::time::Instant::now();
    let codigo = magi::termux::cli::aviso_de_prueba(
        &ordenes,
        Duration::from_millis(100),
        &mut salida,
        &mut error,
    );
    assert_eq!(codigo, 1);
    assert!(inicio.elapsed() < Duration::from_secs(6));
    assert!(String::from_utf8_lossy(&error).contains("Termux:API"));
}

/// El subcomando de verdad, con el binario: en un Termux de mentira publica y
/// retira el aviso de prueba (tarda los 10 s de la espera real).
#[cfg(debug_assertions)]
#[test]
fn magi_termux_aviso_publica_y_retira_el_aviso_de_prueba() {
    let banco = Banco::nuevo();
    let casa = banco.base.path().join("home");
    std::fs::create_dir_all(&casa).unwrap();
    let inicio = std::time::Instant::now();
    let salida = std::process::Command::new(env!("CARGO_BIN_EXE_magi"))
        .args(["termux", "aviso"])
        .env_clear()
        .env("MAGI_PLATAFORMA", "termux")
        .env("HOME", &casa)
        .env("PREFIX", banco.dobles.prefix())
        .env("TMPDIR", banco.dobles.prefix().join("tmp"))
        .env("PATH", banco.dobles.path())
        .env("XDG_DATA_HOME", casa.join("datos"))
        .env("XDG_CONFIG_HOME", casa.join("config"))
        .env("XDG_STATE_HOME", casa.join("estado"))
        .env("TZ", "UTC")
        .output()
        .expect("lanzando magi");
    let texto = String::from_utf8_lossy(&salida.stdout);
    assert!(
        salida.status.success(),
        "{texto}\n{}",
        String::from_utf8_lossy(&salida.stderr)
    );
    assert!(
        inicio.elapsed() >= Duration::from_secs(9),
        "el aviso de prueba dura 10 s"
    );
    assert!(texto.contains("publicado"), "{texto}");
    let publicadas = banco.dobles.llamadas("termux-notification");
    assert_eq!(publicadas.len(), 1);
    assert_eq!(
        publicadas[0].args,
        args_de_aviso("magi-test", "◆ prueba pide atención", "MAGI · prueba")
    );
    assert_eq!(
        banco.dobles.llamadas("termux-notification-remove")[0].args,
        vec!["magi-test".to_string()]
    );
}

/// Fuera de Termux sigue diciendo «solo en Termux» con código 1.
#[test]
fn fuera_de_termux_magi_termux_aviso_dice_solo_en_termux() {
    let banco = Banco::nuevo();
    let casa = banco.base.path().join("home");
    std::fs::create_dir_all(&casa).unwrap();
    let salida = std::process::Command::new(env!("CARGO_BIN_EXE_magi"))
        .args(["termux", "aviso"])
        .env_clear()
        .env("HOME", &casa)
        .env("PATH", banco.dobles.path())
        .env("XDG_DATA_HOME", casa.join("datos"))
        .env("XDG_CONFIG_HOME", casa.join("config"))
        .env("XDG_STATE_HOME", casa.join("estado"))
        .env("XDG_RUNTIME_DIR", casa.join("run"))
        .output()
        .expect("lanzando magi");
    assert_eq!(salida.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&salida.stderr).contains("solo en Termux"));
    assert_eq!(banco.llamadas("termux-notification"), 0);
}

// ====================================================================
// Correcciones de la revisión adversarial del Sprint 2 (pista R1)
// ====================================================================

/// Los instantes en que empezó cada llamada a `programa`: la creación del
/// directorio de la llamada en el estado del doble (los dobles de `Banco`
/// viven en `<base>/termux`).
fn inicios_de(banco: &Banco, programa: &str) -> Vec<std::time::SystemTime> {
    let llamadas = banco.base.path().join("termux/estado/llamadas");
    let prefijo = format!("{programa}.");
    let mut inicios: Vec<std::time::SystemTime> = std::fs::read_dir(llamadas)
        .expect("llamadas de los dobles")
        .filter_map(Result::ok)
        .filter(|entrada| {
            entrada
                .file_name()
                .to_string_lossy()
                .strip_prefix(&prefijo)
                .is_some_and(|numero| numero.parse::<u64>().is_ok())
        })
        .filter_map(|entrada| entrada.metadata().ok()?.modified().ok())
        .collect();
    inicios.sort();
    inicios
}

/// Cuántas llamadas estuvieron en marcha a la vez como mínimo, sabiendo que
/// cada una dura al menos `duracion`: las que empiezan con menos de
/// `duracion` de diferencia coinciden seguro. Nunca da de más.
fn solapamiento_maximo(inicios: &[std::time::SystemTime], duracion: Duration) -> usize {
    inicios
        .iter()
        .enumerate()
        .map(|(i, inicio)| {
            inicios[i..]
                .iter()
                .take_while(|otro| otro.duration_since(*inicio).unwrap_or_default() < duracion)
                .count()
        })
        .max()
        .unwrap_or(0)
}

/// Arranca otro servidor en el escenario (el anterior ya terminó): mismo
/// directorio, mismo host SSH, otro proceso de servidor, y reconecta al
/// cliente del escenario. Es la forma de probar lo que depende de lo que dejó
/// un servidor anterior en el estado, que el escenario no deja preparar antes
/// de arrancar.
async fn reiniciar_servidor(esc: &mut Escenario, termux: OpcionesTermux, config: Config) {
    let socket = magi::servidor::ruta_socket(esc.rutas());
    esc.tarea_magi = tokio::spawn(magi::servidor::arrancar_con(
        esc.rutas().clone(),
        config,
        Some(termux),
    ));
    comun::esperar_socket(&socket).await;
    let mut stream = tokio::net::UnixStream::connect(&socket)
        .await
        .expect("conectando al servidor nuevo");
    comun::enviar(
        &mut stream,
        &MensajeCliente::Hola {
            version: VERSION_PROTOCOLO,
            pid: 42,
        },
    )
    .await;
    let (lectura, escritura) = stream.into_split();
    let mut lector = tokio_util::codec::FramedRead::new(lectura, magi::protocolo::codec());
    let bienvenida: MensajeServidor = comun::siguiente_framed(&mut lector).await;
    let MensajeServidor::Bienvenida { cliente_id, .. } = bienvenida else {
        panic!("se esperaba Bienvenida y llegó {bienvenida:?}");
    };
    esc.cliente_id = cliente_id;
    esc.escritura = escritura;
    esc.lector = lector;
    esc.sesiones.clear();
}

// ------------------------------------------------------- F1: wake lock

/// La marca de un servidor anterior no prueba que el wake lock exista (Android
/// pudo cerrar Termux, que es para lo que está la marca): con trabajo, el
/// servidor nuevo ejecuta `termux-wake-lock` igualmente, una sola vez, y lo
/// suelta con su gracia al quedar sin trabajo.
#[tokio::test]
async fn con_la_marca_de_un_servidor_anterior_y_trabajo_toma_el_wake_lock_una_vez() {
    let banco = Banco::nuevo();
    // El primer servidor es el del escritorio: no toca nada de Termux.
    let mut esc = escenario(opciones(&banco, None, configuracion())).await;
    parar(&mut esc).await;
    estado::poner_marca(esc.rutas(), Marca::WakeLock).unwrap();
    reiniciar_servidor(&mut esc, banco.termux("35"), configuracion()).await;
    let rutas = esc.rutas().clone();

    let sesion = esc.abrir_sesion(esc.hosts[0]).await;
    assert!(
        esc.hasta(PLAZO, |_| banco.llamadas("termux-wake-lock") >= 1)
            .await,
        "con la marca previa y trabajo no se tomó el wake lock"
    );
    // Con trabajo no se suelta ni se repite, aunque pase la gracia (1,5 s) con
    // creces.
    esc.escuchar(Duration::from_millis(3000)).await;
    assert_eq!(
        banco.llamadas("termux-wake-lock"),
        1,
        "exactamente un termux-wake-lock"
    );
    assert!(hay_marca(&rutas, Marca::WakeLock), "la marca sigue puesta");
    // Si el primer segundo del servidor, aún sin la sesión, soltó la marca de
    // antes, la sesión lo tomó de nuevo: un unlock como mucho, y nunca con el
    // trabajo vivo después de tomarlo.
    let soltadas_antes = banco.llamadas("termux-wake-unlock");
    assert!(soltadas_antes <= 1, "{soltadas_antes} unlock con trabajo");

    esc.enviar(&MensajeCliente::Cerrar { sesion_id: sesion })
        .await;
    assert!(
        esc.hasta(PLAZO, |_| banco.llamadas("termux-wake-unlock")
            == soltadas_antes + 1)
            .await,
        "no se soltó al quedar sin trabajo"
    );
    assert!(
        esc.hasta(PLAZO, |_| !hay_marca(&rutas, Marca::WakeLock))
            .await
    );
    parar(&mut esc).await;
}

/// El caso exacto, con el conductor del wake lock a mano (es lo que el
/// servidor alimenta cada segundo desde `revisar_inactividad`): con la marca
/// previa y trabajo desde el primer paso, una sola orden de tomar y ninguna de
/// soltar; sin trabajo, su gracia y se suelta.
#[tokio::test]
async fn el_conductor_con_la_marca_previa_y_trabajo_desde_el_primer_paso_no_suelta_nada() {
    use magi::termux::wake_lock::{FaseWakeLock, WakeLock};
    let banco = Banco::nuevo();
    let entorno = comun::entorno();
    estado::poner_marca(&entorno.rutas, Marca::WakeLock).unwrap();
    let conductor = WakeLock::nuevo(
        std::sync::Arc::new(ordenes_de(&banco, Duration::from_secs(5))),
        entorno.rutas.clone(),
        true,
        Duration::from_millis(600),
        Some(35),
        std::sync::Arc::new(|_| {}),
    );
    // Cada segundo (aquí, cada 40 ms) con trabajo.
    for _ in 0..25 {
        conductor.alimentar(true);
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    assert_eq!(banco.llamadas("termux-wake-lock"), 1, "una sola toma");
    assert_eq!(banco.llamadas("termux-wake-unlock"), 0, "nunca se suelta");
    assert_eq!(conductor.fase(), FaseWakeLock::Tomado);
    assert!(hay_marca(&entorno.rutas, Marca::WakeLock));
    // Sin trabajo, su gracia y se suelta; la marca se va.
    for _ in 0..40 {
        conductor.alimentar(false);
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    assert_eq!(banco.llamadas("termux-wake-unlock"), 1);
    assert_eq!(banco.llamadas("termux-wake-lock"), 1);
    assert!(!hay_marca(&entorno.rutas, Marca::WakeLock));
    assert_eq!(conductor.fase(), FaseWakeLock::Suelto);
}

/// Con la marca previa, si `termux-wake-lock` falla no se quita la marca: el
/// wake lock de antes puede seguir existiendo y se soltará sin trabajo.
#[tokio::test]
async fn si_falla_tomar_con_la_marca_previa_la_marca_se_queda() {
    use magi::termux::wake_lock::WakeLock;
    let banco = Banco::nuevo();
    banco.dobles.programar("termux-wake-lock", Modo::Fallar);
    let entorno = comun::entorno();
    estado::poner_marca(&entorno.rutas, Marca::WakeLock).unwrap();
    let dicho = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let conductor = WakeLock::nuevo(
        std::sync::Arc::new(ordenes_de(&banco, Duration::from_secs(5))),
        entorno.rutas.clone(),
        true,
        Duration::from_millis(500),
        Some(35),
        {
            let dicho = std::sync::Arc::clone(&dicho);
            std::sync::Arc::new(move |aviso| dicho.lock().unwrap().push(aviso))
        },
    );
    for _ in 0..15 {
        conductor.alimentar(true);
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    assert_eq!(banco.llamadas("termux-wake-lock"), 1, "un solo intento");
    assert_eq!(*dicho.lock().unwrap(), vec![AvisoServidor::SinWakeLock]);
    assert!(
        hay_marca(&entorno.rutas, Marca::WakeLock),
        "el de antes puede seguir tomado: la marca se queda"
    );
    // Sin trabajo se suelta el de antes.
    for _ in 0..40 {
        conductor.alimentar(false);
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    assert_eq!(banco.llamadas("termux-wake-unlock"), 1);
    assert!(!hay_marca(&entorno.rutas, Marca::WakeLock));
}

// -------------------------------------------- F2: sin termux-notification

/// Sin el paquete `termux-api` (no hay `termux-notification` en el `PATH`) el
/// servidor no puede avisar de nada: el vigilante no trabaja sin ventanas, como
/// en el escritorio, y la barra no dice nada nuevo.
#[tokio::test]
async fn sin_termux_notification_el_vigilante_no_trabaja_sin_ventanas() {
    let banco = Banco::nuevo();
    banco.dobles.quitar("termux-notification");
    let mut config = configuracion();
    config.persistentes.vigilar_ms_pruebas = 150;
    let mut esc = escenario(opciones(&banco, Some(banco.termux("35")), config)).await;
    let rutas = esc.rutas().clone();
    let host = esc.hosts[0];
    crear_ficha(&esc, host, "agente-api");
    banco.magi(1, &[linea("agente-api", false, "")]);
    assert!(
        esc.hasta(PLAZO, |_| banco.comprobaciones(1) >= 2).await,
        "con ventana, el vigilante trabaja como siempre"
    );
    irse(&mut esc).await;
    let al_irse = banco.comprobaciones(1);
    tokio::time::sleep(Duration::from_millis(1800)).await;
    assert!(
        banco.comprobaciones(1) <= al_irse + 1,
        "sin termux-notification no hay a quién avisar y el vigilante no debe \
         trabajar sin ventanas ({} → {})",
        al_irse,
        banco.comprobaciones(1)
    );
    assert_eq!(
        barra(&rutas),
        Vec::<AvisoServidor>::new(),
        "sin aviso nuevo en la barra"
    );
    parar_desde_otra_ventana(&mut esc).await;
}

/// Con `termux-notification` instalada y sin que nadie la haya ejecutado, el
/// vigilante sí sigue sin ventanas: lo comprueba sin lanzarla.
#[tokio::test]
async fn con_termux_notification_instalada_el_vigilante_sigue_sin_ventanas_sin_lanzarla() {
    let banco = Banco::nuevo();
    let mut config = configuracion();
    config.persistentes.vigilar_ms_pruebas = 150;
    let mut esc = escenario(opciones(&banco, Some(banco.termux("35")), config)).await;
    let host = esc.hosts[0];
    crear_ficha(&esc, host, "agente-api");
    banco.magi(1, &[linea("agente-api", false, "")]);
    assert!(esc.hasta(PLAZO, |_| banco.comprobaciones(1) >= 2).await);
    irse(&mut esc).await;
    let al_irse = banco.comprobaciones(1);
    assert!(
        esperar_que(PLAZO, || banco.comprobaciones(1) >= al_irse + 3).await,
        "con la orden instalada, el vigilante debe seguir sin ventanas"
    );
    assert_eq!(
        banco.llamadas("termux-notification"),
        0,
        "no se ejecutó para saber si estaba"
    );
    parar_desde_otra_ventana(&mut esc).await;
}

// ------------------------------------------ F3: plazo vencido al publicar

/// Si `termux-notification` vence su plazo, Termux:API pudo publicar el aviso
/// antes de que se matara el grupo de procesos: el id se queda anotado en el
/// estado (nadie retiraría un aviso real sin id) y el siguiente servidor lo
/// retira. Con la API apagada este servidor no lanza más órdenes.
#[tokio::test]
async fn un_aviso_cuya_orden_no_responde_queda_anotado_para_retirarlo() {
    let banco = Banco::nuevo();
    banco.dobles.programar("termux-notification", Modo::Colgar);
    let mut config = configuracion();
    config.persistentes.vigilar_ms_pruebas = 150;
    let mut esc = escenario(opciones(&banco, Some(banco.termux("35")), config)).await;
    let rutas = esc.rutas().clone();
    let host = esc.hosts[0];
    let uuid = crear_ficha(&esc, host, "agente-api");
    banco.magi(1, &[linea("agente-api", false, "")]);
    assert!(esc.hasta(PLAZO, |_| banco.comprobaciones(1) >= 2).await);
    irse(&mut esc).await;
    banco.magi(1, &[linea("agente-api", true, "")]);
    assert!(
        esperar_que(PLAZO, || barra(&rutas).contains(&AvisoServidor::ApiApagada)).await,
        "la barra no dice que Termux:API está apagada: {:?}",
        barra(&rutas)
    );
    assert_eq!(banco.llamadas("termux-notification"), 1, "un solo intento");
    assert_eq!(
        estado::leer_avisos(&rutas),
        [id_de(&uuid)].into(),
        "el aviso pudo publicarse: su id sigue anotado"
    );
    // El proceso colgado y lo que lanzó mueren con su grupo.
    for pid in &banco.dobles.llamadas("termux-notification")[0].pids {
        assert!(!dobles::sigue_vivo_tras(*pid, Duration::from_secs(3)));
    }
    // Al terminar, con la API apagada, no se insiste: el id sigue para el
    // siguiente servidor.
    parar_desde_otra_ventana(&mut esc).await;
    assert_eq!(banco.llamadas("termux-notification-remove"), 0);
    assert_eq!(estado::leer_avisos(&rutas), [id_de(&uuid)].into());
}

// ------------------------------------ F4: una campana no es un ciclo por lote

/// Una salida con muchas campanas seguidas despierta a la tarea de avisos en
/// la transición a «pide atención», no una vez por lote.
#[tokio::test]
async fn muchas_campanas_seguidas_despiertan_una_sola_vez_a_la_tarea_de_avisos() {
    let banco = Banco::nuevo();
    let termux = banco.termux("35");
    let despertares = std::sync::Arc::clone(&termux.despertares);
    let mut opciones = opciones(&banco, Some(termux), configuracion());
    // Al recibir una línea suelta 30 campanas, una cada 50 ms (cada una es un
    // lote de datos distinto), y se queda dormido.
    opciones.programa_shell = Some(vec![
        "sh".to_string(),
        "-c".to_string(),
        "read -r linea; i=0; while [ \"$i\" -lt 30 ]; do printf '\\007'; sleep 0.05; \
         i=$((i+1)); done; sleep 20"
            .to_string(),
    ]);
    let mut esc = escenario(opciones).await;
    let sesion = esc.abrir_sesion(esc.hosts[0]).await;
    adjuntar(&mut esc, sesion).await;
    esc.escuchar(Duration::from_millis(300)).await;
    let antes = despertares.load(std::sync::atomic::Ordering::Relaxed);

    esc.enviar(&MensajeCliente::Teclas {
        sesion_id: sesion,
        bytes: b"ya\n".to_vec(),
    })
    .await;
    assert!(
        esc.hasta(PLAZO, |_| banco.llamadas("termux-notification") == 1)
            .await,
        "la primera campana no dio aviso"
    );
    // Pasan las 30 campanas (1,5 s) y la difusión coalescida (1 s).
    esc.escuchar(Duration::from_millis(3000)).await;
    let despertados = despertares.load(std::sync::atomic::Ordering::Relaxed) - antes;
    assert!(
        (1..=4).contains(&despertados),
        "30 campanas seguidas despertaron {despertados} veces a la tarea de avisos"
    );
    assert_eq!(banco.llamadas("termux-notification"), 1, "un solo aviso");
    parar(&mut esc).await;
}

// -------------------------------------------------- F5: retirar en tandas

fn escribir_avisos(rutas: &Rutas, ids: &[String]) {
    let dir = estado::directorio(rutas);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("avisos"), ids.join("\n") + "\n").unwrap();
}

/// La purga de arranque retira con `RETIRADAS_A_LA_VEZ` órdenes a la vez como
/// mucho (cada una arranca un proceso de Android) y no deja ninguna sin
/// retirar.
#[tokio::test]
async fn la_purga_de_arranque_retira_con_como_mucho_cuatro_ordenes_a_la_vez() {
    use magi::servidor::android::RETIRADAS_A_LA_VEZ;
    let banco = Banco::nuevo();
    banco
        .dobles
        .programar("termux-notification-remove", Modo::Lento(6));
    let ids: Vec<String> = (1..=14).map(|n| format!("magi-p{n}")).collect();
    let mut servidor = Suelto::arrancar(Some(banco.termux("35")), configuracion(), |rutas| {
        escribir_avisos(rutas, &ids);
    })
    .await;
    assert!(
        esperar_que(Duration::from_secs(30), || {
            estado::leer_avisos(servidor.rutas()).is_empty()
                && banco.llamadas("termux-notification-remove") == 14
        })
        .await,
        "no se retiraron los 14 avisos: {} órdenes",
        banco.llamadas("termux-notification-remove")
    );
    let inicios = inicios_de(&banco, "termux-notification-remove");
    assert_eq!(inicios.len(), 14);
    let a_la_vez = solapamiento_maximo(&inicios, Duration::from_millis(550));
    assert!(
        a_la_vez <= RETIRADAS_A_LA_VEZ,
        "{a_la_vez} órdenes de retirada a la vez (tope {RETIRADAS_A_LA_VEZ})"
    );
    assert!(
        a_la_vez >= 2,
        "las retiradas no van en paralelo ({a_la_vez})"
    );
    // La primera va sola: sondea Termux:API.
    assert_eq!(
        solapamiento_maximo(&inicios[..2], Duration::from_millis(550)),
        1,
        "la primera orden debe ir sola"
    );
    servidor.parar().await;
}

/// Al terminar, el cierre retira todos los avisos con el mismo tope.
#[tokio::test]
async fn el_cierre_retira_los_avisos_con_como_mucho_cuatro_ordenes_a_la_vez() {
    use magi::servidor::android::RETIRADAS_A_LA_VEZ;
    let banco = Banco::nuevo();
    let mut config = configuracion();
    config.persistentes.vigilar_ms_pruebas = 150;
    let mut esc = escenario(opciones(&banco, Some(banco.termux("35")), config)).await;
    let rutas = esc.rutas().clone();
    let host = esc.hosts[0];
    let nombres: Vec<String> = (1..=7).map(|n| format!("agente-{n}")).collect();
    for nombre in &nombres {
        crear_ficha(&esc, host, nombre);
    }
    let lineas: Vec<String> = nombres.iter().map(|n| linea(n, true, "")).collect();
    banco.magi(1, &lineas);
    irse(&mut esc).await;
    assert!(
        esperar_que(PLAZO, || banco.llamadas("termux-notification") == 7).await,
        "no se publicaron los 7 avisos: {}",
        banco.llamadas("termux-notification")
    );
    assert_eq!(estado::leer_avisos(&rutas).len(), 7);
    // Retirar tarda: así se ve cuántas órdenes corren a la vez.
    banco
        .dobles
        .programar("termux-notification-remove", Modo::Lento(7));
    parar_desde_otra_ventana(&mut esc).await;
    assert_eq!(banco.llamadas("termux-notification-remove"), 7);
    assert!(estado::leer_avisos(&rutas).is_empty(), "quedaron ids");
    let inicios = inicios_de(&banco, "termux-notification-remove");
    let a_la_vez = solapamiento_maximo(&inicios, Duration::from_millis(650));
    assert!(
        a_la_vez <= RETIRADAS_A_LA_VEZ,
        "{a_la_vez} órdenes de retirada a la vez (tope {RETIRADAS_A_LA_VEZ})"
    );
    assert!(
        a_la_vez >= 2,
        "las retiradas no van en paralelo ({a_la_vez})"
    );
}

/// Si el servidor termina con la purga de arranque a medias, la purga no lanza
/// más órdenes: el turno es del cierre, que retira lo que falte en tandas.
#[tokio::test]
async fn al_terminar_con_la_purga_a_medias_la_purga_no_lanza_mas_y_lo_hace_el_cierre() {
    use magi::servidor::android::RETIRADAS_A_LA_VEZ;
    let banco = Banco::nuevo();
    banco
        .dobles
        .programar("termux-notification-remove", Modo::Lento(8));
    let ids: Vec<String> = (1..=9).map(|n| format!("magi-p{n}")).collect();
    let mut servidor = Suelto::arrancar(Some(banco.termux("35")), configuracion(), |rutas| {
        escribir_avisos(rutas, &ids);
    })
    .await;
    assert!(
        esperar_que(PLAZO, || banco.llamadas("termux-notification-remove") == 1).await,
        "la purga no empezó"
    );
    // La primera orden (0,8 s) sigue en marcha: el servidor termina ya.
    servidor.parar().await;
    assert_eq!(
        banco.llamadas("termux-notification-remove"),
        9,
        "cada aviso se retira una sola vez"
    );
    assert!(
        estado::leer_avisos(servidor.rutas()).is_empty(),
        "quedaron ids"
    );
    let inicios = inicios_de(&banco, "termux-notification-remove");
    // Si la purga hubiera seguido ella sola, iría de uno en uno; con el cierre
    // retirando el resto, se ven tandas de cuatro.
    let a_la_vez = solapamiento_maximo(&inicios, Duration::from_millis(750));
    assert!(
        (3..=RETIRADAS_A_LA_VEZ).contains(&a_la_vez),
        "el cierre debía retirar el resto en tandas ({a_la_vez} a la vez)"
    );
}

// ----------------------- F6: el estado de la barra es del servidor que corre

/// El cliente lee el fichero `servidor` nada más conectar y no sabe de quién
/// es: debe estar escrito (con el arranque nuevo y sin avisos) antes de que el
/// socket acepte a nadie, también cuando el servidor anterior murió sin
/// borrarlo.
#[tokio::test]
async fn el_estado_de_la_barra_esta_escrito_antes_de_que_el_socket_acepte_clientes() {
    let banco = Banco::nuevo();
    let entorno = comun::entorno();
    let rutas = entorno.rutas.clone();
    // Lo que dejó un servidor que murió sin borrarlo.
    estado::escribir_servidor(
        &rutas,
        &estado::EstadoServidorTermux {
            arranque: 1,
            avisos: vec![AvisoServidor::ApiApagada],
        },
    )
    .unwrap();
    let antes = ahora();
    let socket = magi::servidor::ruta_socket(&rutas);
    // Un cliente que conecta en cuanto el socket existe, sin dormir entre
    // intentos, y mira el estado en ese mismo instante.
    let observador = tokio::task::spawn_blocking({
        let (rutas, socket) = (rutas.clone(), socket.clone());
        move || {
            let fin = std::time::Instant::now() + Duration::from_secs(20);
            while std::time::Instant::now() < fin {
                if std::os::unix::net::UnixStream::connect(&socket).is_ok() {
                    return estado::leer_servidor(&rutas);
                }
                std::hint::spin_loop();
            }
            None
        }
    });
    let tarea = tokio::spawn(magi::servidor::arrancar_con(
        rutas.clone(),
        configuracion(),
        Some(banco.termux("35")),
    ));
    let visto = observador.await.expect("el observador");
    let visto = visto.expect("el cliente no encontró el estado del servidor al conectar");
    assert!(
        visto.arranque >= antes,
        "el cliente leyó el estado de un servidor anterior (arranque {})",
        visto.arranque
    );
    assert!(
        visto.avisos.is_empty(),
        "el cliente leyó los avisos de un servidor anterior: {:?}",
        visto.avisos
    );
    // Y el servidor sigue siendo el de siempre: se para limpio.
    let mut lector = comun::cliente(&socket, VERSION_PROTOCOLO).await;
    comun::enviar(lector.get_mut(), &MensajeCliente::Parar).await;
    let fin = tokio::time::timeout(Duration::from_secs(30), tarea).await;
    assert!(fin.is_ok(), "el servidor no terminó en 30 s");
    assert_eq!(estado::leer_servidor(&rutas), None);
}

/// Si el servidor no llega a crear el socket (aquí, la ruta está ocupada por
/// un directorio no vacío), no deja escrito un estado que diga que hay un
/// servidor.
#[tokio::test]
async fn si_el_socket_no_se_puede_crear_no_queda_el_estado_del_servidor() {
    let banco = Banco::nuevo();
    let entorno = comun::entorno();
    let rutas = entorno.rutas.clone();
    std::fs::create_dir_all(rutas.dir_runtime()).unwrap();
    let socket = magi::servidor::ruta_socket(&rutas);
    std::fs::create_dir_all(socket.join("dentro")).unwrap();
    let resultado =
        magi::servidor::arrancar_con(rutas.clone(), configuracion(), Some(banco.termux("35")))
            .await;
    assert!(resultado.is_err(), "no debía poder abrir el socket");
    assert_eq!(
        estado::leer_servidor(&rutas),
        None,
        "quedó escrito el estado de un servidor que no existe"
    );
}

/// A4: al terminar, el fichero de estado de la barra se quita antes que el
/// socket y el lock. Quitar el lock deja entrar a un servidor nuevo, que
/// escribe su propio fichero `servidor`: si el viejo lo quitara después, le
/// borraría el suyo. Un solo vigilante de inotify sobre los dos directorios da
/// el orden real de los borrados (una sola cola del kernel).
#[tokio::test]
async fn al_terminar_el_estado_de_la_barra_se_quita_antes_que_el_socket_y_el_lock() {
    use notify::Watcher as _;
    let banco = Banco::nuevo();
    let mut servidor = Suelto::arrancar(Some(banco.termux("35")), configuracion(), |_| {}).await;
    let rutas = servidor.rutas().clone();
    assert!(
        estado::leer_servidor(&rutas).is_some(),
        "sin estado al arrancar"
    );
    let vistos = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let mut vigilante = notify::recommended_watcher({
        let vistos = std::sync::Arc::clone(&vistos);
        move |evento: notify::Result<notify::Event>| {
            let Ok(evento) = evento else { return };
            if !matches!(evento.kind, notify::EventKind::Remove(_)) {
                return;
            }
            for ruta in evento.paths {
                if let Some(nombre) = ruta.file_name().and_then(|nombre| nombre.to_str()) {
                    vistos.lock().unwrap().push(nombre.to_string());
                }
            }
        }
    })
    .expect("un vigilante de ficheros");
    vigilante
        .watch(&rutas.dir_runtime(), notify::RecursiveMode::NonRecursive)
        .expect("vigilar el directorio del socket");
    vigilante
        .watch(&dir_estado(&rutas), notify::RecursiveMode::NonRecursive)
        .expect("vigilar el directorio de estado de Termux");

    servidor.parar().await;
    let esperados = ["servidor", "servidor.sock", "servidor.lock"];
    assert!(
        esperar_que(PLAZO, || {
            let vistos = vistos.lock().unwrap();
            esperados
                .iter()
                .all(|nombre| vistos.iter().any(|visto| visto == nombre))
        })
        .await,
        "no se vieron los tres borrados: {:?}",
        vistos.lock().unwrap()
    );
    let vistos = vistos.lock().unwrap().clone();
    let posicion = |nombre: &str| vistos.iter().position(|visto| visto == nombre).unwrap();
    assert!(
        posicion("servidor") < posicion("servidor.sock"),
        "el estado de la barra se quitó después del socket: {vistos:?}"
    );
    assert!(
        posicion("servidor") < posicion("servidor.lock"),
        "el estado de la barra se quitó después del lock: {vistos:?}"
    );
}
