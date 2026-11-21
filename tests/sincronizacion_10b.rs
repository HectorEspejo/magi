//! Corrección 10b de la sincronización en el escritorio (Fase 9b):
//!
//! - R83: `magi sincronizar` pregunta al servidor de sesiones antes de aplicar
//!   borrados de hosts y túneles, y nunca lo arranca. Sin servidor, aplica; con
//!   el de esta versión, aplaza los de hosts con trabajo vivo; con otra versión o
//!   sin respuesta en el plazo, aplaza todos los de hosts y túneles. Un borrado
//!   aplazado no es un error (código 0) y la salida dice por qué.
//! - R97: el fichero rechazado con la clave anterior deja una línea en el log
//!   (una por proceso, sin su contenido).

mod comun;

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _};
use tokio::net::UnixListener;

use magi::almacen::{conflictos, Almacen};
use magi::config::{Config, Rutas};
use magi::llavero::{LlaveroEnMemoria, LlaveroSecreto, CUENTA_SINCRONIZACION};
use magi::modelo::{DatosHost, DatosTunel, Origen, TipoTunel};
use magi::protocolo::{codificar, MensajeServidor, VERSION_PROTOCOLO};
use magi::servidor::{
    consultar_trabajo, trabajo_por_host, ConsultaServidor, TrabajoDeHost, PLAZO_CONSULTA_SERVIDOR,
};
use magi::sincronizacion::cifrado::ClaveSincronizacion;
use magi::sincronizacion::cli::{self, SalidaOrden};
use magi::sincronizacion::formato::{Documento, EquipoDocumento, LapidaDocumento};
use magi::sincronizacion::{
    self, carpeta, formato, EstadoPasada, OpcionesPasada, PoliticaBorrados, MOTIVO_RECHAZADO,
};
use magi_nucleo::escritorio::{EstadoEscritorio, InfoEscritorio, TipoHost};

use comun::{escenario, OpcionesEscenario};

// ------------------------------------------------------------------ equipo

/// Un MAGI de escritorio de prueba: su magi.db, su config con la carpeta de
/// sincronización y su llavero en memoria con la clave.
struct Equipo {
    _temporal: tempfile::TempDir,
    rutas: Rutas,
    almacen: Almacen,
    config: Config,
    carpeta: PathBuf,
    clave_texto: String,
    llavero: LlaveroEnMemoria,
}

impl Equipo {
    fn nuevo(carpeta: &Path, clave_texto: &str) -> Self {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let raiz = temporal.path();
        let rutas = Rutas {
            datos: raiz.join("datos"),
            config: raiz.join("config"),
            estado: raiz.join("estado"),
            hogar: raiz.join("hogar"),
            runtime: raiz.join("runtime"),
        };
        std::fs::create_dir_all(rutas.dir_ssh()).expect("creando ~/.ssh");
        let mut config = Config::default();
        config.sincronizacion.carpeta = Some(carpeta.to_string_lossy().to_string());
        config.sincronizacion.copias = 5;
        // Umbrales altos: estas pruebas no van de borrados masivos.
        config.sincronizacion.umbral_borrado_hosts = 1000;
        config.sincronizacion.umbral_borrado_pct = 1000;
        let almacen = magi::almacen::abrir(&rutas.base_datos()).expect("creando magi.db");
        let llavero = LlaveroEnMemoria::nuevo();
        llavero
            .guardar(CUENTA_SINCRONIZACION, clave_texto)
            .expect("guardando la clave");
        Self {
            _temporal: temporal,
            rutas,
            almacen,
            config,
            carpeta: carpeta.to_path_buf(),
            clave_texto: clave_texto.to_string(),
            llavero,
        }
    }

    fn conexion(&self) -> &rusqlite::Connection {
        self.almacen.conexion()
    }

    fn clave(&self) -> ClaveSincronizacion {
        ClaveSincronizacion::desde_texto(&self.clave_texto).expect("clave de prueba")
    }

    /// Una pasada del escritorio, sin trabajo vivo (la de la TUI sin servidor).
    fn pasada(&self) {
        let resultado = sincronizacion::pasada(
            self.conexion(),
            &self.rutas,
            &self.config,
            &[self.clave()],
            &OpcionesPasada::sin_trabajo_vivo(PoliticaBorrados::Preguntar),
        )
        .expect("la pasada falla");
        assert_eq!(resultado.estado, EstadoPasada::AlDia);
    }

    fn crear_host(&self, nombre: &str) -> (i64, String) {
        let id = self
            .almacen
            .crear_host(
                &DatosHost {
                    nombre: nombre.to_string(),
                    direccion: "192.0.2.10".to_string(),
                    ..DatosHost::default()
                },
                Origen::Manual,
            )
            .expect("creando el host");
        (id, self.uuid_de("HOSTS", id))
    }

    fn crear_tunel(&self, host_id: i64, nombre: &str, puerto: u16) -> (i64, String) {
        let id = self
            .almacen
            .crear_tunel(&DatosTunel {
                host_id,
                nombre: nombre.to_string(),
                tipo: TipoTunel::Local,
                escucha: format!("127.0.0.1:{puerto}"),
                destino: Some("192.0.2.20:80".to_string()),
                automatico: false,
            })
            .expect("creando el túnel");
        (id, self.uuid_de("TUNELES", id))
    }

    fn uuid_de(&self, tabla: &str, id: i64) -> String {
        self.conexion()
            .query_row(
                &format!("SELECT uuid FROM {tabla} WHERE id = ?1"),
                [id],
                |fila| fila.get(0),
            )
            .expect("leyendo el uuid")
    }

    fn existe(&self, tabla: &str, nombre: &str) -> bool {
        self.conexion()
            .query_row(
                &format!("SELECT COUNT(*) FROM {tabla} WHERE nombre = ?1"),
                [nombre],
                |fila| fila.get::<_, i64>(0),
            )
            .expect("contando")
            > 0
    }

    fn aplazados(&self) -> Vec<(String, String)> {
        conflictos::listar_pendientes(self.conexion())
            .expect("listando conflictos")
            .into_iter()
            .filter(|conflicto| conflicto.tipo == conflictos::TIPO_BORRADO)
            .map(|conflicto| (conflicto.elemento_tipo, conflicto.nombre))
            .collect()
    }

    /// Lo que hace `magi sincronizar` con una respuesta del servidor dada.
    fn orden(&self, consulta: ConsultaServidor) -> SalidaOrden {
        cli::ejecutar_pasada(
            self.conexion(),
            &self.rutas,
            &self.config,
            Some(&self.llavero),
            || consulta,
        )
    }

    /// Escribe en la carpeta un fichero de otro equipo que borra los elementos
    /// dados `(tipo, uuid, nombre)`.
    fn borran_otros(&self, borrados: &[(&str, &str, &str)]) {
        escribir_lapidas(&self.carpeta, &self.clave(), borrados);
    }
}

fn clave_texto() -> String {
    ClaveSincronizacion::generar().texto().to_string()
}

fn carpeta_compartida() -> (tempfile::TempDir, PathBuf) {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let carpeta = temporal.path().join("sync");
    std::fs::create_dir_all(&carpeta).expect("creando la carpeta compartida");
    (temporal, carpeta)
}

/// Un fichero de otro equipo (desconocido para quien lo lee) con una lápida por
/// cada elemento dado. Lleva una versión más reciente que cualquier edición.
fn escribir_lapidas(carpeta: &Path, clave: &ClaveSincronizacion, borrados: &[(&str, &str, &str)]) {
    let equipo = uuid::Uuid::new_v4().to_string();
    let ms = chrono::Local::now().timestamp_millis() + 10_000;
    let version = format!("{ms:013}.{equipo}");
    let vector: BTreeMap<String, i64> = [(equipo.clone(), ms)].into_iter().collect();
    let documento = Documento {
        formato: formato::FORMATO_1,
        app: "1.6.1".to_string(),
        equipo: EquipoDocumento {
            uuid: equipo,
            nombre: "mac-mini".to_string(),
            plataforma: "macos".to_string(),
        },
        generado_en: "2026-11-05T09:12:44.120+01:00".to_string(),
        reloj_ms: ms,
        elementos: Vec::new(),
        lapidas: borrados
            .iter()
            .map(|(tipo, uuid, nombre)| LapidaDocumento {
                tipo: (*tipo).to_string(),
                uuid: (*uuid).to_string(),
                nombre: (*nombre).to_string(),
                version: version.clone(),
                vector: vector.clone(),
                borrado_en: "2026-11-05T09:00:00+01:00".to_string(),
            })
            .collect(),
        huellas: Vec::new(),
        equipos_quitados: Vec::new(),
    };
    let json = formato::serializar(&documento).expect("serializando");
    let firma = clave.firmar(json.as_bytes()).expect("firmando");
    let plano = formato::componer(&json, &firma, documento.formato);
    let cifrado = clave.cifrar(&plano).expect("cifrando");
    carpeta::escribir(carpeta, &documento.equipo.uuid, &cifrado).expect("escribiendo el fichero");
}

fn trabajo_con_sesion(hosts: &[i64]) -> ConsultaServidor {
    ConsultaServidor::Trabajo(
        hosts
            .iter()
            .map(|id| {
                (
                    *id,
                    TrabajoDeHost {
                        sesiones: 1,
                        ..TrabajoDeHost::default()
                    },
                )
            })
            .collect(),
    )
}

// ------------------------------------------------- la orden, con respuestas

#[test]
fn con_trabajo_vivo_en_el_host_el_borrado_se_aplaza_la_salida_dice_por_que_y_sale_con_cero() {
    let (_temporal, carpeta) = carpeta_compartida();
    let equipo = Equipo::nuevo(&carpeta, &clave_texto());
    let (web, uuid_web) = equipo.crear_host("web-01");
    let (_db, uuid_db) = equipo.crear_host("db-01");
    equipo.borran_otros(&[("host", &uuid_web, "web-01"), ("host", &uuid_db, "db-01")]);

    let salida = equipo.orden(trabajo_con_sesion(&[web]));

    assert_eq!(salida.codigo, 0, "un aplazado no es un error: {salida:?}");
    assert!(salida.errores.is_empty(), "{}", salida.errores);
    assert!(
        salida.salida.contains(
            "aplazado: borrar web-01 (tiene trabajo vivo en el servidor de sesiones: 1 sesión(es))"
        ),
        "{}",
        salida.salida
    );
    assert!(
        salida.salida.contains("1 borrado aplazado"),
        "resumen final: {}",
        salida.salida
    );
    assert!(
        equipo.existe("HOSTS", "web-01"),
        "el host con trabajo sigue"
    );
    assert!(
        !equipo.existe("HOSTS", "db-01"),
        "el que no tiene, se borra"
    );
    assert_eq!(
        equipo.aplazados(),
        vec![("host".to_string(), "web-01".to_string())],
        "queda como conflicto de borrado"
    );
    assert!(
        !salida.salida.contains("db-01"),
        "lo aplicado no es un aplazado: {}",
        salida.salida
    );
}

#[test]
fn sin_servidor_los_borrados_se_aplican() {
    let (_temporal, carpeta) = carpeta_compartida();
    let equipo = Equipo::nuevo(&carpeta, &clave_texto());
    let (_id, uuid_web) = equipo.crear_host("web-01");
    equipo.borran_otros(&[("host", &uuid_web, "web-01")]);

    let salida = equipo.orden(ConsultaServidor::Sin);

    assert_eq!(salida.codigo, 0, "{salida:?}");
    assert!(!equipo.existe("HOSTS", "web-01"));
    assert!(equipo.aplazados().is_empty());
    assert!(!salida.salida.contains("aplazado"), "{}", salida.salida);
    assert!(salida.salida.contains("sin avisos"), "{}", salida.salida);
}

#[test]
fn con_otra_version_o_sin_respuesta_se_aplazan_todos_los_de_hosts_y_tuneles() {
    let (_temporal, carpeta) = carpeta_compartida();
    let equipo = Equipo::nuevo(&carpeta, &clave_texto());
    let (web, uuid_web) = equipo.crear_host("web-01");
    let (_db, uuid_db) = equipo.crear_host("db-01");
    let (_tunel, uuid_tunel) = equipo.crear_tunel(web, "panel", 8080);
    equipo.borran_otros(&[
        ("host", &uuid_web, "web-01"),
        ("host", &uuid_db, "db-01"),
        ("tunel", &uuid_tunel, "panel"),
    ]);

    let salida = equipo.orden(ConsultaServidor::Fallo(
        "el servidor es de otra versión (1.5.0) que este magi (1.6.1)".to_string(),
    ));

    assert_eq!(salida.codigo, 0, "{salida:?}");
    assert!(
        salida.salida.contains(
            "aplazados los borrados de hosts y túneles: no se pudo preguntar al servidor"
        ),
        "{}",
        salida.salida
    );
    assert!(equipo.existe("HOSTS", "web-01"));
    assert!(equipo.existe("HOSTS", "db-01"), "hasta el host sin nada");
    assert!(equipo.existe("TUNELES", "panel"));
    assert_eq!(equipo.aplazados().len(), 3, "{:?}", equipo.aplazados());
    assert!(
        salida.salida.contains("3 borrados aplazados"),
        "{}",
        salida.salida
    );
}

#[test]
fn un_tunel_se_aplaza_si_su_host_tiene_trabajo_vivo_y_se_borra_si_no() {
    let (_temporal, carpeta) = carpeta_compartida();
    let equipo = Equipo::nuevo(&carpeta, &clave_texto());
    let (web, _) = equipo.crear_host("web-01");
    let (db, _) = equipo.crear_host("db-01");
    let (_t1, uuid_en_web) = equipo.crear_tunel(web, "panel", 8080);
    let (_t2, uuid_en_db) = equipo.crear_tunel(db, "consola", 8081);
    // Solo se borran los túneles; los hosts siguen.
    equipo.borran_otros(&[
        ("tunel", &uuid_en_web, "panel"),
        ("tunel", &uuid_en_db, "consola"),
    ]);

    let salida = equipo.orden(trabajo_con_sesion(&[web]));

    assert_eq!(salida.codigo, 0, "{salida:?}");
    assert!(
        salida.salida.contains(
            "aplazado: borrar el túnel «panel» (su host tiene trabajo vivo en el servidor de sesiones"
        ),
        "{}",
        salida.salida
    );
    assert!(equipo.existe("TUNELES", "panel"), "el de web-01 se aplaza");
    assert!(!equipo.existe("TUNELES", "consola"), "el de db-01 se borra");
}

#[test]
fn el_trabajo_de_otro_host_no_impide_borrar_este() {
    let (_temporal, carpeta) = carpeta_compartida();
    let equipo = Equipo::nuevo(&carpeta, &clave_texto());
    let (_web, uuid_web) = equipo.crear_host("web-01");
    let (db, _) = equipo.crear_host("db-01");
    equipo.borran_otros(&[("host", &uuid_web, "web-01")]);

    let salida = equipo.orden(trabajo_con_sesion(&[db]));

    assert_eq!(salida.codigo, 0, "{salida:?}");
    assert!(!equipo.existe("HOSTS", "web-01"));
    assert!(equipo.existe("HOSTS", "db-01"));
    assert!(equipo.aplazados().is_empty());
}

#[test]
fn la_orden_no_pregunta_al_servidor_si_no_hay_pasada_que_hacer() {
    let (_temporal, carpeta) = carpeta_compartida();
    // Sin carpeta configurada y sin llavero: no hay nada que decidir.
    let mut equipo = Equipo::nuevo(&carpeta, &clave_texto());
    equipo.config.sincronizacion.carpeta = None;
    let salida = cli::ejecutar_pasada(
        equipo.conexion(),
        &equipo.rutas,
        &equipo.config,
        None,
        || panic!("no debía consultar al servidor"),
    );
    assert_eq!(salida.codigo, 1);
    assert!(
        salida.errores.contains("no está configurada"),
        "{}",
        salida.errores
    );

    // Con carpeta pero sin clave en el llavero: tampoco.
    let equipo = Equipo::nuevo(&carpeta, &clave_texto());
    let vacio = LlaveroEnMemoria::nuevo();
    let salida = cli::ejecutar_pasada(
        equipo.conexion(),
        &equipo.rutas,
        &equipo.config,
        Some(&vacio),
        || panic!("no debía consultar al servidor"),
    );
    assert_eq!(salida.codigo, 1);
    assert!(
        salida.errores.contains("no hay clave en el llavero"),
        "{}",
        salida.errores
    );
}

#[test]
fn un_fichero_dañado_sale_como_aviso_y_la_orden_sigue_con_cero() {
    let (_temporal, carpeta) = carpeta_compartida();
    let equipo = Equipo::nuevo(&carpeta, &clave_texto());
    let roto = uuid::Uuid::new_v4().to_string();
    std::fs::write(
        carpeta.join(carpeta::nombre_fichero(&roto)),
        b"esto no es un fichero age",
    )
    .expect("fichero roto");

    let salida = equipo.orden(ConsultaServidor::Sin);

    assert_eq!(salida.codigo, 0, "{salida:?}");
    let aviso = format!(
        "  ! {}: a medio copiar o dañado; se reintenta en la siguiente pasada",
        carpeta::nombre_fichero(&roto)
    );
    assert!(salida.salida.contains(&aviso), "{}", salida.salida);
    assert!(salida.salida.contains("1 aviso"), "{}", salida.salida);
    assert!(!salida.salida.contains("1 avisos"), "{}", salida.salida);
}

// ------------------------------------------------------------ `estado`

#[test]
fn el_estado_lista_los_aplazados_con_su_motivo_segun_el_servidor() {
    let (_temporal, carpeta) = carpeta_compartida();
    let equipo = Equipo::nuevo(&carpeta, &clave_texto());
    let (web, uuid_web) = equipo.crear_host("web-01");
    equipo.borran_otros(&[("host", &uuid_web, "web-01")]);
    equipo.orden(trabajo_con_sesion(&[web]));
    assert_eq!(equipo.aplazados().len(), 1);

    let estado = |consulta: ConsultaServidor| -> String {
        cli::estado_con(
            equipo.conexion(),
            &equipo.config,
            &equipo.rutas.hogar,
            || consulta,
        )
        .expect("el estado")
        .salida
    };
    let con_trabajo = estado(trabajo_con_sesion(&[web]));
    assert!(
        con_trabajo.contains("borrados aplazados: 1"),
        "{con_trabajo}"
    );
    assert!(
        con_trabajo
            .contains("host «web-01»: tiene trabajo vivo en el servidor de sesiones: 1 sesión(es)"),
        "{con_trabajo}"
    );
    // Los aplazados van aparte: no salen en la lista de conflictos.
    let (conflictos_texto, _) = con_trabajo
        .split_once("borrados aplazados:")
        .expect("el bloque de aplazados");
    assert!(
        !conflictos_texto.contains("borrado "),
        "un borrado aplazado no cuenta como conflicto: {con_trabajo}"
    );

    let sin_servidor = estado(ConsultaServidor::Sin);
    assert!(
        sin_servidor.contains("host «web-01»: ya no tiene trabajo vivo"),
        "{sin_servidor}"
    );

    let sin_respuesta = estado(ConsultaServidor::Fallo(
        "el servidor no respondió en 5 s".into(),
    ));
    assert!(
        sin_respuesta
            .contains("no se pudo preguntar al servidor (el servidor no respondió en 5 s)"),
        "{sin_respuesta}"
    );
}

#[test]
fn el_estado_sin_aplazados_no_pregunta_al_servidor() {
    let (_temporal, carpeta) = carpeta_compartida();
    let equipo = Equipo::nuevo(&carpeta, &clave_texto());
    let salida = cli::estado_con(
        equipo.conexion(),
        &equipo.config,
        &equipo.rutas.hogar,
        || panic!("no debía consultar al servidor"),
    )
    .expect("el estado");
    assert_eq!(salida.codigo, 0);
    assert!(
        salida.salida.contains("borrados aplazados: 0"),
        "{}",
        salida.salida
    );
}

// ------------------------------------------------ consulta al servidor

/// Lo que hace un servidor de pruebas que atiende una sola conexión.
enum Falso {
    /// Rechaza el saludo como uno de otra versión de protocolo.
    VersionIncompatible,
    /// Acepta la conexión, lee el saludo y no contesta nunca.
    Calla,
    /// Contesta una Bienvenida de otra versión de la app.
    OtraVersion,
    /// Contesta algo que no es JSON.
    Basura,
    /// Se comporta como el servidor de esta versión, con estos escritorios.
    Correcto(Vec<InfoEscritorio>),
}

/// Pone un servidor falso en el socket de `rutas` (sin ningún servidor real).
fn servidor_falso(rutas: &Rutas, comportamiento: Falso) -> tokio::task::JoinHandle<()> {
    std::fs::create_dir_all(rutas.dir_runtime()).expect("directorio de runtime");
    let escucha = UnixListener::bind(magi::servidor::ruta_socket(rutas)).expect("socket falso");
    tokio::spawn(async move {
        let (flujo, _) = escucha.accept().await.expect("conexión de la orden");
        let (lectura, mut escritura) = flujo.into_split();
        let mut lector = tokio::io::BufReader::new(lectura);
        let mut saludo = String::new();
        let _ = lector.read_line(&mut saludo).await;
        let bienvenida = |version_app: &str| MensajeServidor::Bienvenida {
            version: VERSION_PROTOCOLO,
            version_app: version_app.to_string(),
            pid: 4242,
            cliente_id: 1,
            clientes: 1,
            sesiones: Vec::new(),
            transferencias: Vec::new(),
            tuneles: Vec::new(),
            ejecuciones: Vec::new(),
            acciones_servicio: Vec::new(),
            tuneles_temporales: Vec::new(),
        };
        let enviar = |mensaje: &MensajeServidor| {
            let mut linea = codificar(mensaje).expect("codificando");
            linea.push('\n');
            linea
        };
        match comportamiento {
            Falso::VersionIncompatible => {
                let linea = enviar(&MensajeServidor::VersionIncompatible {
                    version: 99,
                    pid: None,
                    version_app: Some("9.9.9".to_string()),
                    clientes: Some(0),
                    trabajo: None,
                });
                escritura
                    .write_all(linea.as_bytes())
                    .await
                    .expect("rechazo");
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
            Falso::Calla => tokio::time::sleep(Duration::from_secs(30)).await,
            Falso::OtraVersion => {
                let linea = enviar(&bienvenida("0.0.1"));
                escritura
                    .write_all(linea.as_bytes())
                    .await
                    .expect("bienvenida");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
            Falso::Basura => {
                escritura
                    .write_all(b"esto no es json\n")
                    .await
                    .expect("basura");
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
            Falso::Correcto(escritorios) => {
                let linea = enviar(&bienvenida(env!("CARGO_PKG_VERSION")));
                escritura
                    .write_all(linea.as_bytes())
                    .await
                    .expect("bienvenida");
                if !escritorios.is_empty() {
                    let linea = enviar(&MensajeServidor::Escritorios { lista: escritorios });
                    escritura
                        .write_all(linea.as_bytes())
                        .await
                        .expect("escritorios");
                }
                // Responde a `Listar` con la lista de sesiones, como el real.
                let mut pedido = String::new();
                let _ = lector.read_line(&mut pedido).await;
                assert!(pedido.contains("Listar"), "se esperaba Listar: {pedido}");
                let linea = enviar(&MensajeServidor::Sesiones { lista: Vec::new() });
                escritura
                    .write_all(linea.as_bytes())
                    .await
                    .expect("sesiones");
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        }
    })
}

fn rutas_aisladas() -> (tempfile::TempDir, Rutas) {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let raiz = temporal.path().to_path_buf();
    let rutas = Rutas {
        datos: raiz.join("datos"),
        config: raiz.join("config"),
        estado: raiz.join("estado"),
        hogar: raiz.join("hogar"),
        runtime: raiz.join("runtime"),
    };
    (temporal, rutas)
}

fn escritorio(host_id: i64) -> InfoEscritorio {
    InfoEscritorio {
        id: 1,
        host_id,
        nombre: "oficina".to_string(),
        tipo: TipoHost::Rdp,
        estado: EstadoEscritorio::Abierto,
        fase: None,
        motivo: None,
        visor: true,
        sin_visor_desde: None,
        ancho: 1280,
        alto: 720,
        imagen: None,
        fps: 30,
        directo: true,
        a_traves_de: None,
        desde: 0,
        redimensionable: true,
    }
}

#[tokio::test]
async fn sin_servidor_la_consulta_dice_sin_servidor_y_no_lo_arranca() {
    let (_temporal, rutas) = rutas_aisladas();
    let consulta = consultar_trabajo(&rutas, Duration::from_secs(5)).await;
    assert_eq!(consulta, ConsultaServidor::Sin);
    // Nunca arranca el servidor: ni socket, ni lock, ni log.
    assert!(!magi::servidor::ruta_socket(&rutas).exists());
    assert!(!magi::servidor::ruta_lock(&rutas).exists());
    assert!(!rutas.dir_logs().exists());
}

#[tokio::test]
async fn un_socket_que_nadie_atiende_cuenta_como_sin_servidor() {
    let (_temporal, rutas) = rutas_aisladas();
    std::fs::create_dir_all(rutas.dir_runtime()).expect("runtime");
    // Un servidor que murió sin limpiar: el fichero del socket sigue ahí.
    drop(UnixListener::bind(magi::servidor::ruta_socket(&rutas)).expect("socket"));
    let consulta = consultar_trabajo(&rutas, Duration::from_secs(5)).await;
    assert_eq!(consulta, ConsultaServidor::Sin);
}

#[tokio::test]
async fn un_servidor_de_otra_version_de_protocolo_es_un_fallo_no_un_sin_trabajo() {
    let (_temporal, rutas) = rutas_aisladas();
    let falso = servidor_falso(&rutas, Falso::VersionIncompatible);
    let consulta = consultar_trabajo(&rutas, Duration::from_secs(5)).await;
    match consulta {
        ConsultaServidor::Fallo(motivo) => {
            assert!(motivo.contains("otra versión"), "{motivo}");
            assert!(motivo.contains("9.9.9"), "{motivo}");
        }
        otra => panic!("se esperaba Fallo y llegó {otra:?}"),
    }
    let _ = falso.await;
}

#[tokio::test]
async fn un_servidor_de_otra_version_de_la_app_es_un_fallo() {
    let (_temporal, rutas) = rutas_aisladas();
    let _falso = servidor_falso(&rutas, Falso::OtraVersion);
    match consultar_trabajo(&rutas, Duration::from_secs(5)).await {
        ConsultaServidor::Fallo(motivo) => assert!(motivo.contains("0.0.1"), "{motivo}"),
        otra => panic!("se esperaba Fallo y llegó {otra:?}"),
    }
}

#[tokio::test]
async fn un_servidor_que_acepta_y_calla_es_un_fallo_dentro_del_plazo() {
    let (_temporal, rutas) = rutas_aisladas();
    let _falso = servidor_falso(&rutas, Falso::Calla);
    let inicio = Instant::now();
    let consulta = consultar_trabajo(&rutas, Duration::from_millis(300)).await;
    let transcurrido = inicio.elapsed();
    match consulta {
        ConsultaServidor::Fallo(motivo) => {
            assert!(motivo.contains("no respondió en 300 ms"), "{motivo}")
        }
        otra => panic!("se esperaba Fallo y llegó {otra:?}"),
    }
    assert!(
        transcurrido < Duration::from_secs(3),
        "no espera más que el plazo: {transcurrido:?}"
    );
}

#[tokio::test]
async fn una_respuesta_que_no_se_entiende_es_un_fallo() {
    let (_temporal, rutas) = rutas_aisladas();
    let _falso = servidor_falso(&rutas, Falso::Basura);
    match consultar_trabajo(&rutas, Duration::from_secs(5)).await {
        ConsultaServidor::Fallo(motivo) => assert!(motivo.contains("no válida"), "{motivo}"),
        otra => panic!("se esperaba Fallo y llegó {otra:?}"),
    }
}

#[tokio::test]
async fn la_consulta_del_servidor_de_esta_version_cuenta_los_escritorios_sin_esperar_un_tiempo_fijo(
) {
    // Con un escritorio abierto en el host 7: la lista llega tras la Bienvenida.
    let (_temporal, rutas) = rutas_aisladas();
    let _falso = servidor_falso(&rutas, Falso::Correcto(vec![escritorio(7)]));
    match consultar_trabajo(&rutas, Duration::from_secs(5)).await {
        ConsultaServidor::Trabajo(hosts) => {
            assert_eq!(hosts.len(), 1, "{hosts:?}");
            assert_eq!(hosts[&7].escritorios, 1);
        }
        otra => panic!("se esperaba Trabajo y llegó {otra:?}"),
    }

    // Sin escritorios el servidor no manda la lista: no se espera por ella. Con
    // la espera fija de `magi servidor estado` (1 s) este plazo no bastaría.
    let (_temporal, rutas) = rutas_aisladas();
    let _falso = servidor_falso(&rutas, Falso::Correcto(Vec::new()));
    let inicio = Instant::now();
    match consultar_trabajo(&rutas, Duration::from_secs(5)).await {
        ConsultaServidor::Trabajo(hosts) => assert!(hosts.is_empty(), "{hosts:?}"),
        otra => panic!("se esperaba Trabajo y llegó {otra:?}"),
    }
    assert!(
        inicio.elapsed() < Duration::from_millis(800),
        "la consulta tarda de más: {:?}",
        inicio.elapsed()
    );
}

#[test]
fn el_trabajo_se_agrupa_por_host_con_el_criterio_de_la_tui() {
    use magi::protocolo::{
        EstadoEjecucion, EstadoSesionRemota, EstadoTransferencia, EstadoTunelRemoto, InfoEjecucion,
        InfoEjecucionHost, InfoSesion, InfoTransferencia, InfoTunel, InfoTunelTemporal,
    };
    let sesion = |id: u32, host_id: i64, estado: EstadoSesionRemota| InfoSesion {
        id,
        nombre: "shell".to_string(),
        host_id,
        host_nombre: "web-01".to_string(),
        estado,
        motivo: None,
        identidad: String::new(),
        abierta_en: 0,
        ventanas: 1,
        actividad_no_vista: false,
        reconexion: None,
        titulo: None,
        aviso: None,
        tmux: None,
        escritura_varios: false,
    };
    let transferencia = |host_id: i64, estado: EstadoTransferencia| InfoTransferencia {
        id: 1,
        host_id,
        host_nombre: "web-01".to_string(),
        direccion: magi::protocolo::Direccion::Subida,
        estado,
        origen: "/home/usuario/a".to_string(),
        destino: "/srv/a".to_string(),
        es_directorio: false,
        ficheros_total: 1,
        ficheros_hechos: 0,
        omitidos: 0,
        bytes_total: 10,
        bytes_hechos: 0,
        fichero_actual: None,
        error: None,
        borrar_origen: false,
        solicitante: 1,
        creada_en: 0,
        terminada_en: None,
        peticion_id: None,
        etiqueta: None,
        borrados: 0,
        borrados_total: 0,
    };
    let tunel = |host_id: i64, estado: EstadoTunelRemoto| InfoTunel {
        tunel_id: 1,
        host_id,
        host_nombre: "web-01".to_string(),
        nombre: "panel".to_string(),
        tipo: "local".to_string(),
        escucha: "127.0.0.1:8080".to_string(),
        escucha_efectiva: "127.0.0.1:8080".to_string(),
        destino: None,
        automatico: false,
        estado,
        origen: None,
        solicitante: None,
        conexiones: 0,
        aceptadas: 0,
        bytes_subidos: 0,
        bytes_bajados: 0,
        desde: None,
        ultimo_error: None,
    };
    let ejecucion = |estado: EstadoEjecucion, hosts: &[i64]| InfoEjecucion {
        id: 1,
        peticion_id: 1,
        solicitante: 1,
        snippet_id: None,
        nombre: "estado".to_string(),
        hosts: hosts
            .iter()
            .map(|host_id| InfoEjecucionHost {
                host_id: *host_id,
                nombre: "web".to_string(),
                estado: magi::protocolo::EstadoHostEjecucion::Ejecutando,
                codigo: None,
                inicio_ms: None,
                duracion_ms: None,
                bytes_stdout: 0,
                bytes_stderr: 0,
                truncada: false,
                error: None,
            })
            .collect(),
        timeout_seg: 60,
        parar_al_fallo: false,
        deliberacion_id: None,
        forzada: false,
        estado,
        creada_en: 0,
        terminada_en: None,
    };
    let temporal = |host_id: i64| InfoTunelTemporal {
        id: 1,
        sesion_id: 1,
        host_id,
        host_nombre: "web-01".to_string(),
        puerto_local: 40000,
        destino: "localhost".to_string(),
        puerto: 3000,
        conexiones: 0,
    };

    let hosts = trabajo_por_host(
        &[
            sesion(1, 1, EstadoSesionRemota::Abierta),
            sesion(2, 1, EstadoSesionRemota::Abriendo),
            sesion(3, 2, EstadoSesionRemota::Caida),
            sesion(4, 2, EstadoSesionRemota::Cerrada),
        ],
        &[
            transferencia(3, EstadoTransferencia::EnCola),
            transferencia(3, EstadoTransferencia::EnCurso),
            transferencia(4, EstadoTransferencia::Hecha),
            transferencia(4, EstadoTransferencia::Error),
        ],
        &[
            tunel(5, EstadoTunelRemoto::Activo),
            tunel(6, EstadoTunelRemoto::Inactivo),
            tunel(6, EstadoTunelRemoto::Caido),
            tunel(8, EstadoTunelRemoto::Parando),
        ],
        &[
            ejecucion(EstadoEjecucion::EnCurso, &[9, 9]),
            ejecucion(EstadoEjecucion::Terminada, &[10]),
        ],
        &[],
        &[temporal(11)],
        &[escritorio(12)],
    );
    let con_trabajo: Vec<i64> = hosts.keys().copied().collect();
    assert_eq!(con_trabajo, vec![1, 3, 5, 8, 9, 11, 12], "{hosts:?}");
    assert_eq!(hosts[&1].sesiones, 2);
    assert_eq!(hosts[&3].transferencias, 2);
    assert_eq!(hosts[&9].ejecuciones, 1, "un host repetido cuenta una vez");
    assert_eq!(hosts[&11].tuneles, 1);
    assert_eq!(hosts[&12].escritorios, 1);
    assert_eq!(hosts[&1].describir(), "2 sesión(es)");
}

// ------------------------------------------------- con el servidor de verdad

#[test]
fn sin_servidor_la_orden_completa_aplica_el_borrado_y_no_arranca_ninguno() {
    let (_temporal, carpeta) = carpeta_compartida();
    let equipo = Equipo::nuevo(&carpeta, &clave_texto());
    let (_id, uuid_web) = equipo.crear_host("web-01");
    equipo.borran_otros(&[("host", &uuid_web, "web-01")]);

    let salida = cli::sincronizar(
        equipo.conexion(),
        &equipo.rutas,
        &equipo.config,
        Some(&equipo.llavero),
        PLAZO_CONSULTA_SERVIDOR,
    );

    assert_eq!(salida.codigo, 0, "{salida:?}");
    assert!(!equipo.existe("HOSTS", "web-01"), "sin servidor se aplica");
    assert!(
        !magi::servidor::ruta_socket(&equipo.rutas).exists(),
        "la orden no arranca el servidor"
    );
    assert!(!magi::servidor::ruta_lock(&equipo.rutas).exists());
}

/// La orden completa frente a un servidor falso que no sabe decir qué custodia:
/// aplaza el borrado, sale con cero y no espera más que el plazo.
fn comprobar_orden_completa_con(falso: Falso, nombre: &str) {
    let runtime = tokio::runtime::Runtime::new().expect("runtime de la prueba");
    let (_temporal, carpeta) = carpeta_compartida();
    let equipo = Equipo::nuevo(&carpeta, &clave_texto());
    let (_id, uuid_web) = equipo.crear_host("web-01");
    equipo.borran_otros(&[("host", &uuid_web, "web-01")]);
    // El servidor falso vive en el runtime de la prueba; la orden, síncrona,
    // levanta el suyo, como en `main`.
    let _falso = {
        let _entrada = runtime.enter();
        servidor_falso(&equipo.rutas, falso)
    };

    let inicio = Instant::now();
    let salida = cli::sincronizar(
        equipo.conexion(),
        &equipo.rutas,
        &equipo.config,
        Some(&equipo.llavero),
        Duration::from_millis(300),
    );

    assert!(
        inicio.elapsed() < Duration::from_secs(5),
        "{nombre}: {:?}",
        inicio.elapsed()
    );
    assert_eq!(salida.codigo, 0, "{nombre}: {salida:?}");
    assert!(
        equipo.existe("HOSTS", "web-01"),
        "{nombre}: sin saber lo que hay en marcha se aplaza"
    );
    assert!(
        salida.salida.contains(
            "aplazados los borrados de hosts y túneles: no se pudo preguntar al servidor"
        ),
        "{nombre}: {}",
        salida.salida
    );
    assert_eq!(equipo.aplazados().len(), 1, "{nombre}");
}

#[test]
fn la_orden_completa_con_un_servidor_de_otra_version_aplaza_y_sale_con_cero() {
    comprobar_orden_completa_con(Falso::VersionIncompatible, "otra versión de protocolo");
    comprobar_orden_completa_con(Falso::OtraVersion, "otra versión de la app");
}

#[test]
fn la_orden_completa_con_un_servidor_que_no_responde_aplaza_y_no_espera_mas_que_el_plazo() {
    comprobar_orden_completa_con(Falso::Calla, "servidor que calla");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn con_una_pestana_abierta_en_el_host_el_borrado_que_llega_por_la_sincronizacion_se_aplaza() {
    // Criterio de aceptación de R83: un borrado de un host que llega por la
    // sincronización, con una pestaña abierta en él, queda aplazado, la salida
    // dice por qué y el código de salida es 0. El servidor es el de verdad.
    let mut esc = escenario(OpcionesEscenario {
        hosts: 2,
        sftp: false,
        ..OpcionesEscenario::default()
    })
    .await;
    let con_pestana = esc.hosts[0];
    let sin_pestana = esc.hosts[1];
    esc.abrir_sesion(con_pestana).await;

    let rutas = esc.rutas().clone();
    let mut config = esc.entorno.config.clone();
    let (_temporal, carpeta) = carpeta_compartida();
    config.sincronizacion.carpeta = Some(carpeta.to_string_lossy().to_string());
    config.sincronizacion.umbral_borrado_hosts = 1000;
    config.sincronizacion.umbral_borrado_pct = 1000;
    let clave = ClaveSincronizacion::generar();
    let llavero = LlaveroEnMemoria::nuevo();
    llavero
        .guardar(CUENTA_SINCRONIZACION, &clave.texto())
        .expect("clave en el llavero");
    let uuid = |id: i64| -> String {
        let almacen = magi::almacen::abrir(&rutas.base_datos()).expect("abriendo magi.db");
        let uuid = almacen
            .conexion()
            .query_row("SELECT uuid FROM HOSTS WHERE id = ?1", [id], |fila| {
                fila.get(0)
            })
            .expect("uuid");
        uuid
    };
    let uuid_con = uuid(con_pestana);
    let uuid_sin = uuid(sin_pestana);
    escribir_lapidas(
        &carpeta,
        &clave,
        &[
            ("host", &uuid_con, "prueba-1"),
            ("host", &uuid_sin, "prueba-2"),
        ],
    );

    let salida = {
        let rutas = rutas.clone();
        let config = config.clone();
        tokio::task::spawn_blocking(move || {
            let almacen = magi::almacen::abrir(&rutas.base_datos()).expect("abriendo magi.db");
            let salida = cli::sincronizar(
                almacen.conexion(),
                &rutas,
                &config,
                Some(&llavero),
                PLAZO_CONSULTA_SERVIDOR,
            );
            drop(almacen);
            salida
        })
        .await
        .expect("la orden")
    };

    assert_eq!(salida.codigo, 0, "{salida:?}");
    assert!(
        salida.salida.contains(
            "aplazado: borrar prueba-1 (tiene trabajo vivo en el servidor de sesiones: 1 sesión(es))"
        ),
        "la salida dice por qué: {}",
        salida.salida
    );
    assert!(
        !salida.salida.contains("borrar prueba-2"),
        "el otro host se borra: {}",
        salida.salida
    );
    let almacen = magi::almacen::abrir(&rutas.base_datos()).expect("abriendo magi.db");
    let nombres: Vec<String> = almacen
        .listar_hosts()
        .expect("hosts")
        .into_iter()
        .map(|host| host.nombre)
        .collect();
    assert_eq!(nombres, vec!["prueba-1".to_string()], "{nombres:?}");
    drop(almacen);

    // `magi sincronizar estado` lo lista con su motivo, preguntando al servidor.
    let texto = {
        let rutas = rutas.clone();
        tokio::task::spawn_blocking(move || {
            let almacen = magi::almacen::abrir(&rutas.base_datos()).expect("abriendo magi.db");
            let salida = cli::estado(almacen.conexion(), &rutas, &config, PLAZO_CONSULTA_SERVIDOR)
                .expect("el estado");
            drop(almacen);
            salida.salida
        })
        .await
        .expect("el estado")
    };
    assert!(texto.contains("borrados aplazados: 1"), "{texto}");
    assert!(
        texto.contains(
            "host «prueba-1»: tiene trabajo vivo en el servidor de sesiones: 1 sesión(es)"
        ),
        "{texto}"
    );
}

// ------------------------------------------------------------------- R97

/// Escritor que guarda lo que se loguea.
#[derive(Clone, Default)]
struct Captura(Arc<Mutex<Vec<u8>>>);

impl io::Write for Captura {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Ejecuta `cuerpo` con un suscriptor propio (solo de este hilo) y devuelve las
/// líneas de log que dejó.
fn con_log<T>(cuerpo: impl FnOnce() -> T) -> (T, Vec<String>) {
    let captura = Captura::default();
    let escritor = captura.clone();
    let suscriptor = tracing_subscriber::fmt()
        .with_writer(move || escritor.clone())
        .with_ansi(false)
        .finish();
    let resultado = tracing::subscriber::with_default(suscriptor, cuerpo);
    let texto = String::from_utf8(captura.0.lock().unwrap().clone()).expect("log en UTF-8");
    (resultado, texto.lines().map(str::to_string).collect())
}

#[test]
fn el_fichero_rechazado_con_la_clave_anterior_deja_una_linea_en_el_log_sin_su_contenido() {
    let (_temporal, carpeta) = carpeta_compartida();
    let nueva = clave_texto();
    let vieja = clave_texto();
    let este = Equipo::nuevo(&carpeta, &nueva);

    // Un equipo desconocido escribe con la clave anterior un host llamado de
    // forma reconocible; el contenido no debe aparecer en ningún sitio.
    let intruso = Equipo::nuevo(&carpeta, &vieja);
    intruso.crear_host("web-secreto-del-intruso");
    intruso.pasada();
    let uuid_intruso = magi::almacen::equipos::este(intruso.conexion())
        .expect("equipo")
        .uuid;
    let nombre_fichero = carpeta::nombre_fichero(&uuid_intruso);

    let claves = || {
        vec![
            ClaveSincronizacion::desde_texto(&nueva).expect("clave nueva"),
            ClaveSincronizacion::desde_texto(&vieja).expect("clave anterior"),
        ]
    };
    let pasar = || {
        sincronizacion::pasada(
            este.conexion(),
            &este.rutas,
            &este.config,
            &claves(),
            &OpcionesPasada::sin_trabajo_vivo(PoliticaBorrados::Preguntar),
        )
        .expect("la pasada falla")
    };
    let ((primera, segunda), lineas) = con_log(|| (pasar(), pasar()));

    assert_eq!(primera.estado, EstadoPasada::AlDia, "la pasada sigue");
    assert_eq!(primera.rechazados, vec![nombre_fichero.clone()]);
    assert!(
        primera
            .avisos
            .iter()
            .any(|aviso| aviso == &format!("{nombre_fichero}: rechazado: {MOTIVO_RECHAZADO}")),
        "{:?}",
        primera.avisos
    );
    assert_eq!(
        segunda.rechazados,
        vec![nombre_fichero.clone()],
        "se rechaza en cada pasada (no guarda hash)"
    );
    assert!(!este.existe("HOSTS", "web-secreto-del-intruso"));
    let del_fichero: Vec<&String> = lineas
        .iter()
        .filter(|linea| linea.contains(&nombre_fichero))
        .collect();
    assert_eq!(
        del_fichero.len(),
        1,
        "una línea por proceso, no una por pasada: {lineas:?}"
    );
    assert!(del_fichero[0].contains("rechazado"), "{}", del_fichero[0]);
    assert!(
        lineas
            .iter()
            .all(|linea| !linea.contains("web-secreto-del-intruso")),
        "el log no lleva el contenido: {lineas:?}"
    );
}
