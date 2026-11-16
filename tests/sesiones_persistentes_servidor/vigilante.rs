//! El vigilante y sus consultas (pista B del Sprint 2): comprobación sin
//! diálogos, `SesionesPersistentes{lista}`, vista previa y renombrar.
//!
//! Todo con el servidor SSH de pruebas de `tests/comun` (sin red externa) y el
//! tmux falso de `tests/fijos/terminal`, que aquí va envuelto en un guion que
//! da a cada host de la prueba (por su usuario, `MAGI_USUARIO`) su propio
//! estado de tmux, su registro de llamadas y, si se pide, un retraso al
//! empezar `estado`. Así cada host de un mismo escenario tiene sus sesiones.
//!
//! El ritmo del vigilante se acelera con `vigilar_ms_pruebas` (milisegundos) en
//! las pruebas que lo necesitan; las demás lo dejan en su valor por defecto
//! (60 s) y solo trabajan cuando una ventana les consulta.

use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;
use std::time::Duration;

use tokio::io::AsyncWriteExt as _;
use tokio::time::Instant;

use magi::modelo::IdentidadRef;
use magi::protocolo::{
    ErrorComprobacion, EstadoFicha, ListaPersistentes, MensajeCliente, MensajeServidor,
    ResultadoRenombrar, ServidorTmux, RANGO_TERMINAL,
};
use magi_nucleo::sesiones_persistentes::{DatosFicha, OrigenFicha};

use crate::comun::{escenario, siguiente_framed, Escenario, OpcionesEscenario, Ventana};

/// El tmux falso del repositorio.
const TMUX_FALSO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fijos/terminal/tmux");

/// El `tmux` que ven los `exec` del host de pruebas: el falso, con el estado,
/// las llamadas y el retraso de cada usuario del host.
const GUION_TMUX: &str = r#"#!/bin/sh
usuario=${MAGI_USUARIO:-ninguno}
base=${MAGI_PRUEBA_BASE:?}
MAGI_TMUX_ESTADO="$base/estado.$usuario"
MAGI_TMUX_LLAMADAS="$base/llamadas.$usuario"
export MAGI_TMUX_ESTADO MAGI_TMUX_LLAMADAS
if [ "${1:-}" = "-V" ] && [ -f "$base/retraso.$usuario" ]; then
    sleep "$(cat "$base/retraso.$usuario")"
fi
exec "${MAGI_TMUX_FALSO:?}" "$@"
"#;

// ------------------------------------------------------------------- apoyo

/// El directorio de la prueba: el guion de tmux y los ficheros por host.
struct Banco {
    base: tempfile::TempDir,
}

impl Banco {
    fn nuevo() -> Self {
        let base = tempfile::tempdir().expect("directorio de la prueba");
        let bin = base.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let guion = bin.join("tmux");
        std::fs::write(&guion, GUION_TMUX).unwrap();
        std::fs::set_permissions(&guion, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { base }
    }

    fn binarios(&self) -> PathBuf {
        self.base.path().join("bin")
    }

    fn entorno(&self) -> Vec<(String, String)> {
        vec![
            (
                "MAGI_PRUEBA_BASE".to_string(),
                self.base.path().display().to_string(),
            ),
            ("MAGI_TMUX_FALSO".to_string(), TMUX_FALSO.to_string()),
        ]
    }

    /// Las sesiones del tmux propio del host número `n` (1, 2…), una línea cada
    /// una con el formato del falso.
    fn magi(&self, n: usize, lineas: &[String]) {
        std::fs::write(
            self.base.path().join(format!("estado.usuario{n}.magi")),
            lineas.concat(),
        )
        .unwrap();
    }

    /// Las del tmux habitual del host `n`.
    fn habitual(&self, n: usize, lineas: &[String]) {
        std::fs::write(
            self.base.path().join(format!("estado.usuario{n}")),
            lineas.concat(),
        )
        .unwrap();
    }

    /// Lo que verá `capture-pane` de una sesión del tmux propio.
    fn pantalla(&self, n: usize, nombre: &str, texto: &str) {
        std::fs::write(
            self.base
                .path()
                .join(format!("estado.usuario{n}.magi.pantalla.{nombre}")),
            texto,
        )
        .unwrap();
    }

    /// Cuánto tarda en empezar `estado` en el host `n`.
    fn retraso(&self, n: usize, segundos: u32) {
        std::fs::write(
            self.base.path().join(format!("retraso.usuario{n}")),
            format!("{segundos}\n"),
        )
        .unwrap();
    }

    /// Las veces que el script de tmux ha empezado en el host `n` (cada
    /// ejecución de `estado` empieza con un `tmux -V`).
    fn comprobaciones(&self, n: usize) -> usize {
        self.llamadas(n)
            .lines()
            .filter(|linea| linea.trim() == "-V")
            .count()
    }

    /// El registro de llamadas a tmux del host `n`.
    fn llamadas(&self, n: usize) -> String {
        std::fs::read_to_string(self.base.path().join(format!("llamadas.usuario{n}")))
            .unwrap_or_default()
    }

    /// Los nombres de las sesiones del tmux propio del host `n`, tal como
    /// están ahora.
    fn nombres_magi(&self, n: usize) -> Vec<String> {
        std::fs::read_to_string(self.base.path().join(format!("estado.usuario{n}.magi")))
            .unwrap_or_default()
            .lines()
            .filter_map(|linea| linea.split('\t').next().map(str::to_string))
            .collect()
    }
}

fn ahora() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Una sesión en el formato del tmux falso: nombre, clientes, ventanas, creada,
/// equipo, actividad, campana, título y creación.
fn linea(nombre: &str, clientes: u32, equipo: &str, campana: bool, titulo: &str) -> String {
    format!(
        "{nombre}\t{clientes}\t1\tx\t{equipo}\t{}\t{}\t{titulo}\t{}\n",
        ahora(),
        u8::from(campana),
        ahora()
    )
}

/// La configuración del servidor de las pruebas: sin apagarse por falta de
/// ventanas y con el ritmo del vigilante en milisegundos (0, el normal).
fn configuracion(periodo_ms: u64) -> magi::config::Config {
    let mut config = magi::config::Config::default();
    config.servidor.gracia_apagado_seg = 600;
    config.persistentes.vigilar_ms_pruebas = periodo_ms;
    config
}

fn opciones(banco: &Banco, hosts: usize, config: magi::config::Config) -> OpcionesEscenario {
    OpcionesEscenario {
        hosts,
        sftp: false,
        binarios: Some(banco.binarios()),
        entorno_extra: banco.entorno(),
        config: Some(config),
        ..Default::default()
    }
}

fn datos_ficha(nombre: &str) -> DatosFicha {
    DatosFicha {
        nombre: nombre.to_string(),
        carpeta: None,
        comando: Some("claude --secreto-de-la-prueba".to_string()),
        comando_reanudar: None,
        nota: None,
        origen: OrigenFicha::Manual,
        creada_por: "portatil".to_string(),
    }
}

/// Crea una ficha como lo haría el cliente (el servidor nunca escribe en la
/// tabla) y devuelve su id.
fn crear_ficha(escenario: &Escenario, host_id: i64, nombre: &str) -> i64 {
    let almacen = magi::almacen::abrir(&escenario.rutas().base_datos()).unwrap();
    let id = magi_nucleo::almacen::sesiones_persistentes::crear(
        almacen.conexion(),
        host_id,
        &datos_ficha(nombre),
    )
    .unwrap();
    almacen.cerrar().unwrap();
    id
}

/// Marca un host para las sesiones persistentes (lo que hace el diálogo del
/// host).
fn marcar_host(escenario: &Escenario, host_id: i64) {
    let almacen = magi::almacen::abrir(&escenario.rutas().base_datos()).unwrap();
    magi::almacen::hosts::fijar_sesiones_persistentes(almacen.conexion(), host_id, true).unwrap();
    almacen.cerrar().unwrap();
}

/// Todas las filas de las tablas dadas, como texto, para comparar antes y
/// después.
fn volcado(escenario: &Escenario, tablas: &[&str]) -> String {
    let almacen = magi::almacen::abrir(&escenario.rutas().base_datos()).unwrap();
    let mut texto = String::new();
    for tabla in tablas {
        let mut sentencia = almacen
            .conexion()
            .prepare(&format!("SELECT * FROM {tabla} ORDER BY id"))
            .unwrap();
        let columnas = sentencia.column_count();
        let mut filas = sentencia.query([]).unwrap();
        while let Some(fila) = filas.next().unwrap() {
            texto.push_str(tabla);
            for columna in 0..columnas {
                texto.push_str(&format!("|{:?}", fila.get_ref(columna).unwrap()));
            }
            texto.push('\n');
        }
    }
    almacen.cerrar().unwrap();
    texto
}

/// Espera una `SesionesPersistentes` que cumpla el predicado.
async fn esperar_lista<F>(
    escenario: &mut Escenario,
    plazo: Duration,
    mut vale: F,
) -> Option<ListaPersistentes>
where
    F: FnMut(&ListaPersistentes) -> bool,
{
    let fin = Instant::now() + plazo;
    loop {
        let restante = fin.saturating_duration_since(Instant::now());
        match tokio::time::timeout(restante, escenario.siguiente()).await {
            Ok(MensajeServidor::SesionesPersistentes { lista }) if vale(&lista) => {
                return Some(lista)
            }
            Ok(_) => {}
            Err(_) => return None,
        }
    }
}

/// Lo mismo en una ventana extra.
async fn esperar_lista_en<F>(
    ventana: &mut Ventana,
    plazo: Duration,
    mut vale: F,
) -> Option<ListaPersistentes>
where
    F: FnMut(&ListaPersistentes) -> bool,
{
    let fin = Instant::now() + plazo;
    loop {
        let restante = fin.saturating_duration_since(Instant::now());
        match tokio::time::timeout(
            restante,
            siguiente_framed::<MensajeServidor, _>(&mut ventana.lector),
        )
        .await
        {
            Ok(MensajeServidor::SesionesPersistentes { lista }) if vale(&lista) => {
                return Some(lista)
            }
            Ok(_) => {}
            Err(_) => return None,
        }
    }
}

/// Los mensajes que llegan a una ventana extra durante un rato.
async fn escuchar_ventana(ventana: &mut Ventana, plazo: Duration) -> Vec<MensajeServidor> {
    let fin = Instant::now() + plazo;
    let mut llegados = Vec::new();
    loop {
        let restante = fin.saturating_duration_since(Instant::now());
        match tokio::time::timeout(
            restante,
            siguiente_framed::<MensajeServidor, _>(&mut ventana.lector),
        )
        .await
        {
            Ok(mensaje) => llegados.push(mensaje),
            Err(_) => return llegados,
        }
    }
}

fn son_listas(mensajes: &[MensajeServidor]) -> usize {
    mensajes
        .iter()
        .filter(|mensaje| matches!(mensaje, MensajeServidor::SesionesPersistentes { .. }))
        .count()
}

fn consulta(numero: u64, host_id: Option<i64>) -> MensajeCliente {
    MensajeCliente::ConsultarSesionesPersistentes {
        peticion_id: RANGO_TERMINAL + numero,
        host_id,
    }
}

const PLAZO: Duration = Duration::from_secs(20);

// ------------------------------------------------------- qué se comprueba

#[tokio::test]
async fn el_vigilante_comprueba_los_hosts_con_fichas_o_marcados_y_no_otros() {
    let banco = Banco::nuevo();
    let mut esc = escenario(opciones(&banco, 4, configuracion(150))).await;
    let ids = esc.hosts.clone();
    banco.magi(1, &[linea("api", 0, "portatil", false, "")]);
    banco.magi(2, &[linea("suelta", 1, "sobremesa", false, "Claude Code")]);
    banco.magi(3, &[linea("ajena", 0, "", false, "")]);
    banco.magi(4, &[linea("ajena", 0, "", false, "")]);
    crear_ficha(&esc, ids[0], "api");
    marcar_host(&esc, ids[1]);

    let lista = esperar_lista(&mut esc, PLAZO, |lista| {
        lista
            .fichas
            .iter()
            .any(|ficha| ficha.estado == EstadoFicha::VivaLibre)
            && lista
                .hosts
                .iter()
                .any(|host| host.host_id == ids[1] && host.error.is_none())
    })
    .await
    .expect("el vigilante no difundió el estado de los hosts vigilados");

    // Solo los hosts con ficha o marcados.
    let comprobados: Vec<i64> = lista.hosts.iter().map(|host| host.host_id).collect();
    assert_eq!(comprobados, vec![ids[0], ids[1]], "{lista:?}");
    assert_eq!(
        banco.comprobaciones(3),
        0,
        "el host 3 no tiene ficha ni marca"
    );
    assert_eq!(
        banco.comprobaciones(4),
        0,
        "el host 4 no tiene ficha ni marca"
    );
    assert!(banco.comprobaciones(1) >= 1);
    assert!(banco.comprobaciones(2) >= 1);
    assert_eq!(
        esc.observado.conexiones(),
        2,
        "una conexión por host vigilado"
    );

    // La ficha vive y trae su sesión.
    let ficha = &lista.fichas[0];
    assert_eq!(ficha.nombre, "api");
    assert_eq!(ficha.estado, EstadoFicha::VivaLibre);
    let sesion = ficha.sesion.as_ref().expect("la sesión de una ficha viva");
    assert_eq!(sesion.nombre, "api");
    assert_eq!(sesion.equipo.as_deref(), Some("portatil"));
    assert!(ficha.comprobada_en > 0);

    // El host marcado, sin fichas: su sesión sale como «sin ficha».
    let marcado = lista
        .hosts
        .iter()
        .find(|host| host.host_id == ids[1])
        .unwrap();
    assert_eq!(marcado.sin_ficha.len(), 1);
    assert_eq!(marcado.sin_ficha[0].nombre, "suelta");
    assert_eq!(marcado.sin_ficha[0].clientes, 1);
    assert_eq!(marcado.sin_ficha[0].titulo.as_deref(), Some("Claude Code"));
    assert!(marcado.habitual.is_empty());
    assert!(marcado.tmux.is_some());
    assert!(marcado.ahora > 0, "la hora del host");
}

#[tokio::test]
async fn las_sesiones_del_tmux_habitual_salen_aparte_y_no_casan_con_las_fichas() {
    let banco = Banco::nuevo();
    let mut esc = escenario(opciones(&banco, 1, configuracion(150))).await;
    let host = esc.hosts[0];
    // «api» solo existe en el tmux habitual: la ficha no la encuentra.
    banco.habitual(
        1,
        &[
            linea("api", 1, "", false, ""),
            linea("otra", 0, "", false, ""),
        ],
    );
    banco.magi(1, &[linea("web", 0, "", false, "")]);
    crear_ficha(&esc, host, "api");
    crear_ficha(&esc, host, "web");

    let lista = esperar_lista(&mut esc, PLAZO, |lista| {
        lista.fichas.len() == 2
            && lista
                .fichas
                .iter()
                .all(|ficha| ficha.estado != EstadoFicha::SinComprobar)
    })
    .await
    .expect("lista");
    let estado = |nombre: &str| {
        lista
            .fichas
            .iter()
            .find(|ficha| ficha.nombre == nombre)
            .unwrap()
            .estado
    };
    assert_eq!(estado("api"), EstadoFicha::Terminada);
    assert_eq!(estado("web"), EstadoFicha::VivaLibre);
    let host = &lista.hosts[0];
    assert!(host.sin_ficha.is_empty());
    let mut habituales: Vec<&str> = host
        .habitual
        .iter()
        .map(|sesion| sesion.nombre.as_str())
        .collect();
    habituales.sort_unstable();
    assert_eq!(habituales, vec!["api", "otra"]);
    assert!(host
        .habitual
        .iter()
        .all(|sesion| sesion.servidor == ServidorTmux::Habitual));
}

#[tokio::test]
async fn una_ficha_ilegible_se_salta_y_el_resto_se_sigue_comprobando() {
    let banco = Banco::nuevo();
    let mut esc = escenario(opciones(&banco, 2, configuracion(150))).await;
    let ids = esc.hosts.clone();
    banco.magi(1, &[linea("api", 0, "", false, "")]);
    banco.magi(2, &[linea("rota", 0, "", false, "")]);
    crear_ficha(&esc, ids[0], "api");
    let rota = crear_ficha(&esc, ids[1], "rota");
    // La ficha de otro proceso, con un origen que este binario no conoce.
    let almacen = magi::almacen::abrir(&esc.rutas().base_datos()).unwrap();
    almacen
        .conexion()
        .execute(
            "UPDATE SESIONES_PERSISTENTES SET origen = 'raro' WHERE id = ?1",
            [rota],
        )
        .unwrap();
    almacen.cerrar().unwrap();

    let lista = esperar_lista(&mut esc, PLAZO, |lista| {
        lista
            .fichas
            .iter()
            .any(|ficha| ficha.estado == EstadoFicha::VivaLibre)
    })
    .await
    .expect("el vigilante se quedó parado por una ficha ilegible");
    assert_eq!(lista.fichas.len(), 1, "solo la ficha legible");
    assert_eq!(lista.fichas[0].nombre, "api");
    // El host de la ficha ilegible no tiene otra ficha ni marca: no se mira.
    assert_eq!(banco.comprobaciones(2), 0);
}

// ----------------------------------------------------- ventanas y difusión

#[tokio::test]
async fn sin_ventanas_no_se_comprueba_nada_y_con_alguna_si() {
    let banco = Banco::nuevo();
    let mut esc = escenario(opciones(&banco, 1, configuracion(100))).await;
    let host = esc.hosts[0];
    banco.magi(1, &[linea("api", 0, "", false, "")]);
    crear_ficha(&esc, host, "api");
    esperar_lista(&mut esc, PLAZO, |lista| {
        lista
            .fichas
            .iter()
            .any(|ficha| ficha.estado == EstadoFicha::VivaLibre)
    })
    .await
    .expect("lista con la ventana conectada");

    // La única ventana se va: el servidor sigue (gracia larga) pero ya no
    // comprueba.
    esc.escritura.shutdown().await.unwrap();
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let antes = banco.comprobaciones(1);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(
        banco.comprobaciones(1),
        antes,
        "el vigilante comprobó sin ninguna ventana conectada"
    );

    // Vuelve una ventana: recibe enseguida la lista que había y el vigilante
    // reanuda.
    let mut ventana = esc.otra_ventana().await;
    let lista = esperar_lista_en(&mut ventana, Duration::from_secs(5), |_| true)
        .await
        .expect("la ventana nueva no recibió la lista");
    assert_eq!(lista.fichas.len(), 1);
    let mut reanudado = false;
    for _ in 0..50 {
        if banco.comprobaciones(1) > antes {
            reanudado = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        reanudado,
        "el vigilante no volvió con una ventana conectada"
    );
}

#[tokio::test]
async fn solo_se_difunde_cuando_algo_cambia_y_a_todas_las_ventanas() {
    let banco = Banco::nuevo();
    let mut esc = escenario(opciones(&banco, 1, configuracion(150))).await;
    let host = esc.hosts[0];
    let mut otra = esc.otra_ventana().await;
    banco.magi(1, &[linea("api", 0, "portatil", false, "")]);
    crear_ficha(&esc, host, "api");

    let libre = |lista: &ListaPersistentes| {
        lista
            .fichas
            .iter()
            .any(|ficha| ficha.estado == EstadoFicha::VivaLibre)
    };
    esperar_lista(&mut esc, PLAZO, libre)
        .await
        .expect("principal");
    esperar_lista_en(&mut otra, PLAZO, libre)
        .await
        .expect("la otra ventana también la recibe");

    // Pasan unas diez comprobaciones sin que cambie nada en el host: ni una
    // difusión más, aunque las horas de comprobación sí cambian.
    let comprobadas = banco.comprobaciones(1);
    let llegados = esc.escuchar(Duration::from_millis(1500)).await;
    assert!(
        banco.comprobaciones(1) >= comprobadas + 2,
        "el vigilante no siguió comprobando"
    );
    assert_eq!(
        son_listas(&llegados),
        0,
        "se difundió sin cambios: {llegados:?}"
    );
    let llegados = escuchar_ventana(&mut otra, Duration::from_millis(300)).await;
    assert_eq!(son_listas(&llegados), 0, "{llegados:?}");

    // Alguien se engancha a la sesión: cambia y lo ven las dos ventanas.
    banco.magi(1, &[linea("api", 1, "sobremesa", true, "")]);
    let enganchada = |lista: &ListaPersistentes| {
        lista.fichas.iter().any(|ficha| {
            ficha.estado == EstadoFicha::VivaEnganchada
                && ficha.sesion.as_ref().is_some_and(|sesion| {
                    sesion.campana && sesion.equipo.as_deref() == Some("sobremesa")
                })
        })
    };
    esperar_lista(&mut esc, PLAZO, enganchada)
        .await
        .expect("principal, tras el cambio");
    esperar_lista_en(&mut otra, PLAZO, enganchada)
        .await
        .expect("la otra ventana, tras el cambio");
}

#[tokio::test]
async fn una_ventana_nueva_recibe_la_lista_actual_tras_la_bienvenida() {
    let banco = Banco::nuevo();
    // Sin ritmo de pruebas: solo comprueba cuando se le consulta.
    let mut esc = escenario(opciones(&banco, 1, configuracion(0))).await;
    let host = esc.hosts[0];

    // Con nada que contar, una ventana nueva no recibe nada.
    let mut vacia = esc.otra_ventana().await;
    let llegados = escuchar_ventana(&mut vacia, Duration::from_millis(400)).await;
    assert_eq!(son_listas(&llegados), 0, "{llegados:?}");

    banco.magi(1, &[linea("api", 0, "portatil", false, "")]);
    crear_ficha(&esc, host, "api");
    esc.enviar(&consulta(1, None)).await;
    let lista = esperar_lista(&mut esc, PLAZO, |lista| {
        lista
            .fichas
            .iter()
            .any(|ficha| ficha.estado == EstadoFicha::VivaLibre)
    })
    .await
    .expect("lista tras la consulta");

    // Una ventana que se conecta ahora la recibe justo detrás de la
    // Bienvenida, sin esperar a ninguna comprobación.
    let mut nueva = esc.otra_ventana().await;
    let recibida = esperar_lista_en(&mut nueva, Duration::from_secs(3), |_| true)
        .await
        .expect("la ventana nueva no recibió la lista");
    assert_eq!(recibida.fichas, lista.fichas);
    assert_eq!(recibida.hosts, lista.hosts);
}

// ---------------------------------------------------------------- consulta

#[tokio::test]
async fn las_consultas_seguidas_se_agrupan_y_un_host_reciente_no_se_repite() {
    let banco = Banco::nuevo();
    let mut esc = escenario(opciones(&banco, 1, configuracion(0))).await;
    let host = esc.hosts[0];
    banco.magi(1, &[linea("api", 0, "", false, "")]);
    crear_ficha(&esc, host, "api");

    // Cinco consultas seguidas: una sola comprobación, y como mucho las tres
    // difusiones de una pasada (fichas nuevas, resultado y respuesta).
    for numero in 0..5 {
        esc.enviar(&consulta(numero, None)).await;
    }
    esperar_lista(&mut esc, PLAZO, |lista| {
        lista
            .fichas
            .iter()
            .any(|ficha| ficha.estado == EstadoFicha::VivaLibre)
    })
    .await
    .expect("lista");
    let llegados = esc.escuchar(Duration::from_millis(800)).await;
    assert_eq!(banco.comprobaciones(1), 1);
    // La primera ya se vio dentro de `esperar_lista`; no puede haber otras
    // cinco respuestas.
    assert!(
        son_listas(&llegados) <= 2,
        "las consultas no se agruparon: {}",
        son_listas(&llegados)
    );

    // Otra consulta, de hace menos de 5 s: no se repite el host, pero se
    // contesta igual difundiendo la lista.
    esc.enviar(&consulta(10, None)).await;
    esperar_lista(&mut esc, Duration::from_secs(5), |lista| {
        lista
            .fichas
            .iter()
            .any(|ficha| ficha.estado == EstadoFicha::VivaLibre)
    })
    .await
    .expect("la consulta no se contestó");
    assert_eq!(
        banco.comprobaciones(1),
        1,
        "se repitió un host comprobado hace menos de 5 s"
    );
}

#[tokio::test]
async fn una_consulta_con_host_comprueba_ademas_un_host_sin_fichas_ni_marca() {
    let banco = Banco::nuevo();
    let mut esc = escenario(opciones(&banco, 2, configuracion(0))).await;
    let ids = esc.hosts.clone();
    banco.magi(1, &[linea("api", 0, "", false, "")]);
    banco.magi(2, &[linea("extra", 2, "portatil", false, "")]);
    crear_ficha(&esc, ids[0], "api");

    esc.enviar(&consulta(1, Some(ids[1]))).await;
    let lista = esperar_lista(&mut esc, PLAZO, |lista| {
        lista
            .hosts
            .iter()
            .any(|host| host.host_id == ids[1] && host.error.is_none())
    })
    .await
    .expect("el host pedido a mano no se comprobó");
    let host = lista
        .hosts
        .iter()
        .find(|host| host.host_id == ids[1])
        .unwrap();
    assert_eq!(host.sin_ficha.len(), 1);
    assert_eq!(host.sin_ficha[0].nombre, "extra");
    assert_eq!(banco.comprobaciones(2), 1);

    // Un host inexistente o con un id disparatado no rompe nada.
    esc.enviar(&consulta(2, Some(9999))).await;
    esc.enviar(&consulta(3, Some(-4))).await;
    let lista = esperar_lista(&mut esc, Duration::from_secs(5), |lista| {
        lista.hosts.iter().all(|host| host.host_id != 9999)
    })
    .await
    .expect("sigue contestando");
    assert_eq!(lista.hosts.len(), 2);
}

#[tokio::test]
async fn una_consulta_se_contesta_aunque_no_se_puedan_leer_las_fichas() {
    // Revisión adversarial: si la lectura de la base falla (otro proceso la
    // tiene bloqueada, una importación a medias), la ventana que preguntó no se
    // queda esperando para siempre: recibe la lista que hay.
    let banco = Banco::nuevo();
    let mut esc = escenario(opciones(&banco, 1, configuracion(0))).await;
    let almacen = magi::almacen::abrir(&esc.rutas().base_datos()).unwrap();
    almacen
        .conexion()
        .execute_batch("DROP TABLE SESIONES_PERSISTENTES")
        .unwrap();
    almacen.cerrar().unwrap();

    esc.enviar(&consulta(1, None)).await;
    let lista = esperar_lista(&mut esc, Duration::from_secs(5), |_| true).await;
    assert!(
        lista.is_some(),
        "la consulta no se contestó porque la lectura falló"
    );
}

#[tokio::test]
async fn el_vigilante_reutiliza_la_conexion_viva_del_pool() {
    let banco = Banco::nuevo();
    let mut config = configuracion(0);
    config.servidor.gracia_apagado_seg = 600;
    let mut esc = escenario(OpcionesEscenario {
        multiplexar: true,
        ..opciones(&banco, 1, config)
    })
    .await;
    let host = esc.hosts[0];
    banco.magi(1, &[linea("api", 0, "", false, "")]);
    crear_ficha(&esc, host, "api");

    // Una pestaña abre la conexión del host (multiplexada: va al pool).
    let _sesion = esc.abrir_sesion(host).await;
    assert_eq!(esc.observado.conexiones(), 1);

    esc.enviar(&consulta(1, None)).await;
    esperar_lista(&mut esc, PLAZO, |lista| {
        lista
            .fichas
            .iter()
            .any(|ficha| ficha.estado == EstadoFicha::VivaLibre)
    })
    .await
    .expect("lista");
    assert_eq!(
        esc.observado.conexiones(),
        1,
        "el vigilante abrió otra conexión teniendo una viva"
    );
}

#[tokio::test]
async fn un_host_sin_tmux_o_con_tmux_anterior_a_3_0_deja_sus_fichas_sin_tmux() {
    // «sin tmux»: el `tmux -V` del host falla; «antiguo»: dice 2.9a.
    for (clave, valor, version) in [
        ("MAGI_TMUX_FALSO", "/bin/false", None),
        ("MAGI_TMUX_VERSION", "tmux 2.9a", Some((2, 9))),
    ] {
        let banco = Banco::nuevo();
        let mut ajustes = opciones(&banco, 1, configuracion(0));
        ajustes.entorno_extra.retain(|(nombre, _)| nombre != clave);
        ajustes
            .entorno_extra
            .push((clave.to_string(), valor.to_string()));
        let mut esc = escenario(ajustes).await;
        let host = esc.hosts[0];
        banco.magi(1, &[linea("api", 0, "", false, "")]);
        crear_ficha(&esc, host, "api");
        esc.enviar(&consulta(1, None)).await;
        let lista = esperar_lista(&mut esc, PLAZO, |lista| {
            lista.hosts.len() == 1 && lista.fichas[0].estado != EstadoFicha::SinComprobar
        })
        .await
        .unwrap_or_else(|| panic!("{clave}: no llegó la lista"));
        assert_eq!(lista.fichas[0].estado, EstadoFicha::SinTmux, "{clave}");
        assert_eq!(lista.hosts[0].error, None, "el host sí contestó");
        assert_eq!(
            lista.hosts[0].tmux.map(|v| (v.mayor, v.menor)),
            version,
            "{clave}"
        );
    }
}

// ----------------------------------------------------------- sin diálogos

/// Un host que pediría un diálogo: el vigilante no abre ninguno, el host queda
/// con `NecesitaDialogo` y sus fichas, inalcanzables (nunca terminadas).
async fn comprobar_sin_dialogos(opciones_escenario: OpcionesEscenario, banco: &Banco) {
    let mut esc = escenario(opciones_escenario).await;
    let host = esc.hosts[0];
    // La sesión no existe en el host: si el servidor diera el estado por
    // «sin sesiones», la ficha saldría terminada.
    banco.magi(1, &[]);
    crear_ficha(&esc, host, "api");
    esc.enviar(&consulta(1, None)).await;
    let lista = esperar_lista(&mut esc, PLAZO, |lista| {
        lista
            .hosts
            .iter()
            .any(|host| host.error == Some(ErrorComprobacion::NecesitaDialogo))
    })
    .await
    .unwrap_or_else(|| panic!("no llegó el error de diálogo; vistos: {:?}", esc.vistos));
    assert_eq!(
        lista.fichas[0].estado,
        EstadoFicha::Inalcanzable,
        "un host que pide un diálogo nunca da sus fichas por terminadas"
    );
    assert!(lista.hosts[0].sin_ficha.is_empty());
    // Ni un solo diálogo, ni siquiera de los que se pueden contestar.
    esc.escuchar(Duration::from_millis(500)).await;
    let dialogos: Vec<&MensajeServidor> = esc
        .vistos
        .iter()
        .filter(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::HuellaDesconocida { .. }
                    | MensajeServidor::HuellaCambiada { .. }
                    | MensajeServidor::PideFrase { .. }
                    | MensajeServidor::PideContrasena { .. }
                    | MensajeServidor::PideLlavero { .. }
            )
        })
        .collect();
    assert!(
        dialogos.is_empty(),
        "el vigilante abrió un diálogo: {dialogos:?}"
    );
    assert_eq!(banco.comprobaciones(1), 0, "no llegó a ejecutar nada");
}

#[tokio::test]
async fn un_host_que_pide_la_contrasena_no_abre_dialogo_ni_da_terminadas_sus_fichas() {
    let banco = Banco::nuevo();
    comprobar_sin_dialogos(
        OpcionesEscenario {
            identidad: Some(IdentidadRef::Contrasena),
            ..opciones(&banco, 1, configuracion(0))
        },
        &banco,
    )
    .await;
}

#[tokio::test]
async fn un_host_con_la_contrasena_en_el_llavero_no_se_la_pide_al_llavero() {
    let banco = Banco::nuevo();
    comprobar_sin_dialogos(
        OpcionesEscenario {
            identidad: Some(IdentidadRef::ContrasenaLlavero),
            ..opciones(&banco, 1, configuracion(0))
        },
        &banco,
    )
    .await;
}

#[tokio::test]
async fn un_host_con_la_huella_sin_aceptar_no_abre_dialogo() {
    let banco = Banco::nuevo();
    comprobar_sin_dialogos(
        OpcionesEscenario {
            huella_conocida: false,
            ..opciones(&banco, 1, configuracion(0))
        },
        &banco,
    )
    .await;
}

#[tokio::test]
async fn un_host_con_una_clave_con_frase_no_abre_dialogo() {
    let banco = Banco::nuevo();
    // Una clave de fichero protegida con frase: sin agente, hace falta la
    // frase y el vigilante no puede preguntarla.
    let clave = ssh_key::PrivateKey::random(
        &mut ssh_key::rand_core::UnwrapErr(ssh_key::getrandom::SysRng),
        ssh_key::Algorithm::Ed25519,
    )
    .unwrap();
    let cifrada = clave
        .encrypt(
            &mut ssh_key::rand_core::UnwrapErr(ssh_key::getrandom::SysRng),
            "frase de la prueba",
        )
        .unwrap();
    let ruta = banco.base.path().join("clave_con_frase");
    std::fs::write(&ruta, cifrada.to_openssh(ssh_key::LineEnding::LF).unwrap()).unwrap();
    std::fs::set_permissions(&ruta, std::fs::Permissions::from_mode(0o600)).unwrap();
    comprobar_sin_dialogos(
        OpcionesEscenario {
            identidad: Some(IdentidadRef::Fichero(ruta.display().to_string())),
            ..opciones(&banco, 1, configuracion(0))
        },
        &banco,
    )
    .await;
}

// ------------------------------------------------------------------ plazos

#[tokio::test]
async fn un_host_que_no_contesta_pasa_su_plazo_y_no_retiene_ni_el_canal_ni_la_conexion() {
    let banco = Banco::nuevo();
    let mut esc = escenario(opciones(&banco, 2, configuracion(0))).await;
    let ids = esc.hosts.clone();
    banco.magi(1, &[linea("api", 0, "", false, "")]);
    banco.magi(2, &[linea("web", 0, "", false, "")]);
    // El host 2 no termina nunca de arrancar `estado`.
    banco.retraso(2, 300);
    crear_ficha(&esc, ids[0], "api");
    crear_ficha(&esc, ids[1], "web");

    let inicio = Instant::now();
    esc.enviar(&consulta(1, None)).await;
    // El host sano se ve antes de que acabe el plazo del colgado: el plazo es
    // de cada host.
    let primera = esperar_lista(&mut esc, PLAZO, |lista| {
        lista
            .hosts
            .iter()
            .any(|host| host.host_id == ids[0] && host.error.is_none())
    })
    .await
    .expect("el host sano");
    assert!(
        inicio.elapsed() < Duration::from_secs(8),
        "el host sano esperó al colgado"
    );
    assert!(primera.hosts.iter().all(|host| host.host_id != ids[1]));

    // El colgado acaba con `Plazo` a los 10 s y su ficha no se da por
    // terminada.
    let lista = esperar_lista(&mut esc, Duration::from_secs(15), |lista| {
        lista
            .hosts
            .iter()
            .any(|host| host.host_id == ids[1] && host.error.is_some())
    })
    .await
    .expect("el plazo del host colgado");
    let transcurrido = inicio.elapsed();
    assert!(
        transcurrido >= Duration::from_secs(9) && transcurrido < Duration::from_secs(13),
        "el plazo no es el de 10 s: {transcurrido:?}"
    );
    let colgado = lista
        .hosts
        .iter()
        .find(|host| host.host_id == ids[1])
        .unwrap();
    assert_eq!(colgado.error, Some(ErrorComprobacion::Plazo));
    let ficha_web = lista.fichas.iter().find(|f| f.nombre == "web").unwrap();
    assert_eq!(ficha_web.estado, EstadoFicha::Inalcanzable);

    // El canal se cerró (el host mata lo que corre en un canal cerrado) y la
    // conexión se soltó: sin canales, el pool la cierra al acabar su gracia
    // de 30 s. Si el vigilante no la soltara por identidad, no se cerraría.
    let mut sin_canales = false;
    for _ in 0..30 {
        if esc
            .observado
            .exec_activos
            .load(std::sync::atomic::Ordering::SeqCst)
            == 0
        {
            sin_canales = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(sin_canales, "el exec del host colgado sigue abierto");
    let mut cerradas = false;
    for _ in 0..450 {
        if esc.observado.vivas() == 0 {
            cerradas = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        cerradas,
        "el pool no recicló las conexiones del vigilante: quedan {}",
        esc.observado.vivas()
    );
}

#[tokio::test]
async fn se_comprueban_ocho_hosts_a_la_vez_como_mucho() {
    let banco = Banco::nuevo();
    let mut esc = escenario(opciones(&banco, 10, configuracion(0))).await;
    let ids = esc.hosts.clone();
    for (indice, id) in ids.iter().enumerate() {
        banco.magi(indice + 1, &[linea("api", 0, "", false, "")]);
        // Cada `estado` tarda un rato: da tiempo a que se solapen.
        banco.retraso(indice + 1, 2);
        marcar_host(&esc, *id);
    }
    esc.enviar(&consulta(1, None)).await;
    let lista = esperar_lista(&mut esc, PLAZO, |lista| {
        lista.hosts.len() == 10 && lista.hosts.iter().all(|host| host.error.is_none())
    })
    .await
    .expect("los diez hosts");
    assert_eq!(lista.hosts.len(), 10);
    let maximo = esc
        .observado
        .exec_max
        .load(std::sync::atomic::Ordering::SeqCst);
    assert!(maximo <= 8, "se comprobaron {maximo} hosts a la vez");
    assert!(
        maximo >= 4,
        "no se comprobó en paralelo (máximo a la vez: {maximo})"
    );
}

#[tokio::test]
async fn un_estado_que_no_termina_bien_no_da_por_terminadas_las_fichas_de_sesiones_vivas() {
    // Revisión adversarial: en el host falla el `awk` del listado (o falta). El
    // script escribe la versión de tmux, no escribe ninguna sesión y sale con
    // error: si el vigilante solo mirara lo escrito, todas las fichas saldrían
    // «terminadas» aunque sus sesiones vivan, y `C` las borraría.
    let banco = Banco::nuevo();
    let awk_real = std::process::Command::new("sh")
        .args(["-c", "command -v awk"])
        .output()
        .expect("sh");
    let awk_real = String::from_utf8_lossy(&awk_real.stdout).trim().to_string();
    assert!(!awk_real.is_empty(), "las pruebas necesitan awk");
    let awk = banco.binarios().join("awk");
    // Solo falla el `awk` del listado de `estado` (el que lleva `-v servidor=`);
    // el del tmux falso y los demás van al de verdad.
    std::fs::write(
        &awk,
        format!(
            "#!/bin/sh\nfor a in \"$@\"; do case $a in servidor=*) exit 3 ;; esac; done\nexec {awk_real} \"$@\"\n"
        ),
    )
    .unwrap();
    std::fs::set_permissions(&awk, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut esc = escenario(opciones(&banco, 1, configuracion(0))).await;
    let host = esc.hosts[0];
    banco.magi(1, &[linea("api", 0, "", false, "")]);
    crear_ficha(&esc, host, "api");

    esc.enviar(&consulta(1, None)).await;
    let lista = esperar_lista(&mut esc, PLAZO, |lista| lista.hosts.len() == 1)
        .await
        .expect("lista");
    assert_eq!(
        lista.hosts[0].error,
        Some(ErrorComprobacion::SinRespuesta),
        "un script que sale con error no es una comprobación: {lista:?}"
    );
    assert_eq!(lista.fichas[0].estado, EstadoFicha::Inalcanzable);
    assert!(lista.hosts[0].sin_ficha.is_empty());
}

// ------------------------------------------------------------ vista previa

#[tokio::test]
async fn la_vista_previa_va_solo_a_quien_la_pide_con_su_tope_y_sin_guardarse() {
    let banco = Banco::nuevo();
    let mut config = configuracion(0);
    config.persistentes.vista_previa_lineas = 8;
    let mut esc = escenario(opciones(&banco, 1, config)).await;
    let host = esc.hosts[0];
    banco.magi(
        1,
        &[
            linea("api", 0, "", false, ""),
            linea("grande", 0, "", false, ""),
        ],
    );
    let pantalla: String = (1..=40).map(|n| format!("linea {n:02}\n")).collect();
    banco.pantalla(1, "api", &pantalla);
    let enorme: String = (0..10)
        .map(|n| format!("{}{n}\n", "x".repeat(20_000)))
        .collect();
    banco.pantalla(1, "grande", &enorme);
    let mut otra = esc.otra_ventana().await;
    let registro_antes = volcado(&esc, &["REGISTRO"]);

    // Las últimas líneas, solo las pedidas.
    esc.enviar(&MensajeCliente::PedirVistaPrevia {
        peticion_id: RANGO_TERMINAL + 1,
        host_id: host,
        servidor: ServidorTmux::Magi,
        nombre: "api".to_string(),
    })
    .await;
    let respuesta = esc
        .esperar(|mensaje| matches!(mensaje, MensajeServidor::VistaPrevia { .. }))
        .await;
    let MensajeServidor::VistaPrevia {
        peticion_id,
        host_id,
        nombre,
        lineas,
        leida_en,
        error,
    } = respuesta
    else {
        unreachable!()
    };
    assert_eq!(peticion_id, RANGO_TERMINAL + 1);
    assert_eq!(host_id, host);
    assert_eq!(nombre, "api");
    assert_eq!(error, None);
    assert_eq!(lineas.len(), 8, "{lineas:?}");
    assert_eq!(lineas.first().unwrap(), "linea 33");
    assert_eq!(lineas.last().unwrap(), "linea 40");
    assert!((leida_en - ahora()).abs() < 60);

    // Con el tope de 64 KiB.
    esc.enviar(&MensajeCliente::PedirVistaPrevia {
        peticion_id: RANGO_TERMINAL + 2,
        host_id: host,
        servidor: ServidorTmux::Magi,
        nombre: "grande".to_string(),
    })
    .await;
    let MensajeServidor::VistaPrevia { lineas, error, .. } = esc
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::VistaPrevia { peticion_id, .. } if *peticion_id == RANGO_TERMINAL + 2
            )
        })
        .await
    else {
        unreachable!()
    };
    assert_eq!(error, None);
    let bytes: usize = lineas.iter().map(String::len).sum::<usize>() + lineas.len();
    assert!(!lineas.is_empty());
    assert!(bytes <= 64 * 1024, "la vista previa pesa {bytes} bytes");
    assert!(
        lineas.last().unwrap().ends_with('9'),
        "se pierden las últimas líneas"
    );

    // La otra ventana no ve nada de esto.
    let llegados = escuchar_ventana(&mut otra, Duration::from_millis(600)).await;
    assert!(
        !llegados
            .iter()
            .any(|mensaje| matches!(mensaje, MensajeServidor::VistaPrevia { .. })),
        "la vista previa llegó a otra ventana: {llegados:?}"
    );

    // No se guarda ni se registra nada.
    assert_eq!(volcado(&esc, &["REGISTRO"]), registro_antes);
}

#[tokio::test]
async fn la_vista_previa_dice_por_que_no_pudo_leerse() {
    let banco = Banco::nuevo();
    let mut esc = escenario(opciones(&banco, 1, configuracion(0))).await;
    let host = esc.hosts[0];
    banco.magi(1, &[linea("api", 0, "", false, "")]);

    let pedir = |numero: u64, host_id: i64, nombre: &str| MensajeCliente::PedirVistaPrevia {
        peticion_id: RANGO_TERMINAL + numero,
        host_id,
        servidor: ServidorTmux::Magi,
        nombre: nombre.to_string(),
    };
    for (numero, mensaje, esperado) in [
        (1, pedir(1, host, "no-existe"), "ya no existe"),
        (2, pedir(2, host, "a b"), "no admitido"),
        (3, pedir(3, 9999, "api"), "ya no existe"),
    ] {
        esc.enviar(&mensaje).await;
        let MensajeServidor::VistaPrevia { lineas, error, .. } = esc
            .esperar(|mensaje| {
                matches!(
                    mensaje,
                    MensajeServidor::VistaPrevia { peticion_id, .. } if *peticion_id == RANGO_TERMINAL + numero
                )
            })
            .await
        else {
            unreachable!()
        };
        assert!(lineas.is_empty());
        let error = error.unwrap_or_else(|| panic!("la petición {numero} no dio error"));
        assert!(error.contains(esperado), "{numero}: {error}");
    }
}

#[tokio::test]
async fn la_vista_previa_de_un_host_inalcanzable_lleva_el_error() {
    let banco = Banco::nuevo();
    let mut esc = escenario(OpcionesEscenario {
        huella_conocida: false,
        ..opciones(&banco, 1, configuracion(0))
    })
    .await;
    let host = esc.hosts[0];
    esc.enviar(&MensajeCliente::PedirVistaPrevia {
        peticion_id: RANGO_TERMINAL + 1,
        host_id: host,
        servidor: ServidorTmux::Magi,
        nombre: "api".to_string(),
    })
    .await;
    let MensajeServidor::VistaPrevia { lineas, error, .. } = esc
        .esperar(|mensaje| matches!(mensaje, MensajeServidor::VistaPrevia { .. }))
        .await
    else {
        unreachable!()
    };
    assert!(lineas.is_empty());
    let error = error.expect("el host no se pudo abrir");
    assert!(error.contains("huella"), "{error}");
    // Sin diálogo: la huella no se pregunta desde una vista previa.
    esc.escuchar(Duration::from_millis(300)).await;
    assert!(!esc
        .vistos
        .iter()
        .any(|mensaje| matches!(mensaje, MensajeServidor::HuellaDesconocida { .. })));
}

// --------------------------------------------------------------- renombrar

async fn renombrar(
    esc: &mut Escenario,
    numero: u64,
    host_id: i64,
    nombre: &str,
    nuevo: &str,
) -> ResultadoRenombrar {
    esc.enviar(&MensajeCliente::RenombrarTmux {
        peticion_id: RANGO_TERMINAL + numero,
        host_id,
        nombre: nombre.to_string(),
        nuevo: nuevo.to_string(),
    })
    .await;
    let MensajeServidor::TmuxRenombrada { resultado, .. } = esc
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::TmuxRenombrada { peticion_id, .. } if *peticion_id == RANGO_TERMINAL + numero
            )
        })
        .await
    else {
        unreachable!()
    };
    resultado
}

#[tokio::test]
async fn renombrar_dice_ok_no_existe_u_ocupado_y_nunca_casa_por_prefijo() {
    let banco = Banco::nuevo();
    let mut esc = escenario(opciones(&banco, 1, configuracion(0))).await;
    let host = esc.hosts[0];
    banco.magi(
        1,
        &[
            linea("magi-a-1", 0, "", false, ""),
            linea("magi-a-10", 0, "", false, ""),
            linea("otra", 0, "", false, ""),
        ],
    );
    let ficha = crear_ficha(&esc, host, "magi-a-1");
    let fichas_antes = volcado(&esc, &["SESIONES_PERSISTENTES"]);

    // Ocupado: ya hay otra sesión con ese nombre.
    assert_eq!(
        renombrar(&mut esc, 1, host, "otra", "magi-a-10").await,
        ResultadoRenombrar::Ocupado
    );
    // Un prefijo no es la sesión: «magi-a» no existe aunque haya «magi-a-1».
    assert_eq!(
        renombrar(&mut esc, 2, host, "magi-a", "nueva").await,
        ResultadoRenombrar::NoExiste
    );
    assert_eq!(
        renombrar(&mut esc, 3, host, "no-existe", "nueva").await,
        ResultadoRenombrar::NoExiste
    );
    assert_eq!(banco.nombres_magi(1).len(), 3, "no se tocó nada");

    // Ok: se renombra exactamente `magi-a-1` y no `magi-a-10`.
    assert_eq!(
        renombrar(&mut esc, 4, host, "magi-a-1", "nueva").await,
        ResultadoRenombrar::Ok
    );
    let mut nombres = banco.nombres_magi(1);
    nombres.sort();
    assert_eq!(nombres, vec!["magi-a-10", "nueva", "otra"]);
    let llamadas = banco.llamadas(1);
    assert!(
        llamadas.contains("rename-session -t =magi-a-1 "),
        "no se apuntó con =nombre: {llamadas}"
    );
    assert!(!llamadas.contains("rename-session -t magi-a"));

    // Ya no existe con el nombre viejo, aunque «magi-a-10» empiece igual.
    assert_eq!(
        renombrar(&mut esc, 5, host, "magi-a-1", "otra-vez").await,
        ResultadoRenombrar::NoExiste
    );
    assert!(banco.nombres_magi(1).contains(&"magi-a-10".to_string()));

    // Nombres no válidos: error, sin llegar al host.
    let antes = banco.llamadas(1);
    for (numero, nombre, nuevo) in [
        (6, "nueva", "con espacio"),
        (7, "mal.nombre", "otro"),
        (8, "", "x"),
    ] {
        match renombrar(&mut esc, numero, host, nombre, nuevo).await {
            ResultadoRenombrar::Error(motivo) => assert!(!motivo.is_empty()),
            otro => panic!("{nombre:?} → {nuevo:?}: {otro:?}"),
        }
    }
    assert_eq!(banco.llamadas(1), antes);
    // Un host que no existe.
    assert!(matches!(
        renombrar(&mut esc, 9, 9999, "nueva", "otra").await,
        ResultadoRenombrar::Error(_)
    ));

    // El servidor no cambia la ficha: lo hace el cliente cuando recibe `Ok`.
    assert_eq!(volcado(&esc, &["SESIONES_PERSISTENTES"]), fichas_antes);
    let almacen = magi::almacen::abrir(&esc.rutas().base_datos()).unwrap();
    let nombre: String = almacen
        .conexion()
        .query_row(
            "SELECT nombre FROM SESIONES_PERSISTENTES WHERE id = ?1",
            [ficha],
            |fila| fila.get(0),
        )
        .unwrap();
    assert_eq!(nombre, "magi-a-1");
    almacen.cerrar().unwrap();
}

#[tokio::test]
async fn tras_renombrar_la_siguiente_comprobacion_ve_la_sesion_con_su_nombre_nuevo() {
    let banco = Banco::nuevo();
    let mut esc = escenario(opciones(&banco, 1, configuracion(0))).await;
    let host = esc.hosts[0];
    banco.magi(1, &[linea("api", 0, "", false, "")]);
    crear_ficha(&esc, host, "api");
    esc.enviar(&consulta(1, None)).await;
    esperar_lista(&mut esc, PLAZO, |lista| {
        lista
            .fichas
            .iter()
            .any(|ficha| ficha.estado == EstadoFicha::VivaLibre)
    })
    .await
    .expect("lista inicial");

    assert_eq!(
        renombrar(&mut esc, 2, host, "api", "api-v2").await,
        ResultadoRenombrar::Ok
    );
    // El cliente cambia su ficha tras el `Ok` y vuelve a consultar: el host,
    // comprobado hace menos de 5 s, se mira otra vez porque el renombrado lo
    // obliga, y la ficha nueva vive.
    let almacen = magi::almacen::abrir(&esc.rutas().base_datos()).unwrap();
    let ficha =
        magi_nucleo::almacen::sesiones_persistentes::por_nombre(almacen.conexion(), host, "api")
            .unwrap()
            .unwrap();
    magi_nucleo::almacen::sesiones_persistentes::renombrar(almacen.conexion(), ficha.id, "api-v2")
        .unwrap();
    almacen.cerrar().unwrap();
    esc.enviar(&consulta(3, None)).await;
    let lista = esperar_lista(&mut esc, PLAZO, |lista| {
        lista.fichas.iter().any(|ficha| ficha.nombre == "api-v2")
    })
    .await
    .expect("lista con la ficha renombrada");
    assert_eq!(lista.fichas[0].estado, EstadoFicha::VivaLibre);
    assert_eq!(
        banco.comprobaciones(1),
        2,
        "el renombrado fuerza otra mirada"
    );
    assert!(lista.hosts[0].sin_ficha.is_empty());
}

#[tokio::test]
async fn las_pestanas_enganchadas_siguen_enganchadas_con_el_nombre_nuevo() {
    use magi::protocolo::{CrearTmux, EstadoSesionRemota, PeticionTmux};
    let banco = Banco::nuevo();
    let mut esc = escenario(opciones(&banco, 1, configuracion(0))).await;
    let host = esc.hosts[0];
    banco.magi(1, &[linea("api", 0, "", false, "")]);

    // Una pestaña enganchada a «api» y otra al tmux habitual con el mismo
    // nombre (otra sesión, otro servidor).
    banco.habitual(1, &[linea("api", 0, "", false, "")]);
    let mut abiertas = Vec::new();
    for servidor in [ServidorTmux::Magi, ServidorTmux::Habitual] {
        let conocidas: std::collections::HashSet<u32> =
            esc.sesiones.iter().map(|sesion| sesion.id).collect();
        esc.enviar(&MensajeCliente::AbrirSesion {
            host_id: host,
            cols: 80,
            filas: 24,
            comandos_iniciales: Vec::new(),
            orden: None,
            tmux: Some(PeticionTmux {
                servidor,
                nombre: "api".to_string(),
                crear: None::<CrearTmux>,
                compartir: true,
            }),
        })
        .await;
        let fin = Instant::now() + PLAZO;
        let id = loop {
            if let Some(sesion) = esc.sesiones.iter().find(|sesion| {
                sesion.estado == EstadoSesionRemota::Abierta && !conocidas.contains(&sesion.id)
            }) {
                break sesion.id;
            }
            assert!(Instant::now() < fin, "la pestaña de {servidor:?} no abrió");
            esc.escuchar(Duration::from_millis(100)).await;
        };
        abiertas.push(id);
    }

    assert_eq!(
        renombrar(&mut esc, 1, host, "api", "api-v2").await,
        ResultadoRenombrar::Ok
    );
    // Cambia el nombre de la pestaña del tmux propio y solo el de esa.
    let nombre_de = |esc: &Escenario, id: u32| {
        esc.sesiones
            .iter()
            .find(|sesion| sesion.id == id)
            .and_then(|sesion| sesion.tmux.as_ref())
            .map(|tmux| tmux.nombre.clone())
    };
    assert!(
        esc.hasta(Duration::from_secs(5), |esc| {
            nombre_de(esc, abiertas[0]).as_deref() == Some("api-v2")
        })
        .await,
        "la pestaña enganchada no sigue enganchada con el nombre nuevo: {:?}",
        esc.sesiones
    );
    assert_eq!(nombre_de(&esc, abiertas[1]).as_deref(), Some("api"));
    for id in &abiertas {
        let sesion = esc.sesiones.iter().find(|sesion| sesion.id == *id).unwrap();
        assert_eq!(sesion.estado, EstadoSesionRemota::Abierta);
    }
}

// ---------------------------------------------------- lo que no se escribe

#[tokio::test]
async fn el_servidor_no_escribe_en_las_tablas_de_fichas_ni_en_el_registro() {
    let banco = Banco::nuevo();
    let mut esc = escenario(opciones(&banco, 2, configuracion(100))).await;
    let ids = esc.hosts.clone();
    banco.magi(1, &[linea("api", 0, "", false, "")]);
    banco.magi(
        2,
        &[linea("web", 1, "", true, "titulo secreto de la prueba")],
    );
    banco.pantalla(1, "api", "salida secreta de la prueba\n");
    crear_ficha(&esc, ids[0], "api");
    marcar_host(&esc, ids[1]);
    // Una caída también: el host 2 deja de contestar a mitad.
    esperar_lista(&mut esc, PLAZO, |lista| {
        lista.hosts.len() == 2 && lista.hosts.iter().all(|host| host.error.is_none())
    })
    .await
    .expect("lista");
    let tablas = ["SESIONES_PERSISTENTES", "HOSTS", "REGISTRO"];
    let antes = volcado(&esc, &tablas);

    // Pasadas, consultas, una vista previa y un renombrado fallido.
    esc.enviar(&consulta(1, None)).await;
    esc.enviar(&MensajeCliente::PedirVistaPrevia {
        peticion_id: RANGO_TERMINAL + 2,
        host_id: ids[0],
        servidor: ServidorTmux::Magi,
        nombre: "api".to_string(),
    })
    .await;
    esc.esperar(|mensaje| matches!(mensaje, MensajeServidor::VistaPrevia { .. }))
        .await;
    assert_eq!(
        renombrar(&mut esc, 3, ids[0], "no-existe", "otra").await,
        ResultadoRenombrar::NoExiste
    );
    esc.escuchar(Duration::from_millis(700)).await;

    assert_eq!(
        volcado(&esc, &tablas),
        antes,
        "el servidor escribió en fichas, hosts o registro"
    );
    // Ni la salida de las sesiones, ni sus títulos, ni las vistas, en el
    // registro (en el log lo vigila la revisión).
    let registro = volcado(&esc, &["REGISTRO"]);
    assert!(!registro.contains("secreta"));
    assert!(!registro.contains("titulo"));
}
