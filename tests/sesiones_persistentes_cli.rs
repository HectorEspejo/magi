//! La CLI de las sesiones persistentes (Fase 16, §5.5): `magi sesion` y
//! `magi sesiones`.
//!
//! Lo que no necesita servidor (nombres que no existen, ambiguos o con
//! `--host`, `--json` sin fichas) corre contra el binario con un entorno
//! aislado. Lo que sí lo necesita corre contra el binario y un servidor de
//! sesiones en proceso con el servidor SSH de pruebas de `tests/comun` (sin red
//! externa) y el tmux falso de `tests/fijos/terminal`; un host cuyo puerto está
//! cerrado hace de «no responde».

mod comun;

use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;
use std::process::Output;
use std::sync::Arc;
use std::time::Duration;

use magi::config::{Config, Rutas};
use magi::modelo::{DatosHost, IdentidadRef, Origen};
use magi_nucleo::sesiones_persistentes::{DatosFicha, OrigenFicha};

use comun::{ModoReenvio, Observado, Reenvios, ServidorSesion};

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

/// Un entorno de MAGI aislado en un directorio temporal, con las rutas que
/// resuelve el binario a partir de las variables `XDG_*`.
struct Aislado {
    _temporal: tempfile::TempDir,
    raiz: PathBuf,
    rutas: Rutas,
}

impl Aislado {
    fn nuevo() -> Self {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let raiz = temporal.path().to_path_buf();
        for sub in ["datos", "config", "estado", "hogar", "runtime"] {
            std::fs::create_dir_all(raiz.join(sub)).unwrap();
        }
        // El runtime del servidor debe ser privado, como en un sistema real.
        std::fs::set_permissions(raiz.join("runtime"), std::fs::Permissions::from_mode(0o700))
            .unwrap();
        let rutas = Rutas {
            datos: raiz.join("datos").join("magi"),
            config: raiz.join("config").join("magi"),
            estado: raiz.join("estado").join("magi"),
            hogar: raiz.join("hogar"),
            runtime: raiz.join("runtime").join("magi"),
        };
        Self {
            _temporal: temporal,
            raiz,
            rutas,
        }
    }

    /// `magi <argumentos>` con este entorno y nada más.
    fn comando(&self, argumentos: &[&str]) -> std::process::Command {
        let mut comando = std::process::Command::new(env!("CARGO_BIN_EXE_magi"));
        comando
            .args(argumentos)
            .env_clear()
            .env("HOME", self.raiz.join("hogar"))
            .env("XDG_DATA_HOME", self.raiz.join("datos"))
            .env("XDG_CONFIG_HOME", self.raiz.join("config"))
            .env("XDG_STATE_HOME", self.raiz.join("estado"))
            .env("XDG_RUNTIME_DIR", self.raiz.join("runtime"))
            .stdin(std::process::Stdio::null());
        comando
    }

    fn ejecutar(&self, argumentos: &[&str]) -> Output {
        self.comando(argumentos)
            .output()
            .expect("ejecutando el binario magi")
    }

    /// Lo mismo desde un runtime de tokio, sin bloquear el servidor en proceso.
    async fn ejecutar_async(&self, argumentos: &[&str]) -> Output {
        let mut comando = tokio::process::Command::new(env!("CARGO_BIN_EXE_magi"));
        comando
            .args(argumentos)
            .env_clear()
            .env("HOME", self.raiz.join("hogar"))
            .env("XDG_DATA_HOME", self.raiz.join("datos"))
            .env("XDG_CONFIG_HOME", self.raiz.join("config"))
            .env("XDG_STATE_HOME", self.raiz.join("estado"))
            .env("XDG_RUNTIME_DIR", self.raiz.join("runtime"))
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);
        comando.output().await.expect("ejecutando el binario magi")
    }

    /// Crea un host SSH en el inventario y devuelve su id.
    fn host(&self, nombre: &str, direccion: &str, puerto: u16, identidad: IdentidadRef) -> i64 {
        let almacen = magi::almacen::abrir(&self.rutas.base_datos()).unwrap();
        let id = magi::almacen::hosts::crear(
            almacen.conexion(),
            &DatosHost {
                nombre: nombre.to_string(),
                direccion: direccion.to_string(),
                puerto,
                usuario: Some("usuario".to_string()),
                identidad_ref: identidad,
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .unwrap();
        almacen.cerrar().unwrap();
        id
    }

    /// Crea una ficha de sesión persistente y devuelve su uuid.
    fn ficha(&self, host_id: i64, nombre: &str) -> String {
        let almacen = magi::almacen::abrir(&self.rutas.base_datos()).unwrap();
        let id = magi_nucleo::almacen::sesiones_persistentes::crear(
            almacen.conexion(),
            host_id,
            &DatosFicha {
                nombre: nombre.to_string(),
                carpeta: None,
                comando: Some("claude --secreto-de-la-prueba".to_string()),
                comando_reanudar: None,
                nota: Some("nota privada de la prueba".to_string()),
                origen: OrigenFicha::Manual,
                creada_por: "portatil".to_string(),
            },
        )
        .unwrap();
        let uuid = magi_nucleo::almacen::sesiones_persistentes::obtener(almacen.conexion(), id)
            .unwrap()
            .unwrap()
            .uuid;
        almacen.cerrar().unwrap();
        uuid
    }

    /// Para un servidor lanzado por la propia orden (autolanzado).
    fn parar_servidor(&self) {
        let _ = self.ejecutar(&["servidor", "parar", "--si"]);
    }
}

/// Para el servidor autolanzado al terminar la prueba, aunque falle.
struct ParaAlSalir<'a>(&'a Aislado);

impl Drop for ParaAlSalir<'_> {
    fn drop(&mut self) {
        self.0.parar_servidor();
    }
}

fn texto(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).to_string()
}

fn ahora() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Dos hosts con dos fichas de nombre `agente`, una `agente-api` y una `build`.
fn inventario_con_nombres_repetidos(entorno: &Aislado) {
    let web = entorno.host("web-01", "192.0.2.10", 22, IdentidadRef::Auto);
    let db = entorno.host("db-01", "192.0.2.11", 22, IdentidadRef::Auto);
    entorno.ficha(web, "agente");
    entorno.ficha(db, "agente");
    entorno.ficha(web, "agente-api");
    entorno.ficha(db, "build");
}

// ------------------------------------------------------------ magi sesion

#[test]
fn magi_sesion_con_un_nombre_que_no_existe_lo_dice_y_sugiere_las_parecidas() {
    let entorno = Aislado::nuevo();
    inventario_con_nombres_repetidos(&entorno);
    let salida = entorno.ejecutar(&["sesion", "buil"]);
    assert_eq!(salida.status.code(), Some(1), "{salida:?}");
    let error = texto(&salida.stderr);
    assert!(
        error.contains("no hay ninguna sesión persistente «buil»"),
        "{error}"
    );
    assert!(error.contains("build (en db-01)"), "{error}");
    assert!(error.contains("magi sesiones"), "{error}");
    assert_eq!(texto(&salida.stdout), "", "no abre nada ni imprime nada");

    // Un nombre sin nada parecido tampoco inventa sugerencias.
    let salida = entorno.ejecutar(&["sesion", "zzzzzzzz"]);
    assert_eq!(salida.status.code(), Some(1));
    let error = texto(&salida.stderr);
    assert!(error.contains("«zzzzzzzz»"), "{error}");
    assert!(!error.contains("Parecidas"), "{error}");
}

#[test]
fn magi_sesion_con_un_nombre_en_dos_hosts_pide_host_y_dice_cuales() {
    let entorno = Aislado::nuevo();
    inventario_con_nombres_repetidos(&entorno);
    let salida = entorno.ejecutar(&["sesion", "agente"]);
    assert_eq!(salida.status.code(), Some(1), "{salida:?}");
    let error = texto(&salida.stderr);
    assert!(error.contains("«agente»"), "{error}");
    assert!(error.contains("varios hosts"), "{error}");
    assert!(error.contains("db-01, web-01"), "{error}");
    assert!(error.contains("--host"), "{error}");
}

#[test]
fn magi_sesion_con_host_que_no_existe_o_que_no_tiene_la_sesion_no_abre_nada() {
    let entorno = Aislado::nuevo();
    inventario_con_nombres_repetidos(&entorno);

    let salida = entorno.ejecutar(&["sesion", "agente", "--host", "nube"]);
    assert_eq!(salida.status.code(), Some(1));
    assert!(
        texto(&salida.stderr).contains("no existe el host «nube»"),
        "{}",
        texto(&salida.stderr)
    );

    // `build` solo está en db-01.
    let salida = entorno.ejecutar(&["sesion", "build", "--host", "web-01"]);
    assert_eq!(salida.status.code(), Some(1));
    let error = texto(&salida.stderr);
    assert!(
        error.contains("«web-01» no tiene ninguna sesión persistente «build»"),
        "{error}"
    );
    assert!(error.contains("build (en db-01)"), "{error}");
}

#[test]
fn magi_sesion_rechaza_un_nombre_que_no_vale_antes_de_mirar_nada() {
    let entorno = Aislado::nuevo();
    inventario_con_nombres_repetidos(&entorno);
    for malo in ["con espacio", "ñandú", "a;b"] {
        let salida = entorno.ejecutar(&["sesion", malo, "--host", "no-existe"]);
        assert_eq!(salida.status.code(), Some(1), "{malo}");
        let error = texto(&salida.stderr);
        assert!(
            error.contains("no es un nombre de sesión válido"),
            "{error}"
        );
        assert!(!error.contains("no existe el host"), "{error}");
    }
}

#[test]
fn con_host_la_resolucion_elige_la_ficha_del_almacen_real() {
    // La parte de `magi sesion` que abre la TUI no se puede ejecutar sin una
    // terminal: la resolución, que es lo que decide a qué sesión engancharse,
    // se prueba con lo que lee el almacén de verdad.
    let entorno = Aislado::nuevo();
    inventario_con_nombres_repetidos(&entorno);
    let almacen = magi::almacen::abrir(&entorno.rutas.base_datos()).unwrap();
    let hosts = almacen.listar_hosts().unwrap();
    let fichas = magi_nucleo::almacen::sesiones_persistentes::listar(almacen.conexion()).unwrap();
    almacen.cerrar().unwrap();

    let resuelta =
        magi::sesiones_cli::resolver_sesion("agente", Some("db-01"), &hosts, &fichas).unwrap();
    assert_eq!(resuelta.host_nombre, "db-01");
    assert_eq!(resuelta.nombre, "agente");
    let db = hosts.iter().find(|host| host.nombre == "db-01").unwrap();
    assert_eq!(resuelta.host_id, db.id);

    // Sin ambigüedad, no hace falta --host.
    let resuelta = magi::sesiones_cli::resolver_sesion("build", None, &hosts, &fichas).unwrap();
    assert_eq!(resuelta.host_nombre, "db-01");
    // Con ambigüedad, sí.
    assert!(magi::sesiones_cli::resolver_sesion("agente", None, &hosts, &fichas).is_err());
}

#[test]
fn magi_sesion_engancha_al_arrancar_sin_crear_y_soltando_a_los_demas() {
    // Lo que hace `magi sesion agente --host db-01` una vez resuelta la ficha:
    // MAGI arranca y manda `AbrirSesion` con el tmux propio, sin `crear` (la
    // sesión ya existe) y sin `compartir` (suelta a los demás).
    use magi::app::{AperturaInicial, App};
    use magi::protocolo::{MensajeCliente, ServidorTmux};

    let entorno = Aislado::nuevo();
    inventario_con_nombres_repetidos(&entorno);
    let almacen = magi::almacen::abrir(&entorno.rutas.base_datos()).unwrap();
    let hosts = almacen.listar_hosts().unwrap();
    let fichas = magi_nucleo::almacen::sesiones_persistentes::listar(almacen.conexion()).unwrap();
    let resuelta =
        magi::sesiones_cli::resolver_sesion("agente", Some("db-01"), &hosts, &fichas).unwrap();

    let (mut app, mut enviados) = App::de_prueba(
        entorno.rutas.clone(),
        Config::default(),
        magi::tema::Tema::respaldo(),
        almacen,
    )
    .expect("app de prueba");
    app.abrir_al_arrancar = Some(AperturaInicial::Sesion {
        host_id: resuelta.host_id,
        nombre: resuelta.nombre.clone(),
    });
    app.abrir_lo_del_arranque();
    assert!(
        app.abrir_al_arrancar.is_none(),
        "lo del arranque se abre una sola vez"
    );

    let mut aperturas = Vec::new();
    while let Ok(mensaje) = enviados.try_recv() {
        if let MensajeCliente::AbrirSesion { host_id, tmux, .. } = mensaje {
            aperturas.push((host_id, tmux));
        }
    }
    assert_eq!(aperturas.len(), 1, "{aperturas:?}");
    let (host_id, tmux) = &aperturas[0];
    assert_eq!(*host_id, resuelta.host_id);
    let tmux = tmux.as_ref().expect("una sesión de tmux");
    assert_eq!(tmux.servidor, ServidorTmux::Magi);
    assert_eq!(tmux.nombre, "agente");
    assert!(tmux.crear.is_none(), "la sesión ya existe: no se crea");
    assert!(!tmux.compartir, "engancharse suelta a los demás");
}

#[test]
fn magi_sesion_anota_el_enganche_cuando_aparece_la_pestana_y_nunca_antes() {
    // Informe §3.3: `sesion_persistente_enganchada` se anota también cuando el
    // usuario se engancha por la CLI. Se anota cuando el efecto se ha
    // producido (la pestaña existe), no al decidirlo, y sin comandos ni nada
    // de lo que pasa dentro de la sesión.
    use magi::app::{AperturaInicial, App, Evento};
    use magi::protocolo::{
        EstadoSesionRemota, InfoSesion, MensajeServidor, ServidorTmux, TmuxPestana,
    };
    use magi::registro::FiltroRegistro;
    use magi_nucleo::sesiones_persistentes::VersionTmux;

    let entorno = Aislado::nuevo();
    inventario_con_nombres_repetidos(&entorno);
    let almacen = magi::almacen::abrir(&entorno.rutas.base_datos()).unwrap();
    let hosts = almacen.listar_hosts().unwrap();
    let fichas = magi_nucleo::almacen::sesiones_persistentes::listar(almacen.conexion()).unwrap();
    let resuelta =
        magi::sesiones_cli::resolver_sesion("agente", Some("db-01"), &hosts, &fichas).unwrap();

    let (mut app, _enviados) = App::de_prueba(
        entorno.rutas.clone(),
        Config::default(),
        magi::tema::Tema::respaldo(),
        almacen,
    )
    .expect("app de prueba");
    app.abrir_al_arrancar = Some(AperturaInicial::Sesion {
        host_id: resuelta.host_id,
        nombre: resuelta.nombre.clone(),
    });
    app.abrir_lo_del_arranque();

    let enganchadas = |app: &App| {
        app.almacen
            .listar_registro(&FiltroRegistro::default(), 100, 0)
            .expect("registro")
            .into_iter()
            .filter(|entrada| entrada.tipo == "sesion_persistente_enganchada")
            .collect::<Vec<_>>()
    };
    assert!(
        enganchadas(&app).is_empty(),
        "el efecto aún no se ha producido: no hay pestaña"
    );

    // El servidor abre la pestaña y la difunde.
    app.procesar(Evento::Servidor(MensajeServidor::Sesiones {
        lista: vec![InfoSesion {
            id: 1,
            nombre: "agente".to_string(),
            host_id: resuelta.host_id,
            host_nombre: "db-01".to_string(),
            estado: EstadoSesionRemota::Abierta,
            motivo: None,
            identidad: "clave".to_string(),
            abierta_en: ahora(),
            ventanas: 1,
            actividad_no_vista: false,
            reconexion: None,
            titulo: None,
            aviso: None,
            tmux: Some(TmuxPestana {
                creada: None,
                servidor: ServidorTmux::Magi,
                nombre: "agente".to_string(),
                version: Some(VersionTmux::nueva(3, 5)),
                soltada: None,
            }),
            escritura_varios: false,
        }],
    }));
    let anotadas = enganchadas(&app);
    assert_eq!(anotadas.len(), 1, "{anotadas:?}");
    assert!(
        anotadas[0].detalle.contains("db-01") && anotadas[0].detalle.contains("agente"),
        "{}",
        anotadas[0].detalle
    );
    assert!(
        !anotadas[0].detalle.contains("secreto-de-la-prueba"),
        "nada de comandos en el registro"
    );
}

// ---------------------------------------------------------- magi sesiones

#[test]
fn magi_sesiones_sin_fichas_dice_como_crearlas_y_no_arranca_el_servidor() {
    let entorno = Aislado::nuevo();
    // Hay un host, pero ninguna ficha: no hay nada que preguntar.
    entorno.host("web-01", "192.0.2.10", 22, IdentidadRef::Auto);

    let salida = entorno.ejecutar(&["sesiones"]);
    assert!(salida.status.success(), "{salida:?}");
    let texto_salida = texto(&salida.stdout);
    assert!(
        texto_salida.contains("No hay sesiones persistentes"),
        "{texto_salida}"
    );

    let salida = entorno.ejecutar(&["sesiones", "--json"]);
    assert!(salida.status.success(), "{salida:?}");
    assert_eq!(texto(&salida.stdout), "[]\n");
    assert!(
        !magi::servidor::ruta_socket(&entorno.rutas).exists(),
        "sin fichas no se arranca ningún servidor"
    );
}

#[test]
fn magi_sesiones_con_un_host_que_no_responde_sale_inalcanzable_nunca_terminada() {
    let entorno = Aislado::nuevo();
    let _para = ParaAlSalir(&entorno);
    // Un puerto cerrado: nadie escucha.
    let cerrado = {
        let escucha = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        escucha.local_addr().unwrap().port()
    };
    let host = entorno.host("caido-01", "127.0.0.1", cerrado, IdentidadRef::Auto);
    let uuid = entorno.ficha(host, "agente");

    // La orden arranca el servidor y le pregunta; el host no responde.
    let salida = entorno.ejecutar(&["sesiones", "--json"]);
    assert!(salida.status.success(), "{salida:?}");
    let filas: serde_json::Value = serde_json::from_slice(&salida.stdout).expect("JSON válido");
    assert_eq!(filas.as_array().unwrap().len(), 1, "{filas}");
    assert_eq!(filas[0]["uuid"], uuid);
    assert_eq!(filas[0]["host"], "caido-01");
    assert_eq!(filas[0]["nombre"], "agente");
    assert_eq!(filas[0]["estado"], "inalcanzable", "{filas}");
    assert_ne!(filas[0]["estado"], "terminada");
    assert!(filas[0]["comprobada_en"].as_i64().unwrap() > 0, "{filas}");
    assert!(filas[0]["equipo"].is_null());
    assert_eq!(filas[0]["atencion"], false);

    // La tabla lo dice con palabras y la instrucción va a la salida de errores.
    let salida = entorno.ejecutar(&["sesiones"]);
    assert!(salida.status.success(), "{salida:?}");
    let tabla = texto(&salida.stdout);
    assert!(tabla.contains("inalcanzable"), "{tabla}");
    assert!(!tabla.contains("terminada"), "{tabla}");
    assert!(
        texto(&salida.stderr).contains("«caido-01» no responde"),
        "{}",
        texto(&salida.stderr)
    );
}

/// Un servidor SSH de pruebas con un tmux falso, y un servidor de sesiones en
/// proceso sobre el entorno aislado: lo que la orden `magi sesiones` ve.
struct Montaje {
    entorno: Aislado,
    base: tempfile::TempDir,
    puerto_ssh: u16,
    ruta_clave: PathBuf,
    _ssh: tokio::task::JoinHandle<()>,
    servidor: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl Drop for Montaje {
    fn drop(&mut self) {
        self._ssh.abort();
        self.servidor.abort();
    }
}

impl Montaje {
    async fn nuevo() -> Self {
        let entorno = Aislado::nuevo();
        let base = tempfile::tempdir().expect("directorio de la prueba");
        let bin = base.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let guion = bin.join("tmux");
        std::fs::write(&guion, GUION_TMUX).unwrap();
        std::fs::set_permissions(&guion, std::fs::Permissions::from_mode(0o755)).unwrap();

        // Servidor SSH de pruebas, con clave conocida de antemano.
        let clave = ssh_key::PrivateKey::random(
            &mut ssh_key::rand_core::UnwrapErr(ssh_key::getrandom::SysRng),
            ssh_key::Algorithm::Ed25519,
        )
        .unwrap();
        let config_ssh = Arc::new(russh::server::Config {
            auth_rejection_time: Duration::from_millis(1),
            keys: vec![clave.clone()],
            ..Default::default()
        });
        let escucha = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let puerto_ssh = escucha.local_addr().unwrap().port();
        let mut servidor_ssh = ServidorSesion {
            sftp: false,
            reenvio: ModoReenvio::Concede,
            estado: Arc::new(Reenvios::default()),
            observado: Arc::new(Observado::default()),
            binarios: Some(bin),
            programa_shell: None,
            entorno_extra: vec![
                (
                    "MAGI_PRUEBA_BASE".to_string(),
                    base.path().display().to_string(),
                ),
                ("MAGI_TMUX_FALSO".to_string(), TMUX_FALSO.to_string()),
            ],
        };
        let ssh = tokio::spawn(async move {
            let _ =
                russh::server::Server::run_on_socket(&mut servidor_ssh, config_ssh, &escucha).await;
        });

        // Identidad del cliente y huella del host.
        let rutas = entorno.rutas.clone();
        std::fs::create_dir_all(rutas.dir_ssh()).unwrap();
        let clave_cliente = ssh_key::PrivateKey::random(
            &mut ssh_key::rand_core::UnwrapErr(ssh_key::getrandom::SysRng),
            ssh_key::Algorithm::Ed25519,
        )
        .unwrap();
        let ruta_clave = rutas.dir_ssh().join("prueba_cliente");
        std::fs::write(
            &ruta_clave,
            clave_cliente.to_openssh(ssh_key::LineEnding::LF).unwrap(),
        )
        .unwrap();
        std::fs::set_permissions(&ruta_clave, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::write(
            rutas.fichero_known_hosts(),
            format!(
                "[127.0.0.1]:{puerto_ssh} {}\n",
                clave.public_key().to_openssh().unwrap()
            ),
        )
        .unwrap();

        Self {
            entorno,
            base,
            puerto_ssh,
            ruta_clave,
            _ssh: ssh,
            servidor: tokio::spawn(async { Ok(()) }),
        }
    }

    /// Un host contra el servidor SSH de pruebas; su usuario (`usuario<n>`)
    /// elige su estado de tmux.
    fn host(&self, nombre: &str, numero: usize) -> i64 {
        let almacen = magi::almacen::abrir(&self.entorno.rutas.base_datos()).unwrap();
        let id = magi::almacen::hosts::crear(
            almacen.conexion(),
            &DatosHost {
                nombre: nombre.to_string(),
                direccion: "127.0.0.1".to_string(),
                puerto: self.puerto_ssh,
                usuario: Some(format!("usuario{numero}")),
                identidad_ref: IdentidadRef::Fichero(self.ruta_clave.display().to_string()),
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .unwrap();
        almacen.cerrar().unwrap();
        id
    }

    /// Las sesiones del tmux propio del host número `n`.
    fn tmux(&self, numero: usize, lineas: &[String]) {
        std::fs::write(
            self.base
                .path()
                .join(format!("estado.usuario{numero}.magi")),
            lineas.concat(),
        )
        .unwrap();
    }

    /// Arranca el servidor de sesiones en proceso sobre el entorno.
    async fn arrancar_servidor(&mut self) {
        let mut config = Config::default();
        // Que no se apague entre una orden y la siguiente.
        config.servidor.gracia_apagado_seg = 600;
        self.servidor = tokio::spawn(magi::servidor::arrancar(self.entorno.rutas.clone(), config));
        comun::esperar_socket(&magi::servidor::ruta_socket(&self.entorno.rutas)).await;
    }
}

/// Una sesión en el formato del tmux falso: nombre, clientes, ventanas,
/// creada, equipo, actividad, campana, título y creación.
fn linea(nombre: &str, clientes: u32, equipo: &str, campana: bool, titulo: &str) -> String {
    format!(
        "{nombre}\t{clientes}\t1\tx\t{equipo}\t{}\t{}\t{titulo}\t{}\n",
        ahora() - 90,
        u8::from(campana),
        ahora()
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn magi_sesiones_json_cuenta_lo_que_ve_el_servidor_con_los_campos_de_siempre() {
    let mut montaje = Montaje::nuevo().await;
    let uno = montaje.host("web-01", 1);
    let cerrado = {
        let escucha = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        escucha.local_addr().unwrap().port()
    };
    let caido = montaje
        .entorno
        .host("caido-01", "127.0.0.1", cerrado, IdentidadRef::Auto);
    let enganchada = montaje.entorno.ficha(uno, "agente-api");
    let libre = montaje.entorno.ficha(uno, "build");
    let terminada = montaje.entorno.ficha(uno, "viejo");
    let sin_mirar = montaje.entorno.ficha(caido, "agente");
    montaje.tmux(
        1,
        &[
            linea("agente-api", 1, "portatil", true, "claude: pide permiso"),
            linea("build", 0, "sobremesa", false, ""),
            // Una sesión sin ficha: no sale en `magi sesiones`.
            linea("de-otro-origen", 0, "portatil", false, "no sale"),
        ],
    );
    montaje.arrancar_servidor().await;

    let antes = ahora();
    let salida = montaje
        .entorno
        .ejecutar_async(&["sesiones", "--json"])
        .await;
    let despues = ahora();
    assert!(salida.status.success(), "{salida:?}");
    let bruto = texto(&salida.stdout);
    let filas: serde_json::Value = serde_json::from_str(&bruto).expect("JSON válido");
    let filas = filas.as_array().expect("una lista");
    assert_eq!(filas.len(), 4, "{bruto}");

    // Los campos y su orden son los de siempre.
    let claves = [
        "\"uuid\"",
        "\"host\"",
        "\"nombre\"",
        "\"estado\"",
        "\"equipo\"",
        "\"actividad\"",
        "\"atencion\"",
        "\"titulo\"",
        "\"comprobada_en\"",
    ];
    let primera = bruto.find('{').unwrap();
    let cierre = bruto[primera..].find('}').unwrap() + primera;
    let objeto = &bruto[primera..cierre];
    let posiciones: Vec<usize> = claves
        .iter()
        .map(|clave| {
            objeto
                .find(clave)
                .unwrap_or_else(|| panic!("falta {clave} en {objeto}"))
        })
        .collect();
    assert!(posiciones.windows(2).all(|par| par[0] < par[1]), "{objeto}");
    for fila in filas {
        assert_eq!(fila.as_object().unwrap().len(), claves.len(), "{fila}");
    }

    let por_nombre = |nombre: &str, host: &str| {
        filas
            .iter()
            .find(|fila| fila["nombre"] == nombre && fila["host"] == host)
            .unwrap_or_else(|| panic!("no sale {nombre} en {host}: {bruto}"))
    };

    // Enganchada, con quién la tiene, la atención y el título.
    let fila = por_nombre("agente-api", "web-01");
    assert_eq!(fila["uuid"], enganchada);
    assert_eq!(fila["estado"], "viva_enganchada");
    assert_eq!(fila["equipo"], "portatil");
    assert_eq!(fila["atencion"], true);
    assert_eq!(fila["titulo"], "claude: pide permiso");
    let comprobada = fila["comprobada_en"].as_i64().unwrap();
    assert!(
        (antes - 1..=despues + 1).contains(&comprobada),
        "comprobada_en {comprobada} fuera de [{antes}, {despues}]"
    );
    // La actividad, en la hora de este equipo, de hace unos 90 s.
    let actividad = fila["actividad"].as_i64().unwrap();
    assert!(
        (antes - 130..=despues - 80).contains(&actividad),
        "actividad {actividad} lejos de hace 90 s ({antes}..{despues})"
    );

    // Libre: nadie la tiene, aunque otro equipo la tuvo.
    let fila = por_nombre("build", "web-01");
    assert_eq!(fila["uuid"], libre);
    assert_eq!(fila["estado"], "viva_libre");
    assert!(fila["equipo"].is_null(), "{fila}");
    assert_eq!(fila["atencion"], false);

    // El host responde y la sesión no está: terminada.
    let fila = por_nombre("viejo", "web-01");
    assert_eq!(fila["uuid"], terminada);
    assert_eq!(fila["estado"], "terminada");
    assert!(fila["actividad"].is_null());
    assert!(fila["titulo"].is_null());

    // El host que no responde: inalcanzable, nunca terminada.
    let fila = por_nombre("agente", "caido-01");
    assert_eq!(fila["uuid"], sin_mirar);
    assert_eq!(fila["estado"], "inalcanzable");

    // Y nada de lo privado de las fichas ni de las sesiones sin ficha.
    for secreto in ["secreto-de-la-prueba", "nota privada", "no sale"] {
        assert!(!bruto.contains(secreto), "{secreto} en {bruto}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn magi_sesiones_en_texto_enseña_las_columnas_y_los_avisos_van_aparte() {
    let mut montaje = Montaje::nuevo().await;
    let uno = montaje.host("web-01", 1);
    montaje.entorno.ficha(uno, "agente-api");
    montaje.entorno.ficha(uno, "viejo");
    montaje.tmux(
        1,
        &[linea(
            "agente-api",
            1,
            "portatil",
            true,
            "claude: pide permiso",
        )],
    );
    montaje.arrancar_servidor().await;

    let salida = montaje.entorno.ejecutar_async(&["sesiones"]).await;
    assert!(salida.status.success(), "{salida:?}");
    let tabla = texto(&salida.stdout);
    let lineas: Vec<&str> = tabla.lines().collect();
    assert_eq!(lineas[0], "Sesiones persistentes: 2", "{tabla}");
    for columna in ["ESTADO", "NOMBRE", "HOST", "EQUIPO", "ACTIVIDAD", "TÍTULO"] {
        assert!(lineas[1].contains(columna), "{tabla}");
    }
    let agente = lineas
        .iter()
        .find(|linea| linea.contains("agente-api"))
        .expect("la fila de agente-api");
    for trozo in [
        "enganchada",
        "web-01",
        "portatil",
        "hace 1 min",
        "!",
        "claude: pide permiso",
    ] {
        assert!(agente.contains(trozo), "{trozo} en {agente}");
    }
    assert!(
        lineas
            .iter()
            .any(|linea| linea.starts_with("terminada") && linea.contains("viejo")),
        "{tabla}"
    );
    // La tabla está limpia: ningún aviso mezclado.
    assert!(!tabla.contains("Avisos"), "{tabla}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn una_lista_vieja_del_servidor_no_se_toma_por_la_respuesta() {
    // El servidor recuerda la lista de la última comprobación y se la da a
    // la ventana que llega, que puede ser de hace mucho: la orden tiene que
    // esperar a una comprobación POSTERIOR a su pregunta.
    let mut montaje = Montaje::nuevo().await;
    let uno = montaje.host("web-01", 1);
    montaje.entorno.ficha(uno, "agente-api");
    montaje.tmux(1, &[linea("agente-api", 0, "portatil", false, "")]);
    montaje.arrancar_servidor().await;

    let salida = montaje
        .entorno
        .ejecutar_async(&["sesiones", "--json"])
        .await;
    assert!(salida.status.success(), "{salida:?}");
    let primera: serde_json::Value = serde_json::from_slice(&salida.stdout).unwrap();
    assert_eq!(primera[0]["estado"], "viva_libre", "{primera}");

    // La sesión acaba en el host. El servidor aún guarda la lista de antes.
    montaje.tmux(1, &[]);
    // Pasa la regla de los 5 s del servidor: ya no se da por reciente.
    tokio::time::sleep(Duration::from_secs(6)).await;

    let salida = montaje
        .entorno
        .ejecutar_async(&["sesiones", "--json"])
        .await;
    assert!(salida.status.success(), "{salida:?}");
    let segunda: serde_json::Value = serde_json::from_slice(&salida.stdout).unwrap();
    assert_eq!(
        segunda[0]["estado"], "terminada",
        "se tomó la lista vieja por la respuesta: {segunda}"
    );
    assert!(
        segunda[0]["comprobada_en"].as_i64().unwrap()
            > primera[0]["comprobada_en"].as_i64().unwrap(),
        "{segunda}"
    );
}

#[test]
fn sin_servidor_y_sin_poder_arrancarlo_las_fichas_salen_sin_comprobar_y_el_codigo_es_uno() {
    // Un runtime imposible: la ruta del socket pasa de ~108 bytes y el
    // servidor no puede arrancar, así que la orden no tiene a quién preguntar.
    let entorno = Aislado::nuevo();
    let web = entorno.host("web-01", "192.0.2.10", 22, IdentidadRef::Auto);
    entorno.ficha(web, "agente");
    let largo = entorno.raiz.join("r".repeat(120));
    std::fs::create_dir_all(&largo).unwrap();
    let mut comando = entorno.comando(&["sesiones", "--json"]);
    comando.env("XDG_RUNTIME_DIR", &largo);
    let salida = comando.output().unwrap();
    assert_eq!(salida.status.code(), Some(1), "{salida:?}");
    let filas: serde_json::Value = serde_json::from_slice(&salida.stdout).expect("JSON válido");
    assert_eq!(filas[0]["estado"], "sin_comprobar", "{filas}");
    assert!(filas[0]["comprobada_en"].is_null());
    let error = texto(&salida.stderr);
    assert!(error.contains("sin_comprobar"), "{error}");
}
