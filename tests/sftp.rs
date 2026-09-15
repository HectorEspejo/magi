//! Pruebas de extremo a extremo de la vista Archivos (F4): el servidor de
//! sesiones habla por SFTP con el `sftp-server` real de OpenSSH que sirve el
//! harness de `comun`, sobre ficheros de verdad en directorios temporales.
//!
//! Si el sistema no trae `sftp-server`, las pruebas se saltan con un aviso:
//! probar el cliente contra un servidor propio escondería los fallos que solo
//! aparecen con el de verdad.

mod comun;

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::net::UnixStream;
use tokio_util::codec::FramedRead;

use magi::almacen::{hosts, Almacen};
use magi::archivos::TipoEntrada;
use magi::modelo::{DatosHost, IdentidadRef, Origen};
use magi::protocolo::{
    Direccion, ElementoTransferencia, EstadoTransferencia, MensajeCliente, MensajeServidor,
    Politica, VERSION_PROTOCOLO,
};
use magi::servidor;

use comun::*;

type Lector = FramedRead<tokio::net::unix::OwnedReadHalf, magi::protocolo::LinesCodec>;

/// Escenario completo: servidor SSH de pruebas, host en la base de datos,
/// servidor de sesiones y cliente del protocolo.
struct Montaje {
    entorno: Entorno,
    /// Lo que ve el `sftp-server`: el «remoto» de las pruebas.
    remoto: tempfile::TempDir,
    /// El «local» de las pruebas.
    local: tempfile::TempDir,
    host_id: i64,
    escritura: tokio::net::unix::OwnedWriteHalf,
    lector: Lector,
    /// Última difusión de la cola vista: una transferencia puede terminar
    /// antes de que la prueba mire.
    cola: Vec<magi::protocolo::InfoTransferencia>,
    _tarea_ssh: tokio::task::JoinHandle<()>,
    tarea_magi: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl Montaje {
    fn remoto(&self) -> PathBuf {
        self.remoto.path().to_path_buf()
    }

    fn local(&self) -> PathBuf {
        self.local.path().to_path_buf()
    }

    async fn enviar(&mut self, mensaje: &MensajeCliente) {
        enviar_a(&mut self.escritura, mensaje).await;
    }

    /// Espera un mensaje que cumpla el predicado, con plazo.
    async fn esperar<F>(&mut self, mut vale: F) -> MensajeServidor
    where
        F: FnMut(&MensajeServidor) -> bool,
    {
        // Holgado a propósito: un host sin subsistema SFTP tarda en rendirse
        // el plazo del saludo (10 s) antes de contestar con el error.
        let plazo = tokio::time::Instant::now() + Duration::from_secs(40);
        loop {
            assert!(
                tokio::time::Instant::now() < plazo,
                "no llegó el mensaje esperado"
            );
            let mensaje: MensajeServidor = match tokio::time::timeout(
                Duration::from_secs(25),
                siguiente_framed(&mut self.lector),
            )
            .await
            {
                Ok(mensaje) => mensaje,
                Err(_) => panic!("el servidor no contesta en 25 s"),
            };
            if let MensajeServidor::Transferencias { lista } = &mensaje {
                self.cola = lista.clone();
            }
            if vale(&mensaje) {
                return mensaje;
            }
        }
    }

    /// Abre el canal SFTP del host y devuelve el directorio de inicio remoto.
    async fn abrir_sftp(&mut self) -> String {
        let host_id = self.host_id;
        self.enviar(&MensajeCliente::AbrirSftp { host_id }).await;
        match self
            .esperar(|mensaje| {
                matches!(
                    mensaje,
                    MensajeServidor::SftpAbierto { .. } | MensajeServidor::Error { .. }
                )
            })
            .await
        {
            MensajeServidor::SftpAbierto {
                host_id: id,
                dir_inicio,
            } => {
                assert_eq!(id, host_id);
                dir_inicio
            }
            MensajeServidor::Error { mensaje, .. } => panic!("no se abrió el SFTP: {mensaje}"),
            otro => panic!("respuesta inesperada: {otro:?}"),
        }
    }

    /// Lista un directorio remoto.
    async fn listar(&mut self, ruta: &str, peticion_id: u64) -> Vec<magi::archivos::Entrada> {
        let host_id = self.host_id;
        self.enviar(&MensajeCliente::ListarDir {
            host_id,
            ruta: ruta.to_string(),
            peticion_id,
        })
        .await;
        match self
            .esperar(|mensaje| {
                matches!(
                    mensaje,
                    MensajeServidor::DirListado { .. } | MensajeServidor::Error { .. }
                )
            })
            .await
        {
            MensajeServidor::DirListado {
                entradas,
                peticion_id: id,
                ..
            } => {
                assert_eq!(id, peticion_id);
                entradas
            }
            MensajeServidor::Error { mensaje, .. } => panic!("no se pudo listar: {mensaje}"),
            otro => panic!("respuesta inesperada: {otro:?}"),
        }
    }

    /// Espera el final de una transferencia y devuelve su fila en la cola.
    /// Mira primero lo último difundido: puede haber terminado ya.
    async fn esperar_terminada(&mut self, id: u32) -> magi::protocolo::InfoTransferencia {
        let plazo = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(fila) = self
                .cola
                .iter()
                .find(|fila| fila.id == id && fila.estado.terminada())
            {
                return fila.clone();
            }
            assert!(
                tokio::time::Instant::now() < plazo,
                "la transferencia {id} no termina"
            );
            let _ = self
                .esperar(|mensaje| matches!(mensaje, MensajeServidor::Transferencias { .. }))
                .await;
        }
    }

    /// Encola una transferencia y espera su id en una difusión de la cola.
    async fn transferir(
        &mut self,
        direccion: Direccion,
        elementos: Vec<ElementoTransferencia>,
        politica: Politica,
        borrar_origen: bool,
    ) -> u32 {
        let host_id = self.host_id;
        let conocidas: Vec<u32> = self.cola.iter().map(|fila| fila.id).collect();
        self.enviar(&MensajeCliente::Transferir {
            host_id,
            direccion,
            elementos,
            politica,
            borrar_origen,
        })
        .await;
        let plazo = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(fila) = self.cola.iter().find(|fila| !conocidas.contains(&fila.id)) {
                return fila.id;
            }
            assert!(
                tokio::time::Instant::now() < plazo,
                "la transferencia no llega a la cola"
            );
            if let MensajeServidor::Error { mensaje, .. } = self
                .esperar(|mensaje| {
                    matches!(
                        mensaje,
                        MensajeServidor::Transferencias { .. } | MensajeServidor::Error { .. }
                    )
                })
                .await
            {
                panic!("no se pudo encolar: {mensaje}");
            }
        }
    }
}

impl Drop for Montaje {
    fn drop(&mut self) {
        self.tarea_magi.abort();
    }
}

fn leer(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Monta el escenario. `sftp` en falso simula un host sin subsistema SFTP.
async fn montar(sftp: bool) -> Option<Montaje> {
    montar_con(sftp, false).await
}

/// Como `montar`, pero con la identidad del host: por clave (lo normal) o por
/// contraseña, para probar el camino de los diálogos.
async fn montar_con(sftp: bool, por_contrasena: bool) -> Option<Montaje> {
    if !hay_sftp_server() {
        return None;
    }
    let entorno = entorno();
    let rutas = entorno.rutas.clone();

    // 1. Servidor SSH de pruebas.
    let clave = ssh_key::PrivateKey::random(
        &mut ssh_key::rand_core::UnwrapErr(ssh_key::getrandom::SysRng),
        ssh_key::Algorithm::Ed25519,
    )
    .unwrap();
    let config = Arc::new(russh::server::Config {
        auth_rejection_time: Duration::from_millis(1),
        keys: vec![clave.clone()],
        ..Default::default()
    });
    let escucha = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let puerto_ssh = escucha.local_addr().unwrap().port();
    let tarea_ssh = tokio::spawn(async move {
        let mut servidor = ServidorSesion { sftp };
        let _ = russh::server::Server::run_on_socket(&mut servidor, config, &escucha).await;
    });

    // 2. Clave de cliente y huella conocida de antemano.
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

    // 3. Host en la base de datos.
    let host_id = {
        let almacen = Almacen::abrir(&rutas.base_datos()).unwrap();
        let id = hosts::crear(
            almacen.conexion(),
            &DatosHost {
                nombre: "prueba".to_string(),
                direccion: "127.0.0.1".to_string(),
                puerto: puerto_ssh,
                usuario: Some("hector".to_string()),
                identidad_ref: if por_contrasena {
                    IdentidadRef::Contrasena
                } else {
                    IdentidadRef::Fichero(ruta_clave.display().to_string())
                },
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .unwrap();
        almacen.cerrar().unwrap();
        id
    };

    // 4. Servidor de sesiones y cliente del protocolo.
    let tarea_magi = tokio::spawn(servidor::arrancar(rutas.clone(), entorno.config.clone()));
    esperar_socket(&servidor::ruta_socket(&rutas)).await;
    let mut stream = UnixStream::connect(servidor::ruta_socket(&rutas))
        .await
        .expect("conectando al servidor de sesiones");
    enviar(
        &mut stream,
        &MensajeCliente::Hola {
            version: VERSION_PROTOCOLO,
            pid: 42,
        },
    )
    .await;
    let (lectura, escritura) = stream.into_split();
    let mut lector = FramedRead::new(lectura, magi::protocolo::codec());
    let bienvenida: MensajeServidor = siguiente_framed(&mut lector).await;
    assert!(matches!(bienvenida, MensajeServidor::Bienvenida { .. }));

    Some(Montaje {
        entorno,
        remoto: tempfile::tempdir().unwrap(),
        local: tempfile::tempdir().unwrap(),
        host_id,
        escritura,
        lector,
        cola: Vec::new(),
        _tarea_ssh: tarea_ssh,
        tarea_magi,
    })
}

/// El canal SFTP no es una sesión: no aparece en la lista de pestañas.
#[tokio::test]
async fn el_canal_sftp_no_aparece_como_sesion() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    montaje.abrir_sftp().await;
    montaje.enviar(&MensajeCliente::Listar).await;
    match montaje
        .esperar(|mensaje| matches!(mensaje, MensajeServidor::Sesiones { .. }))
        .await
    {
        MensajeServidor::Sesiones { lista } => assert!(
            lista.is_empty(),
            "el canal SFTP no debe contar como sesión: {lista:?}"
        ),
        otro => panic!("respuesta inesperada: {otro:?}"),
    }
}

#[tokio::test]
async fn abrir_sftp_devuelve_el_directorio_de_inicio() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let inicio = montaje.abrir_sftp().await;
    assert!(
        inicio.starts_with('/'),
        "el directorio de inicio debe ser absoluto: {inicio}"
    );
}

#[tokio::test]
async fn listar_dir_devuelve_tipo_tamano_mtime_y_permisos() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let remoto = montaje.remoto();
    std::fs::create_dir(remoto.join("app")).unwrap();
    std::fs::write(remoto.join("main.py"), b"doce bytes!!").unwrap();
    montaje.abrir_sftp().await;

    let entradas = montaje.listar(&remoto.display().to_string(), 1).await;
    let nombres: Vec<&str> = entradas
        .iter()
        .map(|entrada| entrada.nombre.as_str())
        .collect();
    assert_eq!(nombres, vec!["app", "main.py"], "directorios primero");

    let app = &entradas[0];
    assert_eq!(app.tipo, TipoEntrada::Directorio);
    assert!(app.permisos.is_some(), "los permisos vienen del remoto");
    assert!(app.propietario.is_some(), "el propietario viene del remoto");

    let fichero = &entradas[1];
    assert_eq!(fichero.tipo, TipoEntrada::Fichero);
    assert_eq!(fichero.tamano, 12);
    assert!(
        fichero.mtime > 1_600_000_000,
        "mtime real: {}",
        fichero.mtime
    );
}

#[tokio::test]
async fn listar_una_ruta_inexistente_devuelve_error_con_su_peticion() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    montaje.abrir_sftp().await;
    let host_id = montaje.host_id;
    montaje
        .enviar(&MensajeCliente::ListarDir {
            host_id,
            ruta: "/no/existe/de/ninguna/manera".to_string(),
            peticion_id: 77,
        })
        .await;
    match montaje
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::DirListado { .. } | MensajeServidor::Error { .. }
            )
        })
        .await
    {
        MensajeServidor::Error {
            mensaje,
            peticion_id,
        } => {
            assert_eq!(peticion_id, Some(77), "el error viaja con su petición");
            assert!(
                mensaje.contains("no existe"),
                "mensaje legible, no un volcado: {mensaje}"
            );
        }
        otro => panic!("se esperaba Error, llegó {otro:?}"),
    }
}

#[tokio::test]
async fn un_host_sin_subsistema_sftp_lo_dice() {
    let Some(mut montaje) = montar(false).await else {
        return;
    };
    let host_id = montaje.host_id;
    montaje.enviar(&MensajeCliente::AbrirSftp { host_id }).await;
    match montaje
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::SftpAbierto { .. } | MensajeServidor::Error { .. }
            )
        })
        .await
    {
        MensajeServidor::Error { mensaje, .. } => assert!(
            mensaje.contains("no ofrece SFTP"),
            "el motivo debe ser explícito: {mensaje}"
        ),
        otro => panic!("se esperaba Error, llegó {otro:?}"),
    }
}

#[tokio::test]
async fn borrar_remoto_es_recursivo_y_anota_el_borrado() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let remoto = montaje.remoto();
    std::fs::create_dir_all(remoto.join("viejo/dentro")).unwrap();
    std::fs::write(remoto.join("viejo/dentro/a.txt"), b"x").unwrap();
    std::fs::write(remoto.join("viejo/b.txt"), b"x").unwrap();
    montaje.abrir_sftp().await;

    let host_id = montaje.host_id;
    let ruta = remoto.join("viejo");
    montaje
        .enviar(&MensajeCliente::BorrarRemoto {
            host_id,
            rutas: vec![ruta.display().to_string()],
            peticion_id: 5,
        })
        .await;
    match montaje
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::Hecho { .. } | MensajeServidor::Error { .. }
            )
        })
        .await
    {
        MensajeServidor::Hecho { peticion_id } => assert_eq!(peticion_id, 5),
        otro => panic!("se esperaba Hecho, llegó {otro:?}"),
    }
    assert!(!ruta.exists(), "el directorio entero se ha borrado");

    // El efecto queda en el registro con el tipo nuevo.
    let almacen = Almacen::abrir(&montaje.entorno.rutas.base_datos()).unwrap();
    let entradas = almacen
        .listar_registro(
            &magi::registro::FiltroRegistro {
                texto: "borrado_remoto".to_string(),
                tipos: vec![],
            },
            10,
            0,
        )
        .unwrap();
    assert!(
        entradas
            .iter()
            .any(|entrada| entrada.tipo == "borrado_remoto"),
        "el borrado remoto se anota en REGISTRO"
    );
}

#[tokio::test]
async fn renombrar_y_crear_directorio_remotos_responden_hecho() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let remoto = montaje.remoto();
    std::fs::write(remoto.join("antes.txt"), b"hola").unwrap();
    montaje.abrir_sftp().await;

    let host_id = montaje.host_id;
    montaje
        .enviar(&MensajeCliente::RenombrarRemoto {
            host_id,
            de: remoto.join("antes.txt").display().to_string(),
            a: remoto.join("despues.txt").display().to_string(),
            peticion_id: 11,
        })
        .await;
    match montaje
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::Hecho { .. } | MensajeServidor::Error { .. }
            )
        })
        .await
    {
        MensajeServidor::Hecho { peticion_id } => assert_eq!(peticion_id, 11),
        otro => panic!("se esperaba Hecho, llegó {otro:?}"),
    }
    assert!(remoto.join("despues.txt").exists());

    montaje
        .enviar(&MensajeCliente::CrearDirRemoto {
            host_id,
            ruta: remoto.join("nuevo").display().to_string(),
            peticion_id: 12,
        })
        .await;
    match montaje
        .esperar(|mensaje| matches!(mensaje, MensajeServidor::Hecho { peticion_id } if *peticion_id == 12))
        .await
    {
        MensajeServidor::Hecho { .. } => {}
        otro => panic!("se esperaba Hecho, llegó {otro:?}"),
    }
    assert!(remoto.join("nuevo").is_dir());
}

#[tokio::test]
async fn descargar_temporal_usa_permisos_estrechos_y_borrar_lo_quita() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let remoto = montaje.remoto();
    let origen = remoto.join("error.log");
    std::fs::write(&origen, b"linea de log\n").unwrap();
    montaje.abrir_sftp().await;

    let host_id = montaje.host_id;
    montaje
        .enviar(&MensajeCliente::DescargarTemporal {
            host_id,
            ruta: origen.display().to_string(),
            peticion_id: 21,
        })
        .await;
    let temporal = match montaje
        .esperar(|mensaje| {
            matches!(
                mensaje,
                MensajeServidor::RutaTemporal { .. } | MensajeServidor::Error { .. }
            )
        })
        .await
    {
        MensajeServidor::RutaTemporal { ruta, peticion_id } => {
            assert_eq!(peticion_id, 21);
            ruta
        }
        otro => panic!("se esperaba RutaTemporal, llegó {otro:?}"),
    };

    let camino = PathBuf::from(&temporal);
    assert_eq!(leer(&camino), "linea de log\n");
    let metadatos = std::fs::metadata(&camino).unwrap();
    assert_eq!(
        metadatos.permissions().mode() & 0o777,
        0o600,
        "el temporal va en 600"
    );
    let directorio = camino.parent().unwrap();
    assert_eq!(
        std::fs::metadata(directorio).unwrap().permissions().mode() & 0o777,
        0o700,
        "el directorio de temporales va en 700"
    );

    montaje
        .enviar(&MensajeCliente::BorrarTemporal {
            ruta: temporal.clone(),
        })
        .await;
    // Sin respuesta propia: se comprueba con un listado posterior de fondo.
    let host_id = montaje.host_id;
    montaje
        .enviar(&MensajeCliente::ListarDir {
            host_id,
            ruta: remoto.display().to_string(),
            peticion_id: 22,
        })
        .await;
    let _ = montaje
        .esperar(|mensaje| matches!(mensaje, MensajeServidor::DirListado { .. }))
        .await;
    assert!(!camino.exists(), "el temporal se ha borrado");
}

/// La transferencia de verdad: subir un fichero y bajarlo, con su mtime.
#[tokio::test]
async fn subir_y_bajar_un_fichero_conserva_el_mtime() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let remoto = montaje.remoto();
    let local = montaje.local();
    let origen = local.join("subida.txt");
    std::fs::write(&origen, b"contenido de la subida").unwrap();
    let mtime = filetime::FileTime::from_unix_time(1_700_000_000, 0);
    filetime::set_file_mtime(&origen, mtime).unwrap();
    montaje.abrir_sftp().await;

    let destino = remoto.join("subida.txt");
    let id = montaje
        .transferir(
            Direccion::Subida,
            vec![ElementoTransferencia {
                origen: origen.display().to_string(),
                destino: destino.display().to_string(),
                bytes: 22,
                es_directorio: false,
                politica: None,
            }],
            Politica::Sobrescribir,
            false,
        )
        .await;
    let fila = montaje.esperar_terminada(id).await;
    assert_eq!(fila.estado, EstadoTransferencia::Hecha, "{fila:?}");
    assert_eq!(leer(&destino), "contenido de la subida");
    let en_remoto = std::fs::metadata(&destino).unwrap();
    let mtime_remoto = filetime::FileTime::from_last_modification_time(&en_remoto);
    assert_eq!(
        mtime_remoto.unix_seconds(),
        1_700_000_000,
        "mtime conservado"
    );

    // Y la vuelta: bajar el mismo fichero a otro sitio.
    let bajado = local.join("bajada.txt");
    let id = montaje
        .transferir(
            Direccion::Bajada,
            vec![ElementoTransferencia {
                origen: destino.display().to_string(),
                destino: bajado.display().to_string(),
                bytes: 22,
                es_directorio: false,
                politica: None,
            }],
            Politica::Sobrescribir,
            false,
        )
        .await;
    let fila = montaje.esperar_terminada(id).await;
    assert_eq!(fila.estado, EstadoTransferencia::Hecha, "{fila:?}");
    assert_eq!(leer(&bajado), "contenido de la subida");
    let metadatos = std::fs::metadata(&bajado).unwrap();
    assert_eq!(
        filetime::FileTime::from_last_modification_time(&metadatos).unix_seconds(),
        1_700_000_000,
        "el mtime del origen se conserva en la bajada"
    );
}

#[tokio::test]
async fn bajar_un_directorio_lo_recorre_entero() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let remoto = montaje.remoto();
    let local = montaje.local();
    std::fs::create_dir_all(remoto.join("static/css")).unwrap();
    std::fs::write(remoto.join("static/index.html"), b"<html>").unwrap();
    std::fs::write(remoto.join("static/css/app.css"), b"body{}").unwrap();
    montaje.abrir_sftp().await;

    let destino = local.join("static");
    let id = montaje
        .transferir(
            Direccion::Bajada,
            vec![ElementoTransferencia {
                origen: remoto.join("static").display().to_string(),
                destino: destino.display().to_string(),
                bytes: 0,
                es_directorio: true,
                politica: None,
            }],
            Politica::Sobrescribir,
            false,
        )
        .await;
    let fila = montaje.esperar_terminada(id).await;
    assert_eq!(fila.estado, EstadoTransferencia::Hecha, "{fila:?}");
    assert_eq!(fila.ficheros_total, 2, "los dos ficheros del árbol");
    assert_eq!(leer(&destino.join("index.html")), "<html>");
    assert_eq!(leer(&destino.join("css/app.css")), "body{}");
}

/// Un enlace a directorio no se sigue al bajar: duplicaría el árbol entero.
#[tokio::test]
async fn bajar_un_directorio_omite_los_enlaces_a_directorio() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let remoto = montaje.remoto();
    let local = montaje.local();
    std::fs::create_dir_all(remoto.join("sitio/real")).unwrap();
    std::fs::write(remoto.join("sitio/real/dentro.txt"), b"x").unwrap();
    std::fs::write(remoto.join("sitio/suelto.txt"), b"y").unwrap();
    std::os::unix::fs::symlink(remoto.join("sitio/real"), remoto.join("sitio/atajo")).unwrap();
    montaje.abrir_sftp().await;

    let destino = local.join("sitio");
    let id = montaje
        .transferir(
            Direccion::Bajada,
            vec![ElementoTransferencia {
                origen: remoto.join("sitio").display().to_string(),
                destino: destino.display().to_string(),
                bytes: 0,
                es_directorio: true,
                politica: None,
            }],
            Politica::Sobrescribir,
            false,
        )
        .await;
    let fila = montaje.esperar_terminada(id).await;
    assert_eq!(fila.estado, EstadoTransferencia::Hecha, "{fila:?}");
    assert_eq!(fila.omitidos, 1, "el enlace a directorio se omite");
    assert!(destino.join("real/dentro.txt").exists());
    assert!(
        !destino.join("atajo").exists(),
        "no se sigue el enlace a directorio"
    );
}

/// El criterio de aceptación del checklist: un directorio con tres ficheros de
/// los que dos ya existen, con política `omitir`, copia uno y lo dice.
#[tokio::test]
async fn con_politica_omitir_copia_lo_que_falta_y_cuenta_los_omitidos() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let remoto = montaje.remoto();
    let local = montaje.local();
    std::fs::create_dir_all(local.join("sitio")).unwrap();
    std::fs::write(local.join("sitio/uno.txt"), b"uno nuevo").unwrap();
    std::fs::write(local.join("sitio/dos.txt"), b"dos nuevo").unwrap();
    std::fs::write(local.join("sitio/tres.txt"), b"tres nuevo").unwrap();
    // Dos ya están en el destino, con otro contenido.
    std::fs::write(remoto.join("dos.txt"), b"viejo").unwrap();
    std::fs::write(remoto.join("tres.txt"), b"viejo").unwrap();
    montaje.abrir_sftp().await;

    // La expansión de una subida la hace el cliente: la lista plana lleva los
    // directorios y los ficheros, todos con la política decidida.
    let (elementos, _) = magi::archivos::local::recorrer(&local.join("sitio")).unwrap();
    let lista: Vec<ElementoTransferencia> = elementos
        .iter()
        .map(|elemento| ElementoTransferencia {
            origen: elemento.origen.display().to_string(),
            destino: remoto.join(&elemento.relativo).display().to_string(),
            bytes: elemento.bytes,
            es_directorio: elemento.es_dir,
            politica: Some(Politica::Omitir),
        })
        .collect();
    let id = montaje
        .transferir(Direccion::Subida, lista, Politica::Omitir, false)
        .await;
    let fila = montaje.esperar_terminada(id).await;
    assert_eq!(fila.estado, EstadoTransferencia::Hecha, "{fila:?}");
    assert_eq!(fila.omitidos, 2, "los dos que ya existían");
    assert_eq!(
        leer(&remoto.join("uno.txt")),
        "uno nuevo",
        "se copia el que falta"
    );
    assert_eq!(leer(&remoto.join("dos.txt")), "viejo", "no se sobrescribe");
    assert_eq!(leer(&remoto.join("tres.txt")), "viejo", "no se sobrescribe");
}

#[tokio::test]
async fn una_transferencia_terminada_anota_en_el_registro() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let remoto = montaje.remoto();
    let local = montaje.local();
    std::fs::write(local.join("anotar.txt"), b"x").unwrap();
    montaje.abrir_sftp().await;

    let id = montaje
        .transferir(
            Direccion::Subida,
            vec![ElementoTransferencia {
                origen: local.join("anotar.txt").display().to_string(),
                destino: remoto.join("anotar.txt").display().to_string(),
                bytes: 1,
                es_directorio: false,
                politica: None,
            }],
            Politica::Sobrescribir,
            false,
        )
        .await;
    let fila = montaje.esperar_terminada(id).await;
    assert_eq!(fila.estado, EstadoTransferencia::Hecha, "{fila:?}");

    // El hilo escritor de la base de datos es asíncrono: se le da un respiro.
    let mut anotada = false;
    for _ in 0..50 {
        let almacen = Almacen::abrir(&montaje.entorno.rutas.base_datos()).unwrap();
        let entradas = almacen
            .listar_registro(
                &magi::registro::FiltroRegistro {
                    texto: "transferencia".to_string(),
                    tipos: vec![],
                },
                10,
                0,
            )
            .unwrap();
        if entradas
            .iter()
            .any(|entrada| entrada.tipo == "transferencia")
        {
            assert!(
                entradas
                    .iter()
                    .all(|entrada| !entrada.detalle.contains("contenido")),
                "el registro nunca guarda contenido"
            );
            anotada = true;
            break;
        }
        drop(almacen);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        anotada,
        "la transferencia terminada deja rastro en REGISTRO"
    );
}

#[tokio::test]
async fn subir_un_directorio_crea_los_directorios_que_falten() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let remoto = montaje.remoto();
    let local = montaje.local();
    std::fs::create_dir_all(local.join("paquete/src")).unwrap();
    std::fs::write(local.join("paquete/src/main.rs"), b"fn main() {}").unwrap();
    montaje.abrir_sftp().await;

    // El cliente expande los directorios de la subida.
    let (elementos, _) = magi::archivos::local::recorrer(&local.join("paquete")).unwrap();
    let destino_raiz = remoto.join("hondo/paquete");
    let mut lista: Vec<ElementoTransferencia> = elementos
        .iter()
        .map(|elemento| ElementoTransferencia {
            origen: elemento.origen.display().to_string(),
            destino: destino_raiz.join(&elemento.relativo).display().to_string(),
            bytes: elemento.bytes,
            es_directorio: elemento.es_dir,
            politica: None,
        })
        .collect();
    // Los directorios primero: se crean antes de sus ficheros.
    lista.sort_by_key(|elemento| std::cmp::Reverse(elemento.es_directorio));

    let id = montaje
        .transferir(Direccion::Subida, lista, Politica::Sobrescribir, false)
        .await;
    let fila = montaje.esperar_terminada(id).await;
    assert_eq!(fila.estado, EstadoTransferencia::Hecha, "{fila:?}");
    assert_eq!(
        leer(&destino_raiz.join("src/main.rs")),
        "fn main() {}",
        "los directorios que faltaban se crean"
    );
}

#[tokio::test]
async fn cancelar_una_transferencia_deja_su_estado_y_no_el_parcial() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let remoto = montaje.remoto();
    let local = montaje.local();
    // Un fichero de un tamaño que dé tiempo a cancelar entre bloques.
    let grande = vec![b'a'; 64 * 1024 * 1024];
    std::fs::write(local.join("grande.bin"), &grande).unwrap();
    montaje.abrir_sftp().await;

    let id = montaje
        .transferir(
            Direccion::Subida,
            vec![ElementoTransferencia {
                origen: local.join("grande.bin").display().to_string(),
                destino: remoto.join("grande.bin").display().to_string(),
                bytes: grande.len() as u64,
                es_directorio: false,
                politica: None,
            }],
            Politica::Sobrescribir,
            false,
        )
        .await;
    montaje
        .enviar(&MensajeCliente::CancelarTransferencia { id })
        .await;

    let fila = montaje.esperar_terminada(id).await;
    assert_eq!(
        fila.estado,
        EstadoTransferencia::Cancelada,
        "cancelar deja la transferencia cancelada: {fila:?}"
    );
    let parcial = remoto.join("grande.bin.magi-parcial");
    assert!(!parcial.exists(), "el parcial se borra al cancelar");
    assert!(
        !remoto.join("grande.bin").exists(),
        "no puede quedar un destino a medias"
    );
}

#[tokio::test]
async fn una_segunda_ventana_ve_la_cola_en_la_bienvenida() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let remoto = montaje.remoto();
    let local = montaje.local();
    std::fs::create_dir_all(local.join("cola")).unwrap();
    for indice in 0..3 {
        std::fs::write(local.join(format!("cola/f{indice}.txt")), b"datos").unwrap();
    }
    montaje.abrir_sftp().await;

    let (elementos, _) = magi::archivos::local::recorrer(&local.join("cola")).unwrap();
    let lista: Vec<ElementoTransferencia> = elementos
        .iter()
        .filter(|elemento| !elemento.es_dir)
        .map(|elemento| ElementoTransferencia {
            origen: elemento.origen.display().to_string(),
            destino: remoto.join(&elemento.relativo).display().to_string(),
            bytes: elemento.bytes,
            es_directorio: false,
            politica: None,
        })
        .collect();
    let id = montaje
        .transferir(Direccion::Subida, lista, Politica::Sobrescribir, false)
        .await;

    // Una ventana nueva se conecta al mismo servidor: la bienvenida lleva la cola.
    let mut segunda = cliente(
        &servidor::ruta_socket(&montaje.entorno.rutas),
        VERSION_PROTOCOLO,
    )
    .await;
    let bienvenida: MensajeServidor = siguiente(&mut segunda).await;
    match bienvenida {
        MensajeServidor::Bienvenida { transferencias, .. } => {
            assert!(
                transferencias.iter().any(|fila| fila.id == id),
                "la cola viaja en la bienvenida: {transferencias:?}"
            );
        }
        otro => panic!("se esperaba Bienvenida, llegó {otro:?}"),
    }
}

/// El host dialoga por el mismo camino que una sesión cuando no hay conexión:
/// la contraseña llega al solicitante y su respuesta abre el canal SFTP.
#[tokio::test]
async fn el_dialogo_de_contrasena_tambien_sirve_para_abrir_el_sftp() {
    let Some(mut montaje) = montar_con(true, true).await else {
        return;
    };
    let host_id = montaje.host_id;
    montaje.enviar(&MensajeCliente::AbrirSftp { host_id }).await;
    let mut abierto = false;
    let plazo = tokio::time::Instant::now() + Duration::from_secs(20);
    while !abierto {
        assert!(
            tokio::time::Instant::now() < plazo,
            "ni diálogo ni apertura en 20 s"
        );
        let leido = tokio::time::timeout(
            Duration::from_secs(10),
            siguiente_framed::<MensajeServidor, _>(&mut montaje.lector),
        )
        .await;
        let mensaje: MensajeServidor = match leido {
            Ok(mensaje) => mensaje,
            Err(_) => panic!("el servidor no contesta en 10 s"),
        };
        match mensaje {
            MensajeServidor::PideContrasena { sesion_id, .. } => {
                montaje
                    .enviar(&MensajeCliente::Contrasena {
                        sesion_id,
                        contrasena: magi::protocolo::Secreto::nuevo("secreta"),
                        recordar: false,
                    })
                    .await;
            }
            MensajeServidor::SftpAbierto { .. } => abierto = true,
            MensajeServidor::Error { mensaje, .. } => panic!("apertura fallida: {mensaje}"),
            _ => {}
        }
    }
}
