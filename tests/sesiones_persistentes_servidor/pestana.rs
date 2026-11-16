//! La pestaña persistente en el servidor (pista A del Sprint 2): crear y
//! enganchar, soltada, reconexión que no le quita la sesión a otro equipo,
//! historial de tmux, listar y terminar con el nombre exacto.
//!
//! Todo con el servidor SSH de pruebas de `tests/comun` (sin red externa) y el
//! tmux falso de `tests/fijos/terminal`, que guarda su estado en ficheros: el
//! del tmux propio de MAGI (`MAGI_TMUX_ESTADO_MAGI`) y el del habitual. El
//! falso simula que otro equipo se engancha escribiendo el marcador de la
//! sesión (`<estado>.cliente.<nombre>`): el script del primero sale entonces con
//! 100, como cuando el cliente de tmux al que le quitan la sesión termina.
//! Nunca se toca el tmux real de la máquina: todas las pruebas ponen delante del
//! `PATH` un tmux falso.

use std::collections::HashSet;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::comun::{escenario, Escenario, OpcionesEscenario};
use magi::protocolo::{
    CausaAperturaTmux, ComandoInicial, CrearTmux, EstadoSesionRemota, InfoSesion, LineaHistorial,
    MensajeCliente, MensajeServidor, PeticionTmux, ResultadoTmux, ServidorTmux, TmuxPestana,
    VersionTmux, RANGO_TERMINAL,
};

const PLAZO: Duration = Duration::from_secs(15);

// ------------------------------------------------------------ el banco

/// Un escenario con el tmux falso: ficheros de estado, bitácora de llamadas y
/// el host de la prueba.
struct Banco {
    escenario: Escenario,
    /// Estado del tmux habitual del host de prueba.
    habitual: PathBuf,
    /// Estado del tmux propio de MAGI (`-L magi`).
    magi: PathBuf,
    /// Cada invocación del tmux falso, una línea.
    llamadas: PathBuf,
    host_id: i64,
    temporal: tempfile::TempDir,
}

struct OpcionesBanco {
    /// Lo que imprime `tmux -V`.
    version: &'static str,
    /// El host tiene la marca `sesiones_persistentes`.
    host_marcado: bool,
    /// Directorio con el binario `tmux` de la prueba (el falso por defecto).
    binarios: Option<PathBuf>,
    /// Programa real que hace de shell de las pestañas normales.
    programa_shell: Option<Vec<String>>,
}

impl Default for OpcionesBanco {
    fn default() -> Self {
        Self {
            version: "tmux 3.4",
            host_marcado: false,
            binarios: None,
            programa_shell: None,
        }
    }
}

fn fijos() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fijos/terminal")
}

async fn banco() -> Banco {
    banco_con(OpcionesBanco::default()).await
}

async fn banco_con(opciones: OpcionesBanco) -> Banco {
    let temporal = tempfile::tempdir().unwrap();
    let habitual = temporal.path().join("habitual");
    let magi = temporal.path().join("magi");
    let llamadas = temporal.path().join("llamadas");
    let escenario = escenario(OpcionesEscenario {
        hosts: 1,
        binarios: Some(opciones.binarios.unwrap_or_else(fijos)),
        programa_shell: opciones.programa_shell,
        entorno_extra: vec![
            (
                "MAGI_TMUX_ESTADO".to_string(),
                habitual.display().to_string(),
            ),
            (
                "MAGI_TMUX_ESTADO_MAGI".to_string(),
                magi.display().to_string(),
            ),
            (
                "MAGI_TMUX_LLAMADAS".to_string(),
                llamadas.display().to_string(),
            ),
            (
                "MAGI_TMUX_VERSION".to_string(),
                opciones.version.to_string(),
            ),
        ],
        ..Default::default()
    })
    .await;
    let host_id = escenario.hosts[0];
    if opciones.host_marcado {
        let almacen = magi::almacen::abrir(&escenario.rutas().base_datos()).unwrap();
        magi::almacen::hosts::fijar_sesiones_persistentes(almacen.conexion(), host_id, true)
            .unwrap();
        almacen.cerrar().unwrap();
    }
    Banco {
        escenario,
        habitual,
        magi,
        llamadas,
        host_id,
        temporal,
    }
}

impl Banco {
    fn llamadas(&self) -> String {
        std::fs::read_to_string(&self.llamadas).unwrap_or_default()
    }

    /// Cuántas veces se engancharon clientes (`attach-session`).
    fn enganches(&self) -> usize {
        self.llamadas()
            .lines()
            .filter(|linea| linea.contains("attach-session"))
            .count()
    }

    /// Espera a que el host haya ejecutado `n` enganches: la pestaña se da por
    /// abierta al mandar el `exec`, antes de que el host lo procese.
    async fn esperar_enganches(&self, n: usize) {
        let fin = tokio::time::Instant::now() + PLAZO;
        while self.enganches() < n {
            assert!(
                tokio::time::Instant::now() < fin,
                "se esperaban {n} enganches y hay {}: {}",
                self.enganches(),
                self.llamadas()
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// El exec del enganche más reciente que vio el host de pruebas.
    async fn exec_de_enganche(&self) -> String {
        let fin = tokio::time::Instant::now() + PLAZO;
        loop {
            let comandos = self.escenario.observado.comandos.lock().unwrap().clone();
            if let Some(comando) = comandos
                .iter()
                .rev()
                .find(|comando| comando.contains("magi-tmux enganchar"))
            {
                return comando.clone();
            }
            assert!(
                tokio::time::Instant::now() < fin,
                "el host no recibió el exec de enganchar"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Las fichas de sesiones persistentes que hay en SQLite (el servidor
    /// nunca las escribe).
    fn fichas(&self) -> usize {
        let almacen = magi::almacen::abrir(&self.escenario.rutas().base_datos()).unwrap();
        let fichas = magi::almacen::sesiones_persistentes::listar(almacen.conexion())
            .unwrap()
            .len();
        almacen.cerrar().unwrap();
        fichas
    }

    /// La pestaña `id` tal como la ve la última difusión.
    fn pestana(&self, id: u32) -> Option<&InfoSesion> {
        self.escenario
            .sesiones
            .iter()
            .find(|sesion| sesion.id == id)
    }

    /// Espera a que la pestaña cumpla la condición.
    async fn hasta_pestana(
        &mut self,
        id: u32,
        plazo: Duration,
        condicion: impl Fn(&InfoSesion) -> bool,
    ) -> bool {
        self.escenario
            .hasta(plazo, |escenario| {
                escenario
                    .sesiones
                    .iter()
                    .any(|sesion| sesion.id == id && condicion(sesion))
            })
            .await
    }

    /// Simula que otro equipo se engancha a la sesión con `attach -d`: la
    /// sesión pasa a tener un cliente de ese equipo y al cliente de tmux que
    /// ya estaba enganchado le quitan la sesión. Espera antes a que el host
    /// haya ejecutado `enganches` enganches: un cliente que aún no ha
    /// arrancado no vería el cambio.
    async fn otro_equipo_se_engancha(&self, nombre: &str, equipo: &str, enganches: usize) {
        self.esperar_enganches(enganches).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        cambiar_sesion(&self.magi, nombre, |campos| {
            campos[1] = "1".to_string();
            campos[4] = equipo.to_string();
        });
        std::fs::write(
            format!("{}.cliente.{nombre}", self.magi.display()),
            format!("{equipo}-{}\n", std::process::id()),
        )
        .unwrap();
    }

    /// Escribe lo que muestra el panel de una sesión del tmux propio.
    fn pantalla_de(&self, nombre: &str, texto: &str) {
        std::fs::write(format!("{}.pantalla.{nombre}", self.magi.display()), texto).unwrap();
    }
}

/// La etiqueta de este equipo, como la pone el servidor en `@magi_equipo`.
fn equipo() -> String {
    magi_nucleo::terminal::tmux::etiqueta_equipo(&magi::config::nombre_actual())
}

/// Una línea del fichero de estado del tmux falso.
fn linea_de_estado(nombre: &str, clientes: u32, equipo: &str, titulo: &str) -> String {
    let ahora = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    format!("{nombre}\t{clientes}\t1\t2026-11-09 19:00:00\t{equipo}\t{ahora}\t0\t{titulo}\t{ahora}")
}

fn sembrar(ruta: &Path, lineas: &[String]) {
    let mut contenido = lineas.join("\n");
    contenido.push('\n');
    std::fs::write(ruta, contenido).unwrap();
}

/// Los nombres de las sesiones de un fichero de estado.
fn nombres_de(ruta: &Path) -> Vec<String> {
    std::fs::read_to_string(ruta)
        .unwrap_or_default()
        .lines()
        .filter_map(|linea| linea.split('\t').next())
        .filter(|nombre| !nombre.is_empty())
        .map(str::to_string)
        .collect()
}

/// Los campos de una sesión del fichero de estado.
fn campos_de(ruta: &Path, nombre: &str) -> Option<Vec<String>> {
    std::fs::read_to_string(ruta)
        .unwrap_or_default()
        .lines()
        .map(|linea| {
            linea
                .split('\t')
                .map(str::to_string)
                .collect::<Vec<String>>()
        })
        .find(|campos| campos.first().map(String::as_str) == Some(nombre))
}

/// Cambia los campos de una sesión del fichero de estado.
fn cambiar_sesion(ruta: &Path, nombre: &str, cambio: impl FnOnce(&mut Vec<String>)) {
    let mut lineas: Vec<Vec<String>> = std::fs::read_to_string(ruta)
        .unwrap_or_default()
        .lines()
        .map(|linea| linea.split('\t').map(str::to_string).collect())
        .collect();
    let campos = lineas
        .iter_mut()
        .find(|campos| campos.first().map(String::as_str) == Some(nombre))
        .expect("la sesión sembrada");
    while campos.len() < 9 {
        campos.push(String::new());
    }
    cambio(campos);
    let contenido: Vec<String> = lineas.iter().map(|campos| campos.join("\t")).collect();
    sembrar(ruta, &contenido);
}

fn peticion(nombre: &str, crear: Option<CrearTmux>, compartir: bool) -> PeticionTmux {
    PeticionTmux {
        servidor: ServidorTmux::Magi,
        nombre: nombre.to_string(),
        crear,
        compartir,
    }
}

fn crear(carpeta: Option<&Path>, comando: Option<&str>) -> Option<CrearTmux> {
    Some(CrearTmux {
        carpeta: carpeta.map(|carpeta| carpeta.display().to_string()),
        comando: comando.map(str::to_string),
    })
}

/// Pide una pestaña con tmux y espera a que abra (`Ok`) o a que el servidor
/// diga por qué no (`Err` con la causa y el mensaje).
async fn abrir(
    escenario: &mut Escenario,
    host_id: i64,
    tmux: Option<PeticionTmux>,
    comandos_iniciales: Vec<ComandoInicial>,
) -> Result<InfoSesion, (CausaAperturaTmux, String)> {
    let conocidas: HashSet<u32> = escenario.sesiones.iter().map(|sesion| sesion.id).collect();
    escenario
        .enviar(&MensajeCliente::AbrirSesion {
            host_id,
            cols: 80,
            filas: 24,
            comandos_iniciales,
            orden: None,
            tmux,
        })
        .await;
    loop {
        if let Some(sesion) = escenario.sesiones.iter().find(|sesion| {
            sesion.estado == EstadoSesionRemota::Abierta && !conocidas.contains(&sesion.id)
        }) {
            return Ok(sesion.clone());
        }
        if let MensajeServidor::AperturaTmuxFallida { causa, mensaje, .. } =
            escenario.siguiente().await
        {
            return Err((causa, mensaje));
        }
    }
}

/// Adjunta el cliente del escenario a una pestaña y devuelve el volcado.
async fn adjuntar(escenario: &mut Escenario, sesion_id: u32) -> String {
    escenario
        .enviar(&MensajeCliente::Adjuntar {
            sesion_id,
            cols: 80,
            filas: 24,
            colores: None,
        })
        .await;
    match escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::PantallaCompleta { sesion_id: id, .. } if *id == sesion_id
            )
        })
        .await
    {
        MensajeServidor::PantallaCompleta { bytes, .. } => {
            String::from_utf8_lossy(&bytes).to_string()
        }
        otro => unreachable!("{otro:?}"),
    }
}

/// Adjunta el cliente del escenario a una pestaña y espera a que su pantalla
/// (el volcado y lo que llegue después) lleve `texto`. Devuelve todo lo visto.
async fn adjuntar_hasta(escenario: &mut Escenario, sesion_id: u32, texto: &str) -> String {
    let mut visto = adjuntar(escenario, sesion_id).await;
    let fin = tokio::time::Instant::now() + PLAZO;
    while !visto.contains(texto) {
        assert!(
            tokio::time::Instant::now() < fin,
            "la pantalla de la pestaña no llegó a llevar {texto:?}: {visto:?}"
        );
        for mensaje in escenario.escuchar(Duration::from_millis(200)).await {
            if let MensajeServidor::Datos {
                sesion_id: id,
                bytes,
            } = mensaje
            {
                if id == sesion_id {
                    visto.push_str(&String::from_utf8_lossy(&bytes));
                }
            }
        }
    }
    visto
}

/// Pide una captura completa del historial.
async fn capturar(
    escenario: &mut Escenario,
    sesion_id: u32,
    peticion_id: u64,
) -> Result<Vec<LineaHistorial>, String> {
    escenario
        .enviar(&MensajeCliente::CapturarHistorial {
            peticion_id,
            sesion_id,
        })
        .await;
    let mut lineas = Vec::new();
    loop {
        match escenario.siguiente().await {
            MensajeServidor::HistorialLote {
                peticion_id: id,
                lineas: trozo,
                ultimo,
                ..
            } if id == peticion_id => {
                lineas.extend(trozo);
                if ultimo {
                    return Ok(lineas);
                }
            }
            MensajeServidor::Error {
                mensaje,
                peticion_id: Some(id),
            } if id == peticion_id => return Err(mensaje),
            _ => {}
        }
    }
}

// ----------------------------------------------------- crear y enganchar

#[tokio::test]
async fn crear_y_enganchar_abre_la_pestana_con_pty_y_exec() {
    let mut banco = banco().await;
    let carpeta = banco.temporal.path().join("proyecto");
    std::fs::create_dir(&carpeta).unwrap();
    let host_id = banco.host_id;
    let sesion = abrir(
        &mut banco.escenario,
        host_id,
        Some(peticion(
            "agente-api",
            crear(Some(&carpeta), Some("claude")),
            false,
        )),
        Vec::new(),
    )
    .await
    .expect("la pestaña se abre");

    // InfoSesion.tmux lleva el servidor real, el nombre y la versión de tmux.
    assert_eq!(
        sesion.tmux,
        Some(TmuxPestana {
            servidor: ServidorTmux::Magi,
            nombre: "agente-api".to_string(),
            version: Some(VersionTmux::nueva(3, 4)),
            soltada: None,
        })
    );
    // La sesión existe en el tmux propio y se creó en su carpeta, sin
    // engancharse (`new-session -d`), y la ficha no la escribe el servidor.
    assert_eq!(nombres_de(&banco.magi), vec!["agente-api".to_string()]);
    assert!(nombres_de(&banco.habitual).is_empty());
    let llamadas = banco.llamadas();
    assert!(
        llamadas.contains(&format!(
            "new-session -d -s agente-api -x 80 -y 24 -c {}",
            carpeta.display()
        )),
        "{llamadas}"
    );
    assert_eq!(banco.fichas(), 0, "el servidor no escribe fichas");

    // El enganche va por `exec` en el canal con PTY, no escrito en la shell:
    // el script de 24 KB no pasa por la pantalla de la pestaña.
    let enganche = banco.exec_de_enganche().await;
    assert!(
        enganche.ends_with(&format!(
            " magi-tmux enganchar magi agente-api 1 {}",
            equipo()
        )),
        "{}",
        &enganche[enganche.len().saturating_sub(120)..]
    );
    assert!(
        enganche.len() > 20_000 && enganche.len() < 100_000,
        "el exec lleva el script entero y cabe en un argumento: {} bytes",
        enganche.len()
    );
    assert_eq!(banco.escenario.observado.ptys(), vec![(80, 24)]);
    assert!(
        banco.escenario.observado.shell().is_empty(),
        "no se escribió nada en una shell"
    );
    let volcado = adjuntar_hasta(
        &mut banco.escenario,
        sesion.id,
        "MAGI_TMUX_ADJUNTO=agente-api soltar=1",
    )
    .await;
    assert!(!volcado.contains("set -u") && !volcado.contains("terminal_tmux"));
    banco.esperar_enganches(1).await;
    assert_eq!(banco.enganches(), 1);
}

#[tokio::test]
async fn un_comando_de_arranque_va_a_tmux_y_los_iniciales_solo_a_una_sesion_nueva_sin_comando() {
    let mut banco = banco().await;
    let host_id = banco.host_id;
    // Sesión nueva sin comando propio: el snippet al conectar se escribe
    // DESPUÉS del enganche (le llega al cliente de tmux, que lo pasa al shell).
    let sesion = abrir(
        &mut banco.escenario,
        host_id,
        Some(peticion("sin-comando", crear(None, None), false)),
        vec![ComandoInicial {
            texto: "echo hola\n".to_string(),
            repetir: true,
        }],
    )
    .await
    .unwrap();
    // Se escribe en cuanto tmux pinta: llega al cliente de tmux, que lo pasa
    // a la sesión (el falso lo devuelve como entrada).
    adjuntar_hasta(
        &mut banco.escenario,
        sesion.id,
        "MAGI_TMUX_ENTRADA=echo hola",
    )
    .await;

    // Con comando propio, no se escribe.
    let con_comando = abrir(
        &mut banco.escenario,
        host_id,
        Some(peticion("con-comando", crear(None, Some("claude")), false)),
        vec![ComandoInicial {
            texto: "echo adios\n".to_string(),
            repetir: true,
        }],
    )
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    let volcado = adjuntar(&mut banco.escenario, con_comando.id).await;
    assert!(
        !volcado.contains("echo adios"),
        "se escribió un comando inicial en una sesión con comando propio: {volcado:?}"
    );

    // Y al engancharse a una que ya existía, tampoco.
    let existente = abrir(
        &mut banco.escenario,
        host_id,
        Some(peticion("sin-comando", None, true)),
        vec![ComandoInicial {
            texto: "echo repetido\n".to_string(),
            repetir: true,
        }],
    )
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    let volcado = adjuntar(&mut banco.escenario, existente.id).await;
    assert!(
        !volcado.contains("echo repetido"),
        "se escribió un comando inicial al engancharse a una sesión existente: {volcado:?}"
    );
}

#[tokio::test]
async fn una_carpeta_inexistente_no_deja_pestana_ni_ficha() {
    let mut banco = banco().await;
    let host_id = banco.host_id;
    let falta = banco.temporal.path().join("no-existe");
    let (causa, mensaje) = abrir(
        &mut banco.escenario,
        host_id,
        Some(peticion("agente-api", crear(Some(&falta), None), false)),
        Vec::new(),
    )
    .await
    .expect_err("la carpeta no existe");
    assert_eq!(causa, CausaAperturaTmux::SinCarpeta);
    assert!(
        mensaje.contains("no existe en") && mensaje.contains(&falta.display().to_string()),
        "{mensaje}"
    );
    // Sin pestaña: la lista acaba vacía y el estado de tmux, sin sesión.
    assert!(
        banco
            .escenario
            .hasta(PLAZO, |escenario| escenario.sesiones.is_empty())
            .await,
        "{:?}",
        banco.escenario.sesiones
    );
    assert!(nombres_de(&banco.magi).is_empty());
    assert_eq!(banco.fichas(), 0);
    assert_eq!(banco.enganches(), 0);
    // Nada que anotar: no se produjo ningún efecto y la conexión estaba bien.
    assert_eq!(banco.escenario.anotaciones("conexion_fallida"), 0);
    assert_eq!(banco.escenario.anotaciones("sesion_cerrada"), 0);
}

#[tokio::test]
async fn un_nombre_ocupado_da_la_causa_existe_y_no_toca_la_sesion() {
    let mut banco = banco().await;
    let host_id = banco.host_id;
    sembrar(
        &banco.magi,
        &[linea_de_estado("agente-api", 0, "sobremesa", "Claude Code")],
    );
    let (causa, mensaje) = abrir(
        &mut banco.escenario,
        host_id,
        Some(peticion("agente-api", crear(None, Some("otro")), false)),
        Vec::new(),
    )
    .await
    .expect_err("el nombre está ocupado");
    assert_eq!(causa, CausaAperturaTmux::Existe);
    assert!(mensaje.contains("ya hay una sesión"), "{mensaje}");
    assert_eq!(nombres_de(&banco.magi), vec!["agente-api".to_string()]);
    assert!(
        !banco.llamadas().contains("new-session"),
        "no se intentó crear sobre una viva"
    );
    assert_eq!(banco.enganches(), 0);

    // El cliente puede ofrecer engancharse a la existente: sin crear.
    let sesion = abrir(
        &mut banco.escenario,
        host_id,
        Some(peticion("agente-api", None, false)),
        Vec::new(),
    )
    .await
    .expect("engancharse a la existente");
    assert_eq!(sesion.tmux.as_ref().unwrap().nombre, "agente-api");
    banco.esperar_enganches(1).await;
    assert_eq!(banco.enganches(), 1);
}

#[tokio::test]
async fn sin_tmux_o_con_tmux_antiguo_no_hay_pestana_y_la_causa_es_tipada() {
    // Sin tmux: el binario falla al dar su versión, como si no estuviera.
    let sin_tmux = tempfile::tempdir().unwrap();
    let falso = sin_tmux.path().join("tmux");
    std::fs::write(&falso, "#!/bin/sh\nexit 1\n").unwrap();
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&falso, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut banco = banco_con(OpcionesBanco {
        binarios: Some(sin_tmux.path().to_path_buf()),
        ..Default::default()
    })
    .await;
    let host_id = banco.host_id;
    let (causa, mensaje) = abrir(
        &mut banco.escenario,
        host_id,
        Some(peticion("agente-api", crear(None, None), false)),
        Vec::new(),
    )
    .await
    .expect_err("sin tmux");
    assert_eq!(causa, CausaAperturaTmux::SinTmux);
    assert!(mensaje.contains("instala tmux en el host"), "{mensaje}");
    assert!(
        banco
            .escenario
            .hasta(PLAZO, |escenario| escenario.sesiones.is_empty())
            .await
    );

    // Con un tmux anterior a 3.0.
    let mut antiguo = banco_con(OpcionesBanco {
        version: "tmux 2.9a",
        ..Default::default()
    })
    .await;
    let host_id = antiguo.host_id;
    let (causa, mensaje) = abrir(
        &mut antiguo.escenario,
        host_id,
        Some(peticion("agente-api", crear(None, None), false)),
        Vec::new(),
    )
    .await
    .expect_err("tmux antiguo");
    assert_eq!(causa, CausaAperturaTmux::TmuxAntiguo);
    assert!(
        mensaje.contains("tmux demasiado antiguo (3.0 o posterior)"),
        "{mensaje}"
    );
    assert!(!antiguo.llamadas().contains("new-session"));
    assert!(nombres_de(&antiguo.magi).is_empty());
}

#[tokio::test]
async fn los_datos_no_validos_se_rechazan_sin_llegar_al_host() {
    let mut banco = banco().await;
    let host_id = banco.host_id;
    let casos: Vec<(PeticionTmux, &str)> = vec![
        (peticion("mal nombre", None, false), "nombre"),
        (peticion("a;b", crear(None, None), false), "nombre"),
        (
            peticion(
                "bien",
                crear(Some(Path::new("relativa/carpeta")), None),
                false,
            ),
            "carpeta",
        ),
        (
            peticion("bien", crear(None, Some("con\0nulo")), false),
            "comando",
        ),
        (
            PeticionTmux {
                servidor: ServidorTmux::Habitual,
                nombre: "bien".to_string(),
                crear: crear(None, None),
                compartir: false,
            },
            "solo se crean",
        ),
    ];
    for (peticion, esperado) in casos {
        let (causa, mensaje) = abrir(&mut banco.escenario, host_id, Some(peticion), Vec::new())
            .await
            .expect_err("datos no válidos");
        assert_eq!(causa, CausaAperturaTmux::Otra);
        assert!(
            mensaje.to_lowercase().contains(esperado),
            "{esperado:?} en {mensaje:?}"
        );
    }
    // Ni siquiera se abrió una conexión.
    assert_eq!(banco.escenario.observado.conexiones(), 0);
    assert!(banco.escenario.sesiones.is_empty());
}

// ------------------------------------------------------ pestaña soltada

#[tokio::test]
async fn si_otro_equipo_se_engancha_la_pestana_queda_soltada_con_su_pantalla() {
    let mut banco = banco().await;
    let host_id = banco.host_id;
    let sesion = abrir(
        &mut banco.escenario,
        host_id,
        Some(peticion("refactor", crear(None, None), false)),
        Vec::new(),
    )
    .await
    .unwrap();
    adjuntar_hasta(
        &mut banco.escenario,
        sesion.id,
        "MAGI_TMUX_ADJUNTO=refactor soltar=1",
    )
    .await;
    banco.esperar_enganches(1).await;

    // El portátil se engancha con `-d`: a este cliente le quitan la sesión.
    banco
        .otro_equipo_se_engancha("refactor", "portatil", 1)
        .await;
    assert!(
        banco
            .hasta_pestana(sesion.id, PLAZO, |pestana| {
                pestana.estado == EstadoSesionRemota::Caida
                    && pestana
                        .tmux
                        .as_ref()
                        .and_then(|tmux| tmux.soltada.as_ref())
                        .and_then(|soltada| soltada.por_equipo.as_deref())
                        == Some("portatil")
            })
            .await,
        "la pestaña no quedó soltada por el portátil: {:?}",
        banco.pestana(sesion.id)
    );
    let pestana = banco.pestana(sesion.id).unwrap().clone();
    let tmux = pestana.tmux.unwrap();
    assert_eq!(tmux.nombre, "refactor");
    assert!(tmux.soltada.unwrap().desde > 0);
    // No se cierra ni se anota como cierre: la sesión sigue ahí, en la lista.
    assert_eq!(banco.escenario.anotaciones("sesion_cerrada"), 0);
    assert_eq!(nombres_de(&banco.magi), vec!["refactor".to_string()]);
    // Conserva la última pantalla.
    let volcado = adjuntar(&mut banco.escenario, sesion.id).await;
    assert!(
        volcado.contains("MAGI_TMUX_ADJUNTO=refactor"),
        "la pestaña soltada perdió su pantalla: {volcado:?}"
    );
}

#[tokio::test]
async fn volver_a_una_pestana_soltada_suelta_a_los_demas_o_comparte() {
    let mut banco = banco().await;
    let host_id = banco.host_id;
    let sesion = abrir(
        &mut banco.escenario,
        host_id,
        Some(peticion("refactor", crear(None, None), false)),
        Vec::new(),
    )
    .await
    .unwrap();
    banco
        .otro_equipo_se_engancha("refactor", "portatil", 1)
        .await;
    assert!(
        banco
            .hasta_pestana(sesion.id, PLAZO, |pestana| pestana
                .tmux
                .as_ref()
                .is_some_and(|tmux| tmux.soltada.is_some()))
            .await
    );

    // Volver soltando a los demás: `attach -d` y la marca de soltada se quita.
    banco
        .escenario
        .enviar(&MensajeCliente::Reconectar {
            sesion_id: sesion.id,
            compartir: false,
        })
        .await;
    assert!(
        banco
            .hasta_pestana(sesion.id, PLAZO, |pestana| {
                pestana.estado == EstadoSesionRemota::Abierta
                    && pestana
                        .tmux
                        .as_ref()
                        .is_some_and(|tmux| tmux.soltada.is_none())
            })
            .await,
        "no volvió: {:?}",
        banco.pestana(sesion.id)
    );
    let volcado = adjuntar(&mut banco.escenario, sesion.id).await;
    assert!(
        volcado.contains("soltar=1"),
        "volver soltando debía usar attach -d: {volcado:?}"
    );

    // El portátil la recupera y esta vez se vuelve compartiendo.
    banco
        .otro_equipo_se_engancha("refactor", "portatil", 2)
        .await;
    assert!(
        banco
            .hasta_pestana(sesion.id, PLAZO, |pestana| pestana
                .tmux
                .as_ref()
                .is_some_and(|tmux| tmux.soltada.is_some()))
            .await
    );
    let antes = banco.enganches();
    banco
        .escenario
        .enviar(&MensajeCliente::Reconectar {
            sesion_id: sesion.id,
            compartir: true,
        })
        .await;
    assert!(
        banco
            .hasta_pestana(sesion.id, PLAZO, |pestana| {
                pestana.estado == EstadoSesionRemota::Abierta
                    && pestana
                        .tmux
                        .as_ref()
                        .is_some_and(|tmux| tmux.soltada.is_none())
            })
            .await
    );
    banco.esperar_enganches(antes + 1).await;
    assert_eq!(banco.enganches(), antes + 1);
    let ultima = banco
        .llamadas()
        .lines()
        .rfind(|linea| linea.contains("attach-session"))
        .unwrap()
        .to_string();
    assert!(
        !ultima.contains("-d"),
        "al compartir no se suelta a nadie: {ultima}"
    );
}

#[tokio::test]
async fn cerrar_una_pestana_soltada_la_quita_y_no_termina_la_sesion() {
    let mut banco = banco().await;
    let host_id = banco.host_id;
    let sesion = abrir(
        &mut banco.escenario,
        host_id,
        Some(peticion("refactor", crear(None, None), false)),
        Vec::new(),
    )
    .await
    .unwrap();
    banco
        .otro_equipo_se_engancha("refactor", "portatil", 1)
        .await;
    assert!(
        banco
            .hasta_pestana(sesion.id, PLAZO, |pestana| pestana
                .tmux
                .as_ref()
                .is_some_and(|tmux| tmux.soltada.is_some()))
            .await
    );
    banco
        .escenario
        .enviar(&MensajeCliente::Cerrar {
            sesion_id: sesion.id,
        })
        .await;
    assert!(
        banco
            .escenario
            .hasta(PLAZO, |escenario| escenario.sesiones.is_empty())
            .await
    );
    // Cerrar la pestaña no termina la sesión del host.
    assert_eq!(nombres_de(&banco.magi), vec!["refactor".to_string()]);
}

// --------------------------------------------------------- reconexión

#[tokio::test]
async fn la_reconexion_automatica_no_le_quita_la_sesion_a_otro_equipo() {
    let mut banco = banco().await;
    let host_id = banco.host_id;
    let sesion = abrir(
        &mut banco.escenario,
        host_id,
        Some(peticion("refactor", crear(None, None), false)),
        Vec::new(),
    )
    .await
    .unwrap();
    banco.esperar_enganches(1).await;
    assert_eq!(banco.enganches(), 1);
    // Mientras la pestaña está caída (red), el portátil se engancha. Sin
    // conexión, el script del sobremesa no llega a ver el marcador: lo mata
    // la caída.
    banco.esperar_enganches(1).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    banco.escenario.observado.tirar_conexiones().await;
    cambiar_sesion(&banco.magi, "refactor", |campos| {
        campos[1] = "1".to_string();
        campos[4] = "portatil".to_string();
    });
    assert!(
        banco
            .hasta_pestana(sesion.id, Duration::from_secs(40), |pestana| pestana
                .tmux
                .as_ref()
                .and_then(|tmux| tmux.soltada.as_ref())
                .is_some_and(
                    |soltada| soltada.por_equipo.as_deref() == Some("portatil")
                ))
            .await,
        "la pestaña no quedó soltada: {:?}",
        banco.pestana(sesion.id)
    );
    assert_eq!(
        banco.pestana(sesion.id).unwrap().estado,
        EstadoSesionRemota::Caida
    );
    // No se la quitó: no hubo ningún enganche más.
    banco.esperar_enganches(1).await;
    assert_eq!(banco.enganches(), 1, "{}", banco.llamadas());
    assert_eq!(
        campos_de(&banco.magi, "refactor").unwrap()[4],
        "portatil".to_string()
    );
}

#[tokio::test]
async fn la_reconexion_vuelve_soltando_si_los_clientes_son_de_este_equipo() {
    let mut banco = banco().await;
    let host_id = banco.host_id;
    let sesion = abrir(
        &mut banco.escenario,
        host_id,
        Some(peticion("refactor", crear(None, None), false)),
        Vec::new(),
    )
    .await
    .unwrap();
    // La conexión cae pero tmux aún tiene el cliente (sshd no se ha enterado):
    // un cliente, del propio equipo.
    banco.esperar_enganches(1).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    banco.escenario.observado.tirar_conexiones().await;
    cambiar_sesion(&banco.magi, "refactor", |campos| {
        campos[1] = "1".to_string();
        campos[4] = equipo();
    });
    assert!(
        banco
            .hasta_pestana(sesion.id, Duration::from_secs(40), |pestana| {
                pestana.estado == EstadoSesionRemota::Abierta
            })
            .await,
        "no se reconectó: {:?}",
        banco.pestana(sesion.id)
    );
    banco.esperar_enganches(2).await;
    assert_eq!(banco.enganches(), 2);
    let ultima = banco
        .llamadas()
        .lines()
        .rfind(|linea| linea.contains("attach-session"))
        .unwrap()
        .to_string();
    assert!(
        ultima.contains("-d"),
        "vuelve soltando a los clientes de la conexión caída: {ultima}"
    );
    assert!(banco.pestana(sesion.id).unwrap().tmux.is_some());
}

#[tokio::test]
async fn la_reconexion_de_un_host_marcado_con_nombre_automatico_sigue_en_tmux() {
    // Regresión del fallo heredado de la Fase 15: la reconexión usaba el nombre
    // que se pidió al abrir (ninguno, con nombre automático) y volvía con un
    // shell normal. Ahora sale del estado vivo de la pestaña.
    let mut banco = banco_con(OpcionesBanco {
        host_marcado: true,
        ..Default::default()
    })
    .await;
    let host_id = banco.host_id;
    let sesion = abrir(&mut banco.escenario, host_id, None, Vec::new())
        .await
        .unwrap();
    let nombre = format!("{}-1", equipo());
    assert_eq!(
        sesion.tmux.as_ref().map(|tmux| tmux.nombre.clone()),
        Some(nombre.clone())
    );
    assert_eq!(nombres_de(&banco.magi), vec![nombre.clone()]);
    banco.esperar_enganches(1).await;
    assert_eq!(banco.enganches(), 1);

    banco.esperar_enganches(1).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    banco.escenario.observado.tirar_conexiones().await;
    let ruta_llamadas = banco.llamadas.clone();
    assert!(
        banco
            .escenario
            .hasta(Duration::from_secs(40), |escenario| {
                let enganches = std::fs::read_to_string(&ruta_llamadas)
                    .unwrap_or_default()
                    .matches("attach-session")
                    .count();
                enganches >= 2
                    && escenario.sesiones.iter().any(|pestana| {
                        pestana.id == sesion.id
                            && pestana.estado == EstadoSesionRemota::Abierta
                            && pestana.reconexion.is_none()
                    })
            })
            .await,
        "no se reconectó a tmux: {:?}",
        banco.escenario.sesiones
    );
    let pestana = banco.pestana(sesion.id).unwrap();
    assert_eq!(
        pestana.tmux.as_ref().map(|tmux| tmux.nombre.as_str()),
        Some(nombre.as_str()),
        "la pestaña perdió su sesión de tmux al reconectar"
    );
    banco.esperar_enganches(2).await;
    assert_eq!(banco.enganches(), 2);
    // Y no se creó otra sesión.
    assert_eq!(nombres_de(&banco.magi), vec![nombre]);
}

#[tokio::test]
async fn si_la_sesion_ya_no_existe_al_reconectar_la_pestana_se_cierra() {
    let mut banco = banco().await;
    let host_id = banco.host_id;
    let sesion = abrir(
        &mut banco.escenario,
        host_id,
        Some(peticion("efimera", crear(None, None), false)),
        Vec::new(),
    )
    .await
    .unwrap();
    banco.esperar_enganches(1).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    banco.escenario.observado.tirar_conexiones().await;
    // La sesión termina mientras la pestaña está caída.
    std::fs::write(&banco.magi, "").unwrap();
    assert!(
        banco
            .escenario
            .hasta(Duration::from_secs(40), |escenario| {
                !escenario
                    .sesiones
                    .iter()
                    .any(|pestana| pestana.id == sesion.id)
            })
            .await,
        "la pestaña de una sesión terminada debía cerrarse: {:?}",
        banco.escenario.sesiones
    );
    banco.esperar_enganches(1).await;
    assert_eq!(banco.enganches(), 1, "no se enganchó a nada");
    assert!(
        banco
            .escenario
            .vistos
            .iter()
            .any(|mensaje| matches!(mensaje, MensajeServidor::AperturaTmuxFallida { nombre, .. } if nombre == "efimera")),
        "el solicitante debía saber por qué se cerró"
    );
}

/// Un directorio de binarios con el tmux falso y un `awk` que falla mientras
/// exista el fichero `falla_awk` a su lado: al host le falta `awk`, así que
/// `estado` escribe su cabecera, no lista ninguna sesión y sale con 127. Sin
/// el fichero, el `awk` es el de verdad.
fn binarios_con_awk_que_falla() -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let directorio = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(fijos().join("tmux"), directorio.path().join("tmux")).unwrap();
    let awk = directorio.path().join("awk");
    std::fs::write(
        &awk,
        "#!/bin/sh\n\
         directorio=${0%/*}\n\
         if [ -e \"$directorio/falla_awk\" ]; then\n\
             exit 127\n\
         fi\n\
         PATH=${PATH#\"$directorio\":}\n\
         exec awk \"$@\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&awk, std::fs::Permissions::from_mode(0o755)).unwrap();
    directorio
}

#[tokio::test]
async fn un_estado_incompleto_al_reconectar_no_cierra_la_pestana() {
    // Regresión: `estado` sin salir con 0 (le falta `awk` al host) solo trae la
    // cabecera. Leerlo como «la sesión ya no existe» cerraba la pestaña de una
    // sesión que seguía viva. Sin saber cómo está, la pestaña queda caída con
    // la instrucción y la sesión no se toca.
    let binarios = binarios_con_awk_que_falla();
    let mut banco = banco_con(OpcionesBanco {
        binarios: Some(binarios.path().to_path_buf()),
        ..Default::default()
    })
    .await;
    let host_id = banco.host_id;
    let sesion = abrir(
        &mut banco.escenario,
        host_id,
        Some(peticion("refactor", crear(None, None), false)),
        Vec::new(),
    )
    .await
    .unwrap();
    banco.esperar_enganches(1).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    std::fs::write(binarios.path().join("falla_awk"), "").unwrap();
    banco.escenario.observado.tirar_conexiones().await;
    assert!(
        banco
            .hasta_pestana(sesion.id, Duration::from_secs(40), |pestana| {
                pestana.estado == EstadoSesionRemota::Caida
                    && pestana
                        .motivo
                        .as_deref()
                        .is_some_and(|motivo| motivo.contains("no se pudo comprobar tmux"))
            })
            .await,
        "la pestaña debía quedar caída con la instrucción: {:?}",
        banco.pestana(sesion.id)
    );
    assert!(
        !banco
            .escenario
            .vistos
            .iter()
            .any(|mensaje| matches!(mensaje, MensajeServidor::AperturaTmuxFallida { .. })),
        "no se sabe que la sesión terminara: no se avisa de que terminó"
    );
    // La sesión no se toca y no se engancha a nada más.
    assert_eq!(nombres_de(&banco.magi), vec!["refactor".to_string()]);
    banco.esperar_enganches(1).await;
    assert_eq!(banco.enganches(), 1, "{}", banco.llamadas());
}

#[tokio::test]
async fn volver_a_una_pestana_soltada_que_falla_ya_no_enseña_la_marca_de_soltada() {
    // Regresión: al volver desde una pestaña soltada, si el intento acababa en
    // otra caída (aquí, un estado que no se puede leer) la pestaña conservaba
    // la marca de soltada de antes, con el equipo que ya no la tenía.
    let binarios = binarios_con_awk_que_falla();
    let mut banco = banco_con(OpcionesBanco {
        binarios: Some(binarios.path().to_path_buf()),
        ..Default::default()
    })
    .await;
    let host_id = banco.host_id;
    let sesion = abrir(
        &mut banco.escenario,
        host_id,
        Some(peticion("refactor", crear(None, None), false)),
        Vec::new(),
    )
    .await
    .unwrap();
    banco
        .otro_equipo_se_engancha("refactor", "portatil", 1)
        .await;
    assert!(
        banco
            .hasta_pestana(sesion.id, PLAZO, |pestana| pestana
                .tmux
                .as_ref()
                .is_some_and(|tmux| tmux.soltada.is_some()))
            .await,
        "la pestaña no quedó soltada: {:?}",
        banco.pestana(sesion.id)
    );
    std::fs::write(binarios.path().join("falla_awk"), "").unwrap();
    banco
        .escenario
        .enviar(&MensajeCliente::Reconectar {
            sesion_id: sesion.id,
            compartir: false,
        })
        .await;
    assert!(
        banco
            .hasta_pestana(sesion.id, PLAZO, |pestana| {
                pestana.estado == EstadoSesionRemota::Caida
                    && pestana
                        .motivo
                        .as_deref()
                        .is_some_and(|motivo| motivo.contains("no se pudo comprobar tmux"))
            })
            .await,
        "la pestaña debía quedar caída por no poder comprobar tmux: {:?}",
        banco.pestana(sesion.id)
    );
    assert!(
        banco
            .pestana(sesion.id)
            .and_then(|pestana| pestana.tmux.as_ref())
            .is_some_and(|tmux| tmux.soltada.is_none()),
        "ya no está soltada: {:?}",
        banco.pestana(sesion.id)
    );
}

#[tokio::test]
async fn si_la_sesion_del_tmux_habitual_ya_no_existe_la_reconexion_abre_un_shell_con_aviso() {
    // La Fase 15 abría un shell normal con un aviso; eso se mantiene solo para
    // el tmux habitual.
    let mut banco = banco().await;
    let host_id = banco.host_id;
    sembrar(&banco.habitual, &[linea_de_estado("de-siempre", 0, "", "")]);
    let sesion = abrir(
        &mut banco.escenario,
        host_id,
        Some(PeticionTmux {
            servidor: ServidorTmux::Habitual,
            nombre: "de-siempre".to_string(),
            crear: None,
            compartir: false,
        }),
        Vec::new(),
    )
    .await
    .unwrap();
    assert_eq!(
        sesion.tmux.as_ref().map(|tmux| tmux.servidor),
        Some(ServidorTmux::Habitual)
    );
    banco.esperar_enganches(1).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    banco.escenario.observado.tirar_conexiones().await;
    std::fs::write(&banco.habitual, "").unwrap();
    assert!(
        banco
            .escenario
            .hasta(Duration::from_secs(40), |escenario| {
                escenario.sesiones.iter().any(|pestana| {
                    pestana.id == sesion.id
                        && pestana.estado == EstadoSesionRemota::Abierta
                        && pestana.tmux.is_none()
                        && pestana.reconexion.is_none()
                })
            })
            .await,
        "{:?}",
        banco.escenario.sesiones
    );
}

#[tokio::test]
async fn una_pestana_normal_reconectada_apaga_los_modos_de_teclado_del_programa_anterior() {
    // El programa de la primera shell pide `modifyOtherKeys` (nivel 2) y la
    // segunda no: un shell nuevo empieza sin modos, así que el emulador del
    // servidor (y el de las ventanas) los apaga al reconectar. Con el nivel
    // viejo, Mayús+Intro llegaría al shell nuevo como `CSI 27;2;13~`.
    let temporal = tempfile::tempdir().unwrap();
    let marca = temporal.path().join("primera");
    let programa = format!(
        "[ -e {marca} ] || {{ touch {marca}; printf '\\033[>4;2m'; }}; exec sleep 30",
        marca = marca.display()
    );
    let mut banco = banco_con(OpcionesBanco {
        programa_shell: Some(vec!["sh".to_string(), "-c".to_string(), programa]),
        ..Default::default()
    })
    .await;
    let host_id = banco.host_id;
    let sesion = banco.escenario.abrir_sesion(host_id).await;
    let volcado = adjuntar_hasta(&mut banco.escenario, sesion, "\u{1b}[>4;2m").await;
    assert!(volcado.ends_with("\u{1b}[>4;2m"), "{volcado:?}");
    banco.escenario.observado.tirar_conexiones().await;
    assert!(
        banco
            .hasta_pestana(sesion, PLAZO, |pestana| pestana.reconexion.is_some())
            .await
    );
    assert!(
        banco
            .hasta_pestana(sesion, Duration::from_secs(40), |pestana| {
                pestana.estado == EstadoSesionRemota::Abierta && pestana.reconexion.is_none()
            })
            .await,
        "{:?}",
        banco.pestana(sesion)
    );
    let volcado = adjuntar(&mut banco.escenario, sesion).await;
    assert!(
        volcado.ends_with("\u{1b}[>4;0m"),
        "el nivel de modifyOtherKeys sobrevivió a la reconexión: {volcado:?}"
    );
}

// --------------------------------------------- host marcado y el habitual

#[tokio::test]
async fn un_host_marcado_abre_siempre_una_pestana_persistente_con_nombre_automatico() {
    let mut banco = banco_con(OpcionesBanco {
        host_marcado: true,
        ..Default::default()
    })
    .await;
    let host_id = banco.host_id;
    // Ya existe la 1: la nueva es la 2.
    let prefijo = equipo();
    sembrar(
        &banco.magi,
        &[linea_de_estado(&format!("{prefijo}-1"), 0, "", "")],
    );
    let sesion = abrir(&mut banco.escenario, host_id, None, Vec::new())
        .await
        .unwrap();
    let tmux = sesion.tmux.unwrap();
    assert_eq!(tmux.servidor, ServidorTmux::Magi);
    assert_eq!(tmux.nombre, format!("{prefijo}-2"));
    assert_eq!(
        campos_de(&banco.magi, &tmux.nombre).unwrap()[0],
        tmux.nombre
    );
    assert_eq!(banco.fichas(), 0);
}

#[tokio::test]
async fn un_host_marcado_sin_tmux_abre_una_pestana_normal_con_aviso() {
    let sin_tmux = tempfile::tempdir().unwrap();
    let falso = sin_tmux.path().join("tmux");
    std::fs::write(&falso, "#!/bin/sh\nexit 1\n").unwrap();
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&falso, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut banco = banco_con(OpcionesBanco {
        host_marcado: true,
        binarios: Some(sin_tmux.path().to_path_buf()),
        ..Default::default()
    })
    .await;
    let host_id = banco.host_id;
    let sesion = abrir(&mut banco.escenario, host_id, None, Vec::new())
        .await
        .unwrap();
    assert!(sesion.tmux.is_none());
    assert!(
        banco
            .hasta_pestana(sesion.id, PLAZO, |pestana| pestana
                .aviso
                .as_ref()
                .is_some_and(|aviso| aviso.texto.contains("no es persistente")))
            .await,
        "{:?}",
        banco.pestana(sesion.id)
    );
}

#[tokio::test]
async fn un_host_marcado_con_tmux_antiguo_abre_una_pestana_normal_con_aviso() {
    let mut banco = banco_con(OpcionesBanco {
        host_marcado: true,
        version: "tmux 2.9a",
        ..Default::default()
    })
    .await;
    let host_id = banco.host_id;
    let sesion = banco_abrir_normal(&mut banco, host_id).await;
    assert!(sesion.tmux.is_none());
    assert!(
        banco
            .hasta_pestana(sesion.id, PLAZO, |pestana| pestana
                .aviso
                .as_ref()
                .is_some_and(|aviso| aviso
                    .texto
                    .contains("tmux demasiado antiguo (3.0 o posterior)")
                    && aviso.texto.contains("no es persistente")))
            .await,
        "{:?}",
        banco.pestana(sesion.id)
    );
    // No se creó ninguna sesión.
    assert!(!banco.llamadas().contains("new-session"));
    assert!(nombres_de(&banco.magi).is_empty());
}

async fn banco_abrir_normal(banco: &mut Banco, host_id: i64) -> InfoSesion {
    abrir(&mut banco.escenario, host_id, None, Vec::new())
        .await
        .expect("la pestaña normal se abre")
}

#[tokio::test]
async fn engancharse_al_tmux_habitual_no_suelta_al_propio_terminal_del_usuario() {
    let mut banco = banco().await;
    let host_id = banco.host_id;
    sembrar(&banco.habitual, &[linea_de_estado("de-siempre", 1, "", "")]);
    // Aunque no se pida compartir, el tmux habitual no se suelta: la Fase 15
    // compartía.
    let sesion = abrir(
        &mut banco.escenario,
        host_id,
        Some(PeticionTmux {
            servidor: ServidorTmux::Habitual,
            nombre: "de-siempre".to_string(),
            crear: None,
            compartir: false,
        }),
        Vec::new(),
    )
    .await
    .unwrap();
    adjuntar_hasta(
        &mut banco.escenario,
        sesion.id,
        "MAGI_TMUX_ADJUNTO=de-siempre soltar=0",
    )
    .await;
    let llamadas = banco.llamadas();
    assert!(
        !llamadas.contains("-L magi"),
        "el tmux habitual no pasa por el servidor propio: {llamadas}"
    );
    let tmux = sesion.tmux.unwrap();
    assert_eq!(tmux.servidor, ServidorTmux::Habitual);
    assert!(tmux.soltada.is_none());
}

// ---------------------------------------------------------- historial

#[tokio::test]
async fn el_historial_de_una_pestana_persistente_es_el_de_tmux_por_tramos() {
    let mut banco = banco().await;
    let host_id = banco.host_id;
    let sesion = abrir(
        &mut banco.escenario,
        host_id,
        Some(peticion("largo", crear(None, None), false)),
        Vec::new(),
    )
    .await
    .unwrap();
    // 300 líneas (más que la pantalla) con color y una línea más larga que el
    // ancho de la pestaña.
    let mut texto = String::new();
    for numero in 1..=300 {
        texto.push_str(&format!("\u{1b}[31mlinea {numero}\u{1b}[0m final\n"));
    }
    texto.push_str(&format!("{}\n", "x".repeat(130)));
    banco.pantalla_de("largo", &texto);
    let lineas = capturar(&mut banco.escenario, sesion.id, RANGO_TERMINAL + 1)
        .await
        .expect("la captura");
    let textos: Vec<String> = lineas.iter().map(LineaHistorial::texto).collect();
    assert!(
        textos.contains(&"linea 1 final".to_string()),
        "falta lo más viejo"
    );
    assert!(textos.contains(&"linea 300 final".to_string()));
    // Con sus colores, en tramos.
    let primera = lineas
        .iter()
        .find(|linea| linea.texto() == "linea 1 final")
        .unwrap();
    assert!(primera.tramos.len() >= 2, "{primera:?}");
    // La línea larga sale envuelta por el ancho de la pestaña (80).
    let indice = textos
        .iter()
        .position(|texto| texto.starts_with(&"x".repeat(80)))
        .expect("la línea larga");
    assert!(lineas[indice].envuelta);
    // Se pidió a tmux por la suborden `historial` del tmux propio, a la
    // sesión exacta y con el historial configurado.
    let llamadas = banco.llamadas();
    assert!(
        llamadas.contains("capture-pane -p -e -J -S -10000 -t =largo:"),
        "{llamadas}"
    );
    // Sigue en pie la pestaña y la captura no la toca.
    assert_eq!(
        banco.pestana(sesion.id).unwrap().estado,
        EstadoSesionRemota::Abierta
    );
}

#[tokio::test]
async fn una_pestana_normal_captura_su_historial_como_en_la_fase_15() {
    let mut banco = banco().await;
    let host_id = banco.host_id;
    let sesion = banco.escenario.abrir_sesion(host_id).await;
    adjuntar_hasta(&mut banco.escenario, sesion, "MARCA_CLIENTE").await;
    let lineas = capturar(&mut banco.escenario, sesion, RANGO_TERMINAL + 2)
        .await
        .expect("captura de una pestaña normal");
    assert!(lineas
        .iter()
        .any(|linea| linea.texto().contains("MARCA_CLIENTE")));
    assert!(
        !banco.llamadas().contains("capture-pane"),
        "una pestaña normal no pregunta a tmux"
    );
}

#[tokio::test]
async fn un_fallo_al_leer_el_historial_de_tmux_es_un_error_y_no_abre_nada() {
    let mut banco = banco().await;
    let host_id = banco.host_id;
    let sesion = abrir(
        &mut banco.escenario,
        host_id,
        Some(peticion("se-va", crear(None, None), false)),
        Vec::new(),
    )
    .await
    .unwrap();
    // La sesión desaparece del host sin que la pestaña se entere aún.
    std::fs::write(&banco.magi, "").unwrap();
    let error = capturar(&mut banco.escenario, sesion.id, RANGO_TERMINAL + 3)
        .await
        .expect_err("la sesión ya no existe");
    assert!(
        error.contains("no se pudo leer el historial de tmux"),
        "{error}"
    );
}

// -------------------------------------------------- listar y terminar

#[tokio::test]
async fn listar_trae_las_sesiones_de_los_dos_servidores() {
    let mut banco = banco().await;
    let host_id = banco.host_id;
    sembrar(
        &banco.magi,
        &[linea_de_estado("agente-api", 1, "sobremesa", "Claude Code")],
    );
    sembrar(&banco.habitual, &[linea_de_estado("de-siempre", 0, "", "")]);
    let peticion_id = RANGO_TERMINAL + 10;
    banco
        .escenario
        .enviar(&MensajeCliente::ListarTmux {
            peticion_id,
            host_id,
        })
        .await;
    let MensajeServidor::SesionesTmux {
        sesiones, sin_tmux, ..
    } = banco
        .escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::SesionesTmux { peticion_id: id, .. } if *id == peticion_id
            )
        })
        .await
    else {
        unreachable!()
    };
    assert!(sin_tmux.is_none());
    let propia = sesiones
        .iter()
        .find(|sesion| sesion.servidor == ServidorTmux::Magi && sesion.nombre == "agente-api")
        .expect("la del tmux propio");
    assert_eq!(propia.clientes, 1);
    assert_eq!(propia.equipo.as_deref(), Some("sobremesa"));
    assert_eq!(propia.titulo.as_deref(), Some("Claude Code"));
    assert!(sesiones
        .iter()
        .any(|sesion| sesion.servidor == ServidorTmux::Habitual && sesion.nombre == "de-siempre"));
}

#[tokio::test]
async fn terminar_apunta_al_nombre_exacto_y_no_a_un_prefijo() {
    let mut banco = banco().await;
    let host_id = banco.host_id;
    sembrar(
        &banco.magi,
        &[
            linea_de_estado("magi-a-10", 0, "", ""),
            linea_de_estado("magi-a-2", 0, "", ""),
        ],
    );
    sembrar(&banco.habitual, &[linea_de_estado("magi-a-1", 0, "", "")]);
    // Terminar «magi-a-1» en el tmux propio, con «magi-a-10» viva: no existe y
    // no toca la otra.
    banco
        .escenario
        .enviar(&MensajeCliente::TerminarTmux {
            peticion_id: RANGO_TERMINAL + 20,
            host_id,
            servidor: ServidorTmux::Magi,
            nombre: "magi-a-1".to_string(),
        })
        .await;
    let respuesta = banco
        .escenario
        .esperar(|mensaje| matches!(mensaje, MensajeServidor::TmuxTerminada { .. }))
        .await;
    assert!(
        matches!(
            respuesta,
            MensajeServidor::TmuxTerminada {
                servidor: ServidorTmux::Magi,
                resultado: ResultadoTmux::NoExiste,
                ..
            }
        ),
        "{respuesta:?}"
    );
    assert_eq!(
        nombres_de(&banco.magi),
        vec!["magi-a-10".to_string(), "magi-a-2".to_string()]
    );
    assert_eq!(banco.escenario.anotaciones("tmux_terminada"), 0);
    // Terminar la que sí existe, con la fila ya escrita al responder.
    banco
        .escenario
        .enviar(&MensajeCliente::TerminarTmux {
            peticion_id: RANGO_TERMINAL + 21,
            host_id,
            servidor: ServidorTmux::Magi,
            nombre: "magi-a-10".to_string(),
        })
        .await;
    let respuesta = banco
        .escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::TmuxTerminada { peticion_id, .. } if *peticion_id == RANGO_TERMINAL + 21
            )
        })
        .await;
    assert!(matches!(
        respuesta,
        MensajeServidor::TmuxTerminada {
            servidor: ServidorTmux::Magi,
            resultado: ResultadoTmux::Ok,
            ..
        }
    ));
    assert_eq!(banco.escenario.anotaciones("tmux_terminada"), 1);
    assert_eq!(nombres_de(&banco.magi), vec!["magi-a-2".to_string()]);
    // La del tmux habitual, con el servidor que se pide.
    banco
        .escenario
        .enviar(&MensajeCliente::TerminarTmux {
            peticion_id: RANGO_TERMINAL + 22,
            host_id,
            servidor: ServidorTmux::Habitual,
            nombre: "magi-a-1".to_string(),
        })
        .await;
    let respuesta = banco
        .escenario
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::TmuxTerminada { peticion_id, .. } if *peticion_id == RANGO_TERMINAL + 22
            )
        })
        .await;
    assert!(matches!(
        respuesta,
        MensajeServidor::TmuxTerminada {
            servidor: ServidorTmux::Habitual,
            resultado: ResultadoTmux::Ok,
            ..
        }
    ));
    assert!(nombres_de(&banco.habitual).is_empty());
    assert_eq!(banco.escenario.anotaciones("tmux_terminada"), 2);
}

// ------------------------------------------- nada de contenido al log

/// El log del proceso de pruebas, en memoria.
fn log_global() -> BufferDeLog {
    static LOG: std::sync::OnceLock<BufferDeLog> = std::sync::OnceLock::new();
    LOG.get_or_init(|| {
        let log = BufferDeLog::default();
        let _ = tracing_subscriber::fmt()
            .with_writer(log.clone())
            .with_ansi(false)
            .with_max_level(tracing::Level::TRACE)
            .try_init();
        log
    })
    .clone()
}

/// Un escritor de `tracing` que guarda todo en memoria.
#[derive(Clone, Default)]
struct BufferDeLog(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for BufferDeLog {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for BufferDeLog {
    type Writer = BufferDeLog;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[tokio::test]
async fn ni_el_log_ni_registro_reciben_contenido_titulos_ni_comandos() {
    const COMANDO: &str = "echo COMANDO_SECRETO_7f3a";
    const TITULO: &str = "TITULO_SECRETO_9c1d";
    const CONTENIDO: &str = "CONTENIDO_SECRETO_55aa";
    const CARPETA: &str = "carpeta_secreta_31bc";
    // Un suscriptor global que guarda el log en memoria (uno de ámbito de hilo
    // no ve los eventos de las tareas del servidor). Solo se instala una vez
    // por proceso; los secretos de esta prueba son únicos, así que lo que
    // escriban las demás pruebas del binario no la afecta.
    let log = log_global();
    let mut banco = banco().await;
    let host_id = banco.host_id;
    // Todo el recorrido: crear con comando (y carpeta que no existe, para que
    // el mensaje de error la lleve al cliente pero no al log), crear bien,
    // soltada, historial, listar y terminar.
    let (_, mensaje) = abrir(
        &mut banco.escenario,
        host_id,
        Some(peticion(
            "falla",
            crear(Some(&banco.temporal.path().join(CARPETA)), Some(COMANDO)),
            false,
        )),
        Vec::new(),
    )
    .await
    .expect_err("carpeta inexistente");
    assert!(mensaje.contains(CARPETA), "el cliente sí lo ve: {mensaje}");
    let sesion = abrir(
        &mut banco.escenario,
        host_id,
        Some(peticion("secreta", crear(None, Some(COMANDO)), false)),
        Vec::new(),
    )
    .await
    .unwrap();
    cambiar_sesion(&banco.magi, "secreta", |campos| {
        campos[7] = TITULO.to_string()
    });
    banco.pantalla_de("secreta", &format!("{CONTENIDO}\n"));
    let lineas = capturar(&mut banco.escenario, sesion.id, RANGO_TERMINAL + 30)
        .await
        .unwrap();
    assert!(lineas.iter().any(|linea| linea.texto().contains(CONTENIDO)));
    banco
        .escenario
        .enviar(&MensajeCliente::ListarTmux {
            peticion_id: RANGO_TERMINAL + 31,
            host_id,
        })
        .await;
    let MensajeServidor::SesionesTmux { sesiones, .. } = banco
        .escenario
        .esperar(|mensaje| matches!(mensaje, MensajeServidor::SesionesTmux { .. }))
        .await
    else {
        unreachable!()
    };
    assert!(sesiones
        .iter()
        .any(|sesion| sesion.titulo.as_deref() == Some(TITULO)));
    // La vista previa llega al cliente que la pide, pero no al log.
    banco
        .escenario
        .enviar(&MensajeCliente::PedirVistaPrevia {
            peticion_id: RANGO_TERMINAL + 33,
            host_id,
            servidor: ServidorTmux::Magi,
            nombre: "secreta".to_string(),
        })
        .await;
    let MensajeServidor::VistaPrevia { lineas, error, .. } = banco
        .escenario
        .esperar(|mensaje| matches!(mensaje, MensajeServidor::VistaPrevia { .. }))
        .await
    else {
        unreachable!()
    };
    assert!(error.is_none(), "{error:?}");
    assert!(
        lineas.iter().any(|linea| linea.contains(CONTENIDO)),
        "el cliente sí la ve: {lineas:?}"
    );
    banco
        .otro_equipo_se_engancha("secreta", "portatil", 1)
        .await;
    assert!(
        banco
            .hasta_pestana(sesion.id, PLAZO, |pestana| pestana
                .tmux
                .as_ref()
                .is_some_and(|tmux| tmux.soltada.is_some()))
            .await
    );
    banco
        .escenario
        .enviar(&MensajeCliente::TerminarTmux {
            peticion_id: RANGO_TERMINAL + 32,
            host_id,
            servidor: ServidorTmux::Magi,
            nombre: "secreta".to_string(),
        })
        .await;
    banco
        .escenario
        .esperar(|mensaje| matches!(mensaje, MensajeServidor::TmuxTerminada { .. }))
        .await;

    // Nada de eso en REGISTRO ni en el log.
    let registro = banco.escenario.registro();
    for (tipo, detalle) in &registro {
        for secreto in [COMANDO, TITULO, CONTENIDO, CARPETA] {
            assert!(
                !detalle.contains(secreto),
                "REGISTRO {tipo} lleva {secreto}: {detalle}"
            );
        }
    }
    let texto_del_log = String::from_utf8_lossy(&log.0.lock().unwrap()).to_string();
    assert!(
        !texto_del_log.is_empty(),
        "el log de la prueba no recogió nada"
    );
    for secreto in [COMANDO, TITULO, CONTENIDO, CARPETA, "COMANDO_SECRETO"] {
        assert!(
            !texto_del_log.contains(secreto),
            "el log lleva {secreto}: {texto_del_log}"
        );
    }
    let _ = std::io::stdout().flush();
}
