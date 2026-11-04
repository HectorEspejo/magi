//! Prueba de extremo a extremo del túnel local del móvil, sin red externa:
//! servidor SSH en proceso (russh) + servicio de eco local como destino.
//!
//! Lo que entra por el agujero del túnel tiene que salir por él y volver. Es la
//! prueba que faltaba en la Fase 11 y que debe fallar si el reenvío no funciona.

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt as _;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use magi_movil::api::{
    ErrorNucleo, EstadoSincronizacion, EstadoTunel, Pantalla, PeticionDialogo, Retrollamadas,
    Sesion, TunelVivo,
};
use magi_movil::estado::{con_almacen, EstadoNucleo};
use magi_movil::retrollamadas::{
    CarpetaMovil, FicheroCarpeta, FirmanteMovil, LlaveroMovil, OyenteNucleo,
};
use magi_nucleo::almacen::movil as movil_bd;
use magi_nucleo::almacen::versiones::Tabla;
use magi_nucleo::almacen::{hosts, tuneles as almacen_tuneles};
use magi_nucleo::modelo::{DatosHost, DatosTunel, Origen, TipoTunel};
use magi_nucleo::rutas::{Equipo, Rutas};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

struct CarpetaVacia;

impl CarpetaMovil for CarpetaVacia {
    fn listar(&self) -> Result<Vec<FicheroCarpeta>, ErrorNucleo> {
        Ok(Vec::new())
    }

    fn leer(&self, _nombre: String, _maximo: u64) -> Result<Vec<u8>, ErrorNucleo> {
        Err(ErrorNucleo::Carpeta {
            detalle: "carpeta vacía".to_string(),
        })
    }
}

#[derive(Clone, Default)]
struct LlaveroMemoria {
    secretos: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl LlaveroMemoria {
    fn guardar_en_memoria(&self, cuenta: &str, valor: &[u8]) {
        self.secretos
            .lock()
            .expect("llavero")
            .insert(cuenta.to_string(), valor.to_vec());
    }
}

impl LlaveroMovil for LlaveroMemoria {
    fn leer(&self, clave: String) -> Result<Option<Vec<u8>>, ErrorNucleo> {
        Ok(self.secretos.lock().expect("llavero").get(&clave).cloned())
    }

    fn guardar(&self, clave: String, secreto: Vec<u8>) -> Result<(), ErrorNucleo> {
        self.secretos
            .lock()
            .expect("llavero")
            .insert(clave, secreto);
        Ok(())
    }

    fn borrar(&self, clave: String) -> Result<(), ErrorNucleo> {
        self.secretos.lock().expect("llavero").remove(&clave);
        Ok(())
    }
}

/// Clave P-256 de pruebas: la cadena del móvil autentica con la clave pública
/// y el servidor de pruebas acepta cualquier firma (no la verifica).
struct FirmantePrueba {
    clave: p256::ecdsa::SigningKey,
}

/// Cabecera SPKI de una clave pública EC P-256 sin comprimir.
const PREFIJO_SPKI: [u8; 26] = [
    0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a,
    0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00,
];

impl FirmantePrueba {
    fn nuevo() -> Self {
        let clave = p256::ecdsa::SigningKey::from_bytes(&p256::FieldBytes::from([7u8; 32]))
            .expect("clave P-256 de pruebas");
        Self { clave }
    }
}

impl FirmanteMovil for FirmantePrueba {
    fn clave_publica_x509(&self) -> Result<Vec<u8>, ErrorNucleo> {
        let punto = self.clave.verifying_key().to_sec1_bytes();
        assert_eq!(punto.first(), Some(&0x04), "punto SEC1 sin comprimir");
        let mut der = PREFIJO_SPKI.to_vec();
        der.extend_from_slice(&punto);
        Ok(der)
    }

    fn firmar(&self, datos: Vec<u8>) -> Result<Vec<u8>, ErrorNucleo> {
        use p256::ecdsa::signature::Signer as _;
        let firma: p256::ecdsa::Signature = self.clave.sign(datos.as_slice());
        Ok(firma.to_der().as_bytes().to_vec())
    }

    fn olvidar_clave(&self) -> Result<(), ErrorNucleo> {
        Ok(())
    }
}

#[derive(Default)]
struct OyenteGuardado {
    tuneles: Mutex<Vec<TunelVivo>>,
    sesiones: Mutex<Vec<Sesion>>,
}

impl OyenteGuardado {
    fn tuneles(&self) -> Vec<TunelVivo> {
        self.tuneles.lock().expect("oyente").clone()
    }
}

impl OyenteNucleo for OyenteGuardado {
    fn pantalla(&self, _pantalla: Pantalla) {}

    fn sesiones(&self, lista: Vec<Sesion>) {
        *self.sesiones.lock().expect("oyente") = lista;
    }

    fn tuneles(&self, lista: Vec<TunelVivo>) {
        *self.tuneles.lock().expect("oyente") = lista;
    }

    fn dialogo(&self, peticion: PeticionDialogo) {
        eprintln!("DIALOGO PEDIDO: {peticion:?}");
    }

    fn sincronizacion(&self, _estado: EstadoSincronizacion) {}
}

// ------------------------------------------------------------ servidor SSH

struct ServidorPruebas {
    mangos: Arc<Mutex<Vec<russh::server::Handle>>>,
}

impl russh::server::Server for ServidorPruebas {
    type Handler = HandlerPruebas;

    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> HandlerPruebas {
        HandlerPruebas {
            mangos: self.mangos.clone(),
        }
    }
}

struct HandlerPruebas {
    mangos: Arc<Mutex<Vec<russh::server::Handle>>>,
}

impl russh::server::Handler for HandlerPruebas {
    type Error = russh::Error;

    async fn auth_password(
        &mut self,
        _usuario: &str,
        contrasena: &str,
    ) -> Result<russh::server::Auth, Self::Error> {
        if contrasena == "secreta" {
            Ok(russh::server::Auth::Accept)
        } else {
            Ok(russh::server::Auth::reject())
        }
    }

    async fn auth_publickey(
        &mut self,
        _usuario: &str,
        _clave: &russh::keys::PublicKey,
    ) -> Result<russh::server::Auth, Self::Error> {
        // Pruebas: la clave pública vale, no se verifica la firma.
        Ok(russh::server::Auth::Accept)
    }

    /// Como un sshd con AllowTcpForwarding: abre el destino y bombea bytes.
    async fn channel_open_direct_tcpip(
        &mut self,
        canal: russh::Channel<russh::server::Msg>,
        host: &str,
        puerto: u32,
        _origen: &str,
        _puerto_origen: u32,
        respuesta: russh::server::ChannelOpenHandle,
        sesion: &mut russh::server::Session,
    ) -> Result<(), Self::Error> {
        self.mangos.lock().expect("mangos").push(sesion.handle());
        let conexion = tokio::time::timeout(
            Duration::from_secs(5),
            TcpStream::connect((host, puerto as u16)),
        )
        .await;
        let Ok(Ok(mut tcp)) = conexion else {
            respuesta
                .reject(russh::ChannelOpenFailure::ConnectFailed)
                .await;
            return Ok(());
        };
        respuesta.accept().await;
        tokio::spawn(async move {
            let mut flujo = canal.into_stream();
            let _ = tokio::io::copy_bidirectional(&mut tcp, &mut flujo).await;
        });
        Ok(())
    }
}

// ------------------------------------------------------------ escenario

/// Servicio de eco: devuelve lo que recibe.
async fn servir_eco() -> u16 {
    let escucha = TcpListener::bind(("127.0.0.1", 0)).await.expect("eco");
    let puerto = escucha.local_addr().expect("eco").port();
    tokio::spawn(async move {
        loop {
            let Ok((mut flujo, _)) = escucha.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut bufer = vec![0u8; 1024];
                loop {
                    match flujo.read(&mut bufer).await {
                        Ok(0) | Err(_) => return,
                        Ok(leidos) => {
                            if flujo.write_all(&bufer[..leidos]).await.is_err() {
                                return;
                            }
                        }
                    }
                }
            });
        }
    });
    puerto
}

/// Servidor SSH de pruebas; devuelve puerto, clave pública, mangos y tarea.
async fn servir_ssh() -> (
    u16,
    ssh_key::PublicKey,
    Arc<Mutex<Vec<russh::server::Handle>>>,
    tokio::task::JoinHandle<()>,
) {
    let clave = ssh_key::PrivateKey::random(
        &mut ssh_key::rand_core::UnwrapErr(ssh_key::getrandom::SysRng),
        ssh_key::Algorithm::Ed25519,
    )
    .expect("clave de host");
    let config = Arc::new(russh::server::Config {
        auth_rejection_time: Duration::from_millis(1),
        keys: vec![clave.clone()],
        ..Default::default()
    });
    let escucha = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("escucha ssh");
    let puerto = escucha.local_addr().expect("ssh").port();
    let mangos = Arc::new(Mutex::new(Vec::new()));
    let mangos_servidor = mangos.clone();
    let tarea = tokio::spawn(async move {
        let mut servidor = ServidorPruebas {
            mangos: mangos_servidor,
        };
        let _ = russh::server::Server::run_on_socket(&mut servidor, config, &escucha).await;
    });
    (puerto, clave.public_key().clone(), mangos, tarea)
}

struct Montaje {
    _temporal: tempfile::TempDir,
    estado: Arc<EstadoNucleo>,
    oyente: Arc<OyenteGuardado>,
    host_uuid: String,
    tunel_uuid: String,
    puerto_tunel: u16,
    mangos_ssh: Arc<Mutex<Vec<russh::server::Handle>>>,
    _tarea_ssh: tokio::task::JoinHandle<()>,
}

async fn montar() -> Montaje {
    let puerto_eco = servir_eco().await;
    let (puerto_ssh, clave_host, mangos_ssh, tarea_ssh) = servir_ssh().await;

    let temporal = tempfile::tempdir().expect("temporal");
    let base = temporal.path();
    let rutas = Rutas::nuevas(
        base.join("datos"),
        base.join("config"),
        base.join("estado"),
        base.join("hogar"),
        base.join("runtime"),
    );
    std::fs::create_dir_all(&rutas.datos).expect("datos");
    std::fs::create_dir_all(rutas.dir_ssh()).expect("ssh");

    let ruta_conocidos = rutas.fichero_known_hosts();
    std::fs::write(
        &ruta_conocidos,
        format!(
            "[127.0.0.1]:{puerto_ssh} {}\n",
            clave_host.to_openssh().expect("pública")
        ),
    )
    .expect("known_hosts");
    std::fs::set_permissions(&ruta_conocidos, std::fs::Permissions::from_mode(0o600))
        .expect("permisos");

    let llavero = LlaveroMemoria::default();
    let oyente = Arc::new(OyenteGuardado::default());
    let estado = EstadoNucleo::nuevo(
        rutas,
        Equipo::nuevo("pruebas", "android"),
        Retrollamadas {
            carpeta: Arc::new(CarpetaVacia),
            llavero: Arc::new(llavero.clone()),
            firmante: Arc::new(FirmantePrueba::nuevo()),
            oyente: oyente.clone(),
        },
    )
    .expect("estado del núcleo");

    let host_id = con_almacen(&estado, move |almacen| {
        hosts::crear(
            almacen.conexion(),
            &DatosHost {
                nombre: "prueba".to_string(),
                direccion: "127.0.0.1".to_string(),
                puerto: puerto_ssh,
                usuario: Some("usuario".to_string()),
                ..DatosHost::default()
            },
            Origen::Manual,
        )
    })
    .expect("escritor")
    .expect("host");
    let uuid_host = con_almacen(&estado, move |almacen| {
        movil_bd::uuid_por_id(almacen.conexion(), Tabla::Hosts)
            .map(|mapa| mapa.get(&host_id).cloned())
    })
    .expect("escritor")
    .expect("mapa")
    .expect("uuid del host");
    llavero.guardar_en_memoria(&format!("contrasena:{uuid_host}"), b"secreta");

    // Puerto libre para la escucha del túnel (se libera justo antes de usarlo).
    let escucha = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("puerto libre");
    let puerto_tunel = escucha.local_addr().expect("puerto").port();
    drop(escucha);

    let host_id_tunel = host_id;
    let tunel_id = con_almacen(&estado, move |almacen| {
        almacen_tuneles::crear(
            almacen.conexion(),
            &DatosTunel {
                host_id: host_id_tunel,
                nombre: "web".to_string(),
                tipo: TipoTunel::Local,
                escucha: format!("127.0.0.1:{puerto_tunel}"),
                destino: Some(format!("127.0.0.1:{puerto_eco}")),
                automatico: false,
            },
        )
    })
    .expect("escritor")
    .expect("túnel");
    let tunel_uuid = con_almacen(&estado, move |almacen| {
        movil_bd::uuid_por_id(almacen.conexion(), Tabla::Tuneles)
            .map(|mapa| mapa.get(&tunel_id).cloned())
    })
    .expect("escritor")
    .expect("mapa")
    .expect("uuid del túnel");

    Montaje {
        _temporal: temporal,
        estado,
        oyente,
        host_uuid: uuid_host,
        tunel_uuid,
        puerto_tunel,
        mangos_ssh,
        _tarea_ssh: tarea_ssh,
    }
}

fn tunel_por_uuid(oyente: &OyenteGuardado, uuid: &str) -> Option<TunelVivo> {
    oyente
        .tuneles()
        .into_iter()
        .find(|tunel| tunel.uuid == uuid)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn el_tunel_local_del_movil_reenvia_de_verdad() {
    let montaje = montar().await;
    tokio::time::timeout(
        Duration::from_secs(15),
        magi_movil::gestor::conexion_para_host(&montaje.estado, &montaje.host_uuid),
    )
    .await
    .expect("la conexión del host se colgó")
    .expect("conectando al host");
    tokio::time::timeout(
        Duration::from_secs(20),
        magi_movil::gestor::activar_tunel(&montaje.estado, &montaje.tunel_uuid, false),
    )
    .await
    .expect("la activación del túnel se colgó")
    .expect("activando el túnel");

    let tunel = tunel_por_uuid(&montaje.oyente, &montaje.tunel_uuid).expect("fila del túnel");
    assert_eq!(tunel.estado, EstadoTunel::Activo, "{tunel:?}");
    assert_eq!(
        tunel.escucha,
        format!("127.0.0.1:{}", montaje.puerto_tunel),
        "{tunel:?}"
    );

    let mut flujo = TcpStream::connect(("127.0.0.1", montaje.puerto_tunel))
        .await
        .expect("conectando al agujero del túnel");
    flujo
        .write_all(b"hola por el tunel\n")
        .await
        .expect("escribiendo");
    let mut bufer = vec![0u8; 256];
    let leidos = tokio::time::timeout(Duration::from_secs(5), flujo.read(&mut bufer))
        .await
        .expect("el eco no llegó por el túnel")
        .expect("leyendo el eco");
    assert_eq!(&bufer[..leidos], b"hola por el tunel\n");

    // Los contadores tienen que reflejarlo.
    let mut contadores = (0u32, 0u32, 0u64, 0u64);
    for _ in 0..100 {
        let lista = magi_movil::gestor::tuneles(&montaje.estado, None).expect("túneles");
        if let Some(tunel) = lista.into_iter().find(|t| t.uuid == montaje.tunel_uuid) {
            contadores = (
                tunel.conexiones,
                tunel.aceptadas,
                tunel.bytes_subida,
                tunel.bytes_bajada,
            );
            if contadores.1 >= 1 && contadores.2 > 0 && contadores.3 > 0 {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    assert!(
        contadores.1 >= 1 && contadores.2 > 0 && contadores.3 > 0,
        "contadores sin tráfico: {contadores:?}"
    );
    // Con el interruptor normal no se marca «puerto libre».
    let fila = magi_movil::gestor::tuneles(&montaje.estado, None)
        .expect("túneles")
        .into_iter()
        .find(|t| t.uuid == montaje.tunel_uuid)
        .expect("fila");
    assert_eq!(fila.puerto_efectivo, None, "{fila:?}");

    // «Abrir en un puerto libre» sí lo marca y la escucha es la de verdad.
    magi_movil::gestor::parar_tunel(&montaje.estado, &montaje.tunel_uuid)
        .await
        .expect("parando el túnel");
    magi_movil::gestor::activar_tunel(&montaje.estado, &montaje.tunel_uuid, true)
        .await
        .expect("activando en puerto libre");
    let libre = magi_movil::gestor::tuneles(&montaje.estado, None)
        .expect("túneles")
        .into_iter()
        .find(|t| t.uuid == montaje.tunel_uuid)
        .expect("fila libre");
    let puerto = libre.puerto_efectivo.expect("puerto libre marcado");
    assert_eq!(libre.escucha, format!("127.0.0.1:{puerto}"), "{libre:?}");

    // El runtime del núcleo no se puede soltar dentro del contexto async del
    // test; se deja al proceso (los temporales se limpian al terminar).
    std::mem::forget(montaje);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn si_cae_la_conexion_del_host_el_tunel_queda_caido() {
    let montaje = montar().await;
    magi_movil::gestor::activar_tunel(&montaje.estado, &montaje.tunel_uuid, false)
        .await
        .expect("activando el túnel");

    // Alguien conecta al agujero: eso abre el canal y guarda el mango.
    let mut flujo = TcpStream::connect(("127.0.0.1", montaje.puerto_tunel))
        .await
        .expect("conectando");
    flujo.write_all(b"x").await.expect("escribiendo");
    let mut bufer = [0u8; 1];
    flujo.read_exact(&mut bufer).await.expect("eco");
    drop(flujo);

    // Se tira la conexión SSH del host, como un cambio de red.
    let mangos: Vec<russh::server::Handle> = {
        let mut guardia = montaje.mangos_ssh.lock().expect("mangos");
        std::mem::take(&mut *guardia)
    };
    for mango in mangos {
        let _ = mango
            .disconnect(
                russh::Disconnect::ByApplication,
                "".to_string(),
                "".to_string(),
            )
            .await;
    }

    let mut caido = None;
    for _ in 0..200 {
        let lista = magi_movil::gestor::tuneles(&montaje.estado, None).expect("túneles");
        if let Some(tunel) = lista.into_iter().find(|t| t.uuid == montaje.tunel_uuid) {
            if tunel.estado == magi_movil::api::EstadoTunel::Caido {
                caido = Some(tunel);
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let caido = caido.expect("el túnel no pasó a caído tras perder la conexión");
    let motivo = caido.error.unwrap_or_default();
    assert!(motivo.contains("conexión"), "motivo poco claro: {motivo:?}");
    std::mem::forget(montaje);
}
