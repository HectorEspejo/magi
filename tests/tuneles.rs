//! Pruebas de extremo a extremo de la Fase 5: túneles locales, dinámicos y
//! remotos contra el servidor SSH en proceso de `comun`, que aquí concede (o
//! rechaza) reenvíos y abre destinos como un `sshd` de verdad.
//!
//! El tráfico de verdad se comprueba contra un servicio de eco local: lo que
//! entra por el túnel tiene que salir por él.

mod comun;

use std::os::unix::fs::PermissionsExt as _;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream, UnixStream};
use tokio_util::codec::FramedRead;

use magi::almacen::{hosts, tuneles as almacen_tuneles, Almacen};
use magi::modelo::{DatosHost, DatosTunel, IdentidadRef, Origen, TipoTunel};
use magi::protocolo::{
    EstadoTunelRemoto, InfoTunel, MensajeCliente, MensajeServidor, VERSION_PROTOCOLO,
};
use magi::servidor;

use comun::*;

type Lector = FramedRead<tokio::net::unix::OwnedReadHalf, magi::protocolo::LinesCodec>;

/// Escenario completo: servidor SSH de pruebas, host en la base de datos,
/// servidor de sesiones y cliente del protocolo.
struct Montaje {
    entorno: Entorno,
    host_id: i64,
    escritura: tokio::net::unix::OwnedWriteHalf,
    lector: Lector,
    /// El host de pruebas, para provocar conexiones entrantes y ver qué pidió.
    reenvios: Arc<Reenvios>,
    /// Última difusión de túneles vista.
    tuneles: Vec<InfoTunel>,
    /// Última difusión de sesiones vista.
    sesiones: Vec<magi::protocolo::InfoSesion>,
    _tarea_ssh: tokio::task::JoinHandle<()>,
    /// Viva mientras dure la prueba: el servidor de sesiones.
    _tarea_magi: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl Montaje {
    async fn enviar(&mut self, mensaje: &MensajeCliente) {
        enviar_a(&mut self.escritura, mensaje).await;
    }

    /// Siguiente mensaje del servidor, con plazo: si no llega, la prueba falla
    /// en vez de quedarse colgada.
    async fn siguiente(&mut self) -> MensajeServidor {
        match tokio::time::timeout(Duration::from_secs(15), siguiente_framed(&mut self.lector))
            .await
        {
            Ok(mensaje) => {
                // Se guardan las listas para no perder una difusión que llegue
                // mientras la prueba espera otra cosa.
                match &mensaje {
                    MensajeServidor::Tuneles { lista } => self.tuneles = lista.clone(),
                    MensajeServidor::Sesiones { lista } => self.sesiones = lista.clone(),
                    _ => {}
                }
                mensaje
            }
            Err(_) => panic!(
                "el servidor no mandó nada en 15 s; la última difusión de túneles fue {:?}",
                self.tuneles
            ),
        }
    }

    /// Lee hasta la próxima difusión de túneles que cumpla el predicado.
    async fn esperar_tuneles<F>(&mut self, mut vale: F) -> Vec<InfoTunel>
    where
        F: FnMut(&[InfoTunel]) -> bool,
    {
        loop {
            if let MensajeServidor::Tuneles { lista } = self.siguiente().await {
                self.tuneles = lista.clone();
                if vale(&lista) {
                    return lista;
                }
            }
        }
    }

    /// Espera a ver el túnel en un estado concreto dentro de la difusión.
    async fn esperar_estado_igual(
        &mut self,
        tunel_id: i64,
        estado: EstadoTunelRemoto,
    ) -> InfoTunel {
        self.esperar_tuneles(|lista| {
            lista
                .iter()
                .any(|tunel| tunel.tunel_id == tunel_id && tunel.estado == estado)
        })
        .await
        .into_iter()
        .find(|tunel| tunel.tunel_id == tunel_id)
        .expect("el túnel está en la lista")
    }

    /// Vigila durante un rato que el túnel no aparezca en marcha en ninguna
    /// difusión. Es lo que hay que comprobar cuando lo correcto es que el
    /// servidor **no** mande nada: si se levantara, difundiría al momento.
    async fn sigue_inactivo(&mut self, tunel_id: i64, plazo: Duration) {
        let fin = tokio::time::Instant::now() + plazo;
        loop {
            let restante = fin.saturating_duration_since(tokio::time::Instant::now());
            if restante.is_zero() {
                return;
            }
            match tokio::time::timeout(restante, siguiente_framed(&mut self.lector)).await {
                Ok(MensajeServidor::Tuneles { lista }) => {
                    assert!(
                        !lista
                            .iter()
                            .any(|tunel| tunel.tunel_id == tunel_id && tunel.estado.en_marcha()),
                        "el túnel se levantó cuando no debía: {lista:?}"
                    );
                    self.tuneles = lista;
                }
                Ok(MensajeServidor::Sesiones { lista }) => self.sesiones = lista,
                Ok(_) => {}
                Err(_) => return,
            }
        }
    }

    /// La difusión solo trae lo que el servidor tiene en memoria: un túnel que
    /// no aparece es que no está levantado. No espera una difusión nueva (si no
    /// ha cambiado nada, no la habrá): mira la última conocida y solo sigue
    /// leyendo mientras el túnel aparezca en marcha.
    async fn esperar_no_activo(&mut self, tunel_id: i64) {
        loop {
            if !self
                .tuneles
                .iter()
                .any(|tunel| tunel.tunel_id == tunel_id && tunel.estado.en_marcha())
            {
                return;
            }
            self.siguiente().await;
        }
    }

    async fn esperar_estado(&mut self, tunel_id: i64, estado: EstadoTunelRemoto) -> InfoTunel {
        self.esperar_tuneles(|lista| {
            lista
                .iter()
                .any(|tunel| tunel.tunel_id == tunel_id && tunel.estado == estado)
        })
        .await
        .into_iter()
        .find(|tunel| tunel.tunel_id == tunel_id)
        .expect("el túnel está en la lista")
    }

    /// Activa un túnel y espera a `Hecho`.
    async fn activar(&mut self, tunel_id: i64) {
        self.enviar(&MensajeCliente::ActivarTunel {
            tunel_id,
            peticion_id: 100 + tunel_id as u64,
        })
        .await;
        self.esperar_hecho(100 + tunel_id as u64).await;
    }

    /// Espera el `Hecho` de una petición; falla si llega un `Error`.
    async fn esperar_hecho(&mut self, peticion_id: u64) {
        let motivo = self.esperar_respuesta(peticion_id).await;
        assert!(motivo.is_none(), "la operación falló: {motivo:?}");
    }

    /// Espera la respuesta de una petición: `None` si fue `Hecho`, el mensaje si
    /// fue `Error`.
    async fn esperar_respuesta(&mut self, peticion_id: u64) -> Option<String> {
        loop {
            match self.siguiente().await {
                MensajeServidor::Hecho { peticion_id: id } if id == peticion_id => return None,
                MensajeServidor::Error {
                    mensaje,
                    peticion_id: Some(id),
                } if id == peticion_id => return Some(mensaje),
                MensajeServidor::Tuneles { lista } => self.tuneles = lista,
                _ => {}
            }
        }
    }

    /// Espera a que haya una sesión abierta con este host y devuelve su id.
    async fn esperar_sesion_abierta(&mut self) -> u32 {
        use magi::protocolo::EstadoSesionRemota;
        loop {
            if let Some(sesion) = self
                .sesiones
                .iter()
                .find(|sesion| sesion.estado == EstadoSesionRemota::Abierta)
            {
                return sesion.id;
            }
            self.siguiente().await;
        }
    }

    /// Abre una conexión al agujero de un túnel local y devuelve el eco.
    async fn hablar_por(&self, escucha: &str, saludo: &str) -> String {
        let (direccion, puerto) = magi::modelo::partir_direccion_puerto(escucha)
            .unwrap_or_else(|| panic!("escucha rara: {escucha}"));
        let mut flujo = TcpStream::connect((direccion.as_str(), puerto))
            .await
            .expect("conectando al túnel");
        flujo.write_all(saludo.as_bytes()).await.expect("saludo");
        let mut bufer = vec![0u8; 256];
        let leidos = tokio::time::timeout(Duration::from_secs(5), flujo.read(&mut bufer))
            .await
            .expect("el eco no llegó")
            .expect("leyendo el eco");
        String::from_utf8_lossy(&bufer[..leidos]).to_string()
    }
}

/// Monta el escenario con un host de pruebas que hace lo que dice `reenvio`.
async fn montar(reenvio: ModoReenvio) -> Montaje {
    montar_con(reenvio, false).await
}

/// Como `montar`, pero el host puede multiplexar la conexión. Multiplexando,
/// la conexión del túnel vive en el pool y sobrevive a los 30 s de gracia, que
/// es donde se nota si parar corta de verdad las conexiones abiertas.
async fn montar_con(reenvio: ModoReenvio, multiplexar: bool) -> Montaje {
    let entorno = entorno();
    let rutas = entorno.rutas.clone();

    // 1. Servidor SSH de pruebas, con clave conocida de antemano.
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
    let escucha = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let puerto_ssh = escucha.local_addr().unwrap().port();
    let reenvios = Arc::new(Reenvios::default());
    let estado_ssh = reenvios.clone();
    let tarea_ssh = tokio::spawn(async move {
        let mut servidor = ServidorSesion {
            reenvio,
            estado: estado_ssh,
            ..Default::default()
        };
        let _ = russh::server::Server::run_on_socket(&mut servidor, config, &escucha).await;
    });

    // 2. Identidad del cliente y huella del host.
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
                identidad_ref: IdentidadRef::Fichero(ruta_clave.display().to_string()),
                multiplexar,
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

    Montaje {
        entorno,
        host_id,
        escritura,
        lector,
        reenvios,
        tuneles: Vec::new(),
        sesiones: Vec::new(),
        _tarea_ssh: tarea_ssh,
        _tarea_magi: tarea_magi,
    }
}

/// Crea un túnel en la tabla (lo que hace el cliente desde su ventana).
fn crear_tunel(montaje: &Montaje, datos: &DatosTunel) -> i64 {
    let almacen = Almacen::abrir(&montaje.entorno.rutas.base_datos()).unwrap();
    let id = almacen_tuneles::crear(almacen.conexion(), datos).expect("creando el túnel");
    almacen.cerrar().unwrap();
    id
}

fn tunel(
    host_id: i64,
    nombre: &str,
    tipo: TipoTunel,
    escucha: &str,
    destino: Option<&str>,
) -> DatosTunel {
    DatosTunel {
        host_id,
        nombre: nombre.to_string(),
        tipo,
        escucha: escucha.to_string(),
        destino: destino.map(str::to_string),
        automatico: false,
    }
}

/// Las últimas filas del registro, para comprobar lo que se anotó.
fn registro(montaje: &Montaje) -> Vec<(String, String)> {
    let almacen = Almacen::abrir(&montaje.entorno.rutas.base_datos()).unwrap();
    let filas = {
        let mut sentencia = almacen
            .conexion()
            .prepare("SELECT tipo, detalle FROM REGISTRO ORDER BY id DESC LIMIT 12")
            .unwrap();
        sentencia
            .query_map([], |fila| Ok((fila.get(0)?, fila.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<(String, String)>>>()
            .unwrap()
    };
    almacen.cerrar().unwrap();
    filas
}

/// Un túnel local lleva el tráfico y va contando bytes y conexiones.
#[tokio::test]
async fn el_tunel_local_lleva_el_trafico_y_cuenta_los_bytes() {
    let (puerto_eco, _eco) = servicio_eco().await;
    let mut montaje = montar(ModoReenvio::Concede).await;
    let tunel_id = crear_tunel(
        &montaje,
        &tunel(
            montaje.host_id,
            "eco",
            TipoTunel::Local,
            "127.0.0.1:0",
            Some(&format!("127.0.0.1:{puerto_eco}")),
        ),
    );

    montaje.activar(tunel_id).await;
    let activo = montaje
        .esperar_estado(tunel_id, EstadoTunelRemoto::Activo)
        .await;
    // Con el puerto 0 escucha donde el sistema diga y se enseña ese puerto.
    assert_ne!(activo.escucha_efectiva, "127.0.0.1:0", "{activo:?}");

    assert_eq!(
        montaje.hablar_por(&activo.escucha_efectiva, "hola").await,
        "hola"
    );

    // Los contadores se difunden coalescidos: se espera a que crezcan.
    let con_datos = montaje
        .esperar_tuneles(|lista| {
            lista
                .iter()
                .any(|tunel| tunel.tunel_id == tunel_id && tunel.bytes_subidos > 0)
        })
        .await;
    let info = con_datos
        .into_iter()
        .find(|tunel| tunel.tunel_id == tunel_id)
        .unwrap();
    assert!(info.aceptadas >= 1, "{info:?}");
    assert!(info.bytes_subidos >= 4, "{info:?}");
    assert!(info.bytes_bajados >= 4, "{info:?}");

    // Y en el registro queda constancia de que se abrió.
    assert!(
        registro(&montaje)
            .iter()
            .any(|(tipo, _)| tipo == "tunel_abierto"),
        "no se anotó la apertura del túnel"
    );
}

/// El dinámico atiende SOCKS5 con dominio (sin resolverlo en local) y con IP.
#[tokio::test]
async fn el_tunel_dinamico_atiende_socks5_con_dominio_y_con_ip() {
    let (puerto_eco, _eco) = servicio_eco().await;
    let mut montaje = montar(ModoReenvio::Concede).await;
    let tunel_id = crear_tunel(
        &montaje,
        &tunel(
            montaje.host_id,
            "socks",
            TipoTunel::Dinamico,
            "127.0.0.1:0",
            None,
        ),
    );

    montaje.activar(tunel_id).await;
    let activo = montaje
        .esperar_estado(tunel_id, EstadoTunelRemoto::Activo)
        .await;
    let (_, puerto_socks) =
        magi::modelo::partir_direccion_puerto(&activo.escucha_efectiva).unwrap();

    // El nombre viaja tal cual al host: quien resuelve es él.
    assert_eq!(
        socks5_conectar(puerto_socks, "localhost", puerto_eco, "hola").await,
        Some("hola".to_string())
    );
    // Y una IP también.
    assert_eq!(
        socks5_conectar(puerto_socks, "127.0.0.1", puerto_eco, "adios").await,
        Some("adios".to_string())
    );
}

/// El remoto trae hasta el servicio local la petición que entra en el host.
#[tokio::test]
async fn el_tunel_remoto_trae_la_peticion_del_host() {
    let (puerto_eco, _eco) = servicio_eco().await;
    let mut montaje = montar(ModoReenvio::Concede).await;
    let tunel_id = crear_tunel(
        &montaje,
        &tunel(
            montaje.host_id,
            "webhook",
            TipoTunel::Remoto,
            "127.0.0.1:0",
            Some(&format!("127.0.0.1:{puerto_eco}")),
        ),
    );

    montaje.activar(tunel_id).await;
    let activo = montaje
        .esperar_estado(tunel_id, EstadoTunelRemoto::Activo)
        .await;
    let (direccion, puerto) =
        magi::modelo::partir_direccion_puerto(&activo.escucha_efectiva).unwrap();

    // Como si en el host alguien hiciera `curl localhost:<puerto>`.
    let respuesta = montaje
        .reenvios
        .conectar_de_vuelta(&direccion, u32::from(puerto), "hola")
        .await;
    assert_eq!(respuesta, "hola");

    let con_datos = montaje
        .esperar_tuneles(|lista| {
            lista
                .iter()
                .any(|tunel| tunel.tunel_id == tunel_id && tunel.aceptadas > 0)
        })
        .await;
    let info = con_datos
        .into_iter()
        .find(|tunel| tunel.tunel_id == tunel_id)
        .unwrap();
    assert_eq!(info.aceptadas, 1, "{info:?}");

    // Parar cancela el reenvío en el host sin tocar nada más.
    montaje
        .enviar(&MensajeCliente::PararTunel {
            tunel_id,
            peticion_id: 7,
        })
        .await;
    montaje.esperar_hecho(7).await;
    assert!(
        montaje
            .reenvios
            .cancelados
            .lock()
            .expect("cancelados")
            .contains(&(direccion, u32::from(puerto))),
        "el host no recibió la cancelación del reenvío"
    );
    montaje.esperar_no_activo(tunel_id).await;
}

/// Un puerto que ya está ocupado deja el túnel caído con motivo legible.
#[tokio::test]
async fn un_bind_ocupado_deja_el_tunel_caido_con_motivo() {
    // Se ocupa el puerto desde la prueba, como si lo tuviera otra aplicación.
    let ocupado = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let puerto = ocupado.local_addr().unwrap().port();
    let mut montaje = montar(ModoReenvio::Concede).await;
    let tunel_id = crear_tunel(
        &montaje,
        &tunel(
            montaje.host_id,
            "ocupado",
            TipoTunel::Local,
            &format!("127.0.0.1:{puerto}"),
            Some("127.0.0.1:9"),
        ),
    );

    montaje
        .enviar(&MensajeCliente::ActivarTunel {
            tunel_id,
            peticion_id: 5,
        })
        .await;
    let motivo = montaje.esperar_respuesta(5).await.expect("debe fallar");
    assert!(motivo.contains("uso"), "{motivo}");

    let caido = montaje
        .esperar_estado(tunel_id, EstadoTunelRemoto::Caido)
        .await;
    assert!(caido.ultimo_error.unwrap_or_default().contains("uso"));
    assert!(
        registro(&montaje)
            .iter()
            .any(|(tipo, detalle)| tipo == "tunel_fallido" && detalle.contains("uso")),
        "no se anotó el fallo"
    );

    // Descartar el error deja la fila lista para volver a intentarlo.
    montaje
        .enviar(&MensajeCliente::PararTunel {
            tunel_id,
            peticion_id: 6,
        })
        .await;
    montaje.esperar_hecho(6).await;
    montaje.esperar_no_activo(tunel_id).await;
}

/// Si el host rechaza el reenvío, el túnel queda caído con ese motivo.
#[tokio::test]
async fn el_rechazo_del_host_deja_el_tunel_caido() {
    let mut montaje = montar(ModoReenvio::Rechaza).await;
    let tunel_id = crear_tunel(
        &montaje,
        &tunel(
            montaje.host_id,
            "webhook",
            TipoTunel::Remoto,
            "127.0.0.1:0",
            Some("127.0.0.1:9"),
        ),
    );
    montaje
        .enviar(&MensajeCliente::ActivarTunel {
            tunel_id,
            peticion_id: 9,
        })
        .await;
    let motivo = montaje.esperar_respuesta(9).await.expect("debe fallar");
    assert!(motivo.contains("rechazó"), "{motivo}");
    assert!(montaje
        .esperar_estado_igual(tunel_id, EstadoTunelRemoto::Caido)
        .await
        .ultimo_error
        .as_deref()
        .is_some());
}

/// Un destino que el host no puede abrir cierra esa conexión y deja el error,
/// pero el túnel sigue vivo (y se puede volver a usar).
#[tokio::test]
async fn el_rechazo_de_una_conexion_no_tumba_el_tunel() {
    let mut montaje = montar(ModoReenvio::Concede).await;
    // Puerto 1: nadie escucha ahí, así que el host no puede conectarlo.
    let tunel_id = crear_tunel(
        &montaje,
        &tunel(
            montaje.host_id,
            "cerrado",
            TipoTunel::Local,
            "127.0.0.1:0",
            Some("127.0.0.1:1"),
        ),
    );
    montaje.activar(tunel_id).await;
    let activo = montaje
        .esperar_estado(tunel_id, EstadoTunelRemoto::Activo)
        .await;

    let (direccion, puerto) =
        magi::modelo::partir_direccion_puerto(&activo.escucha_efectiva).unwrap();
    // La conexión se cierra: lo que se lea es un final de fichero.
    let mut flujo = TcpStream::connect((direccion.as_str(), puerto))
        .await
        .unwrap();
    let mut bufer = vec![0u8; 16];
    let leidos = tokio::time::timeout(Duration::from_secs(5), flujo.read(&mut bufer))
        .await
        .expect("la conexión no se cerró")
        .unwrap_or(0);
    assert_eq!(leidos, 0, "el host no debía entregar nada");

    let con_error = montaje
        .esperar_tuneles(|lista| {
            lista
                .iter()
                .any(|tunel| tunel.tunel_id == tunel_id && tunel.ultimo_error.is_some())
        })
        .await;
    let info = con_error
        .into_iter()
        .find(|tunel| tunel.tunel_id == tunel_id)
        .unwrap();
    assert_eq!(
        info.estado,
        EstadoTunelRemoto::Activo,
        "el túnel sigue vivo"
    );
    assert!(info.aceptadas == 0, "{info:?}");
}

/// Parar un túnel con una conexión abierta la corta y anota los totales.
#[tokio::test]
async fn parar_corta_las_conexiones_abiertas_y_anota_los_totales() {
    let (puerto_eco, _eco) = servicio_eco().await;
    let mut montaje = montar(ModoReenvio::Concede).await;
    let tunel_id = crear_tunel(
        &montaje,
        &tunel(
            montaje.host_id,
            "eco",
            TipoTunel::Local,
            "127.0.0.1:0",
            Some(&format!("127.0.0.1:{puerto_eco}")),
        ),
    );
    montaje.activar(tunel_id).await;
    let activo = montaje
        .esperar_estado(tunel_id, EstadoTunelRemoto::Activo)
        .await;
    let (direccion, puerto) =
        magi::modelo::partir_direccion_puerto(&activo.escucha_efectiva).unwrap();

    // Una conexión abierta y con tráfico.
    let mut flujo = TcpStream::connect((direccion.as_str(), puerto))
        .await
        .unwrap();
    flujo.write_all(b"hola").await.unwrap();
    let mut bufer = vec![0u8; 16];
    let leidos = flujo.read(&mut bufer).await.unwrap();
    assert_eq!(&bufer[..leidos], b"hola");
    montaje
        .esperar_tuneles(|lista| {
            lista
                .iter()
                .any(|tunel| tunel.tunel_id == tunel_id && tunel.conexiones == 1)
        })
        .await;

    montaje
        .enviar(&MensajeCliente::PararTunel {
            tunel_id,
            peticion_id: 11,
        })
        .await;
    montaje.esperar_hecho(11).await;

    // La conexión abierta se corta.
    let leidos = tokio::time::timeout(Duration::from_secs(5), flujo.read(&mut bufer))
        .await
        .expect("la conexión no se cortó")
        .unwrap_or(0);
    assert_eq!(leidos, 0);

    montaje.esperar_no_activo(tunel_id).await;
    let cierres = registro(&montaje);
    assert!(
        cierres
            .iter()
            .any(|(tipo, detalle)| tipo == "tunel_cerrado" && detalle.contains("conexión")),
        "no se anotó el cierre con los totales: {cierres:?}"
    );
}

/// El ciclo automático: se levanta con la primera pestaña y se para con la
/// última, y un túnel manual no se toca.
#[tokio::test]
async fn el_ciclo_automatico_respeta_los_manuales() {
    let (puerto_eco, _eco) = servicio_eco().await;
    let mut montaje = montar(ModoReenvio::Concede).await;

    let mut automatico = tunel(
        montaje.host_id,
        "auto",
        TipoTunel::Local,
        "127.0.0.1:0",
        Some(&format!("127.0.0.1:{puerto_eco}")),
    );
    automatico.automatico = true;
    let id_auto = crear_tunel(&montaje, &automatico);
    let id_manual = crear_tunel(
        &montaje,
        &tunel(
            montaje.host_id,
            "a-mano",
            TipoTunel::Local,
            "127.0.0.1:0",
            Some(&format!("127.0.0.1:{puerto_eco}")),
        ),
    );

    // Con una pestaña abierta, el automático se levanta solo.
    montaje
        .enviar(&MensajeCliente::AbrirSesion {
            host_id: montaje.host_id,
            cols: 80,
            filas: 24,
        })
        .await;
    let auto = montaje
        .esperar_estado(id_auto, EstadoTunelRemoto::Activo)
        .await;
    assert_eq!(auto.origen, Some(magi::protocolo::OrigenTunel::Automatico));
    montaje.esperar_no_activo(id_manual).await;

    // Se levanta el manual a mano y se cierra la pestaña.
    montaje.activar(id_manual).await;
    let sesion_id = montaje.esperar_sesion_abierta().await;
    montaje.enviar(&MensajeCliente::Cerrar { sesion_id }).await;

    // El automático se para con la última pestaña; el manual sigue vivo.
    // Al pararse, el automático desaparece de la difusión: lo que se espera es
    // que deje de estar en marcha, no que siga en la lista.
    montaje.esperar_no_activo(id_auto).await;
    let manual = montaje
        .tuneles
        .iter()
        .find(|tunel| tunel.tunel_id == id_manual)
        .expect("el manual sigue en la lista");
    assert_eq!(
        manual.estado,
        EstadoTunelRemoto::Activo,
        "el manual no se para con la última pestaña"
    );
}

/// Un reenvío remoto con un puerto **concreto** se registra en ese puerto: el
/// host no contesta con ninguno cuando no se le pidió el 0, y quedarse con ese
/// silencio dejaría el reenvío apuntando al puerto 0 (canales rechazados y
/// escucha imposible de cancelar).
#[tokio::test]
async fn el_reenvio_remoto_con_puerto_fijo_usa_ese_puerto() {
    let (puerto_eco, _eco) = servicio_eco().await;
    // Un puerto libre para pedirlo tal cual.
    let puerto_fijo = TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let mut montaje = montar(ModoReenvio::Concede).await;
    let tunel_id = crear_tunel(
        &montaje,
        &tunel(
            montaje.host_id,
            "webhook",
            TipoTunel::Remoto,
            &format!("127.0.0.1:{puerto_fijo}"),
            Some(&format!("127.0.0.1:{puerto_eco}")),
        ),
    );

    montaje.activar(tunel_id).await;
    let activo = montaje
        .esperar_estado(tunel_id, EstadoTunelRemoto::Activo)
        .await;
    assert_eq!(
        activo.escucha_efectiva,
        format!("127.0.0.1:{puerto_fijo}"),
        "la escucha efectiva debe ser la pedida, no un 0"
    );
    assert_eq!(
        montaje
            .reenvios
            .concedidos
            .lock()
            .expect("concedidos")
            .last(),
        Some(&("127.0.0.1".to_string(), u32::from(puerto_fijo)))
    );

    // Y el canal que abre el host en ese puerto llega al servicio local.
    assert_eq!(
        montaje
            .reenvios
            .conectar_de_vuelta("127.0.0.1", u32::from(puerto_fijo), "hola")
            .await,
        "hola"
    );
}

/// Parar corta las conexiones abiertas aunque la conexión del host siga viva
/// (host que multiplexa y conexión en el pool): si no, seguirían llevando
/// tráfico de un túnel que ya no existe.
#[tokio::test]
async fn parar_corta_las_conexiones_aunque_el_host_multiplexe() {
    let (puerto_eco, _eco) = servicio_eco().await;
    let mut montaje = montar_con(ModoReenvio::Concede, true).await;
    let tunel_id = crear_tunel(
        &montaje,
        &tunel(
            montaje.host_id,
            "eco",
            TipoTunel::Local,
            "127.0.0.1:0",
            Some(&format!("127.0.0.1:{puerto_eco}")),
        ),
    );
    montaje.activar(tunel_id).await;
    let activo = montaje
        .esperar_estado(tunel_id, EstadoTunelRemoto::Activo)
        .await;
    let (direccion, puerto) =
        magi::modelo::partir_direccion_puerto(&activo.escucha_efectiva).unwrap();

    let mut flujo = TcpStream::connect((direccion.as_str(), puerto))
        .await
        .unwrap();
    flujo.write_all(b"hola").await.unwrap();
    let mut bufer = vec![0u8; 16];
    let leidos = flujo.read(&mut bufer).await.unwrap();
    assert_eq!(&bufer[..leidos], b"hola");
    // La conexión está contada como abierta.
    montaje
        .esperar_tuneles(|lista| {
            lista
                .iter()
                .any(|tunel| tunel.tunel_id == tunel_id && tunel.conexiones == 1)
        })
        .await;

    montaje
        .enviar(&MensajeCliente::PararTunel {
            tunel_id,
            peticion_id: 21,
        })
        .await;
    montaje.esperar_hecho(21).await;

    // Con la conexión del pool viva, el corte tiene que venir del túnel.
    let leidos = tokio::time::timeout(Duration::from_secs(5), flujo.read(&mut bufer))
        .await
        .expect("la conexión no se cortó al parar el túnel")
        .unwrap_or(0);
    assert_eq!(leidos, 0);
}

/// Un automático parado a mano no vuelve hasta el ciclo siguiente.
#[tokio::test]
async fn un_automatico_parado_a_mano_no_vuelve_en_el_mismo_ciclo() {
    let (puerto_eco, _eco) = servicio_eco().await;
    let mut montaje = montar(ModoReenvio::Concede).await;
    let mut automatico = tunel(
        montaje.host_id,
        "auto",
        TipoTunel::Local,
        "127.0.0.1:0",
        Some(&format!("127.0.0.1:{puerto_eco}")),
    );
    automatico.automatico = true;
    let id_auto = crear_tunel(&montaje, &automatico);

    // Con la primera pestaña se levanta solo.
    montaje
        .enviar(&MensajeCliente::AbrirSesion {
            host_id: montaje.host_id,
            cols: 80,
            filas: 24,
        })
        .await;
    montaje
        .esperar_estado(id_auto, EstadoTunelRemoto::Activo)
        .await;

    // El usuario lo para a mano: no debe volver a levantarse en este ciclo.
    montaje
        .enviar(&MensajeCliente::PararTunel {
            tunel_id: id_auto,
            peticion_id: 31,
        })
        .await;
    montaje.esperar_hecho(31).await;
    montaje.esperar_no_activo(id_auto).await;

    // Cualquier otro cambio del ciclo (aquí, otra pestaña al mismo host) no lo
    // resucita.
    montaje
        .enviar(&MensajeCliente::AbrirSesion {
            host_id: montaje.host_id,
            cols: 80,
            filas: 24,
        })
        .await;
    montaje.esperar_sesion_abierta().await;
    // Si volviera a levantarse, la difusión saldría al momento: se vigila un
    // rato y no debe aparecer.
    montaje
        .sigue_inactivo(id_auto, Duration::from_millis(1500))
        .await;
}
