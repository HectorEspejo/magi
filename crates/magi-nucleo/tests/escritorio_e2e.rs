//! Prueba de extremo a extremo real de la Fase 12 (checklist 163, T95), sin
//! red externa: servidor RFB de prueba en 127.0.0.1:0 -> rfb::abrir (cliente
//! RFB real) -> puente del visor (visor_http::RegistroVisor) -> cliente
//! WebSocket de prueba (tokio-tungstenite).
//!
//! Comprueba píxeles de ida (Rect con RGBA exacto y Copia de CopyRect) y que
//! la entrada del visor (kc, m, b, sp) llega de vuelta al servidor RFB como
//! KeyEvent y PointerEvent, byte a byte.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use magi_nucleo::escritorio::visor_http::{AvisoVisor, RegistroVisor, Tiempos};
use magi_nucleo::escritorio::{
    rfb, Conexion, ConfigEscritorio, CredencialesEscritorio, DatosCertificado, DialogosEscritorio,
    ErrorEscritorio, EstadoEscritorio, ImagenEscritorio, InfoEscritorio, OpcionesEscritorio,
    PeticionCredenciales, TipoHost,
};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message as MensajeWs;
use tokio_tungstenite::MaybeTlsStream;
use tokio_tungstenite::WebSocketStream;

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Plazo de las esperas de la prueba.
const PLAZO: Duration = Duration::from_secs(5);
/// Escritorio que anuncia el servidor RFB de prueba.
const ANCHO_RFB: u16 = 100;
/// Alto del escritorio del servidor RFB de prueba.
const ALTO_RFB: u16 = 60;
/// Identificador de la sesión de escritorio en el registro del visor.
const ID_SESION: u32 = 41;

// ---------------------------------------------------------------- diálogo

/// El servidor RFB de prueba habla la seguridad None: el diálogo no se usa.
struct DialogoVnc;

#[async_trait]
impl DialogosEscritorio for DialogoVnc {
    async fn certificado(&self, _datos: DatosCertificado) -> Result<bool, ErrorEscritorio> {
        Ok(true)
    }

    async fn credenciales(
        &self,
        _peticion: PeticionCredenciales,
    ) -> Result<Option<CredencialesEscritorio>, ErrorEscritorio> {
        Ok(None)
    }
}

fn configuracion(puerto: u16) -> ConfigEscritorio {
    ConfigEscritorio {
        tipo: TipoHost::Vnc,
        direccion: "127.0.0.1".to_string(),
        puerto,
        usuario: None,
        dominio: None,
        opciones: OpcionesEscritorio::por_defecto(TipoHost::Vnc),
        distribucion_teclado: "es-ES".to_string(),
        h264: false,
        tamano_inicial: (ANCHO_RFB, ALTO_RFB),
        portapapeles: true,
        huella_escritorio: None,
    }
}

fn info_sesion() -> InfoEscritorio {
    InfoEscritorio {
        id: ID_SESION,
        host_id: 1,
        nombre: "vnc-01".to_string(),
        tipo: TipoHost::Vnc,
        estado: EstadoEscritorio::Abierto,
        fase: None,
        motivo: None,
        visor: false,
        sin_visor_desde: None,
        ancho: ANCHO_RFB,
        alto: ALTO_RFB,
        imagen: Some(ImagenEscritorio::Rectangulos),
        fps: 0,
        directo: false,
        a_traves_de: None,
        desde: 0,
        redimensionable: true,
    }
}

fn tiempos_prueba() -> Tiempos {
    Tiempos {
        vida_enlace: Duration::from_secs(30),
        ventana_fallos: Duration::from_millis(500),
        bloqueo_fallos: Duration::from_millis(400),
        espera_hola: Duration::from_secs(2),
        intervalo_ping: Duration::from_secs(60),
        intervalo_marcos: Duration::from_millis(16),
        plazo_envio: Duration::from_secs(10),
    }
}

// ------------------------------------------------- servidor RFB de prueba

/// Mensajes del cliente RFB que le interesan a la prueba.
#[derive(Debug, Clone, PartialEq)]
enum MensajeRfb {
    Peticion {
        incremental: bool,
        x: u16,
        y: u16,
        ancho: u16,
        alto: u16,
    },
    Tecla {
        abajo: bool,
        keysym: u32,
    },
    Raton {
        botones: u8,
        x: u16,
        y: u16,
    },
    Otro(#[allow(dead_code)] u8),
}

impl MensajeRfb {
    fn como_peticion(&self) -> Option<(bool, u16, u16, u16, u16)> {
        match self {
            MensajeRfb::Peticion {
                incremental,
                x,
                y,
                ancho,
                alto,
            } => Some((*incremental, *x, *y, *ancho, *alto)),
            _ => None,
        }
    }
}

async fn leer_de(lector: &mut OwnedReadHalf, cuantos: usize) -> Vec<u8> {
    let mut datos = vec![0u8; cuantos];
    match tokio::time::timeout(PLAZO, lector.read_exact(&mut datos)).await {
        Ok(Ok(_)) => datos,
        otro => panic!("el cliente RFB no mandó {cuantos} bytes: {otro:?}"),
    }
}

async fn escribir_en(escritor: &mut OwnedWriteHalf, datos: &[u8]) {
    tokio::time::timeout(PLAZO, escritor.write_all(datos))
        .await
        .expect("plazo escribiendo al cliente RFB")
        .expect("escribir al cliente RFB");
}

async fn leer_mensaje_de(lector: &mut OwnedReadHalf) -> MensajeRfb {
    let tipo = leer_de(lector, 1).await[0];
    match tipo {
        0 => {
            leer_de(lector, 19).await;
            MensajeRfb::Otro(0)
        }
        2 => {
            let cabecera = leer_de(lector, 3).await;
            let cuantas = u16::from_be_bytes([cabecera[1], cabecera[2]]) as usize;
            leer_de(lector, cuantas * 4).await;
            MensajeRfb::Otro(2)
        }
        3 => {
            let resto = leer_de(lector, 9).await;
            MensajeRfb::Peticion {
                incremental: resto[0] != 0,
                x: u16::from_be_bytes([resto[1], resto[2]]),
                y: u16::from_be_bytes([resto[3], resto[4]]),
                ancho: u16::from_be_bytes([resto[5], resto[6]]),
                alto: u16::from_be_bytes([resto[7], resto[8]]),
            }
        }
        4 => {
            let resto = leer_de(lector, 7).await;
            MensajeRfb::Tecla {
                abajo: resto[0] != 0,
                keysym: u32::from_be_bytes([resto[3], resto[4], resto[5], resto[6]]),
            }
        }
        5 => {
            let resto = leer_de(lector, 5).await;
            MensajeRfb::Raton {
                botones: resto[0],
                x: u16::from_be_bytes([resto[1], resto[2]]),
                y: u16::from_be_bytes([resto[3], resto[4]]),
            }
        }
        6 => {
            let resto = leer_de(lector, 7).await;
            let largo = i32::from_be_bytes([resto[3], resto[4], resto[5], resto[6]]);
            leer_de(lector, largo.unsigned_abs() as usize).await;
            MensajeRfb::Otro(6)
        }
        otro => panic!("mensaje del cliente RFB no esperado en la prueba: {otro}"),
    }
}

/// Servidor RFB mínimo: RFB 3.8, seguridad None, ClientInit/ServerInit y lectura
/// de los mensajes del cliente.
struct ServidorRfb {
    lector: OwnedReadHalf,
    escritor: OwnedWriteHalf,
}

impl ServidorRfb {
    fn nuevo(flujo: TcpStream) -> Self {
        let _ = flujo.set_nodelay(true);
        let (lector, escritor) = flujo.into_split();
        Self { lector, escritor }
    }

    async fn leer(&mut self, cuantos: usize) -> Vec<u8> {
        leer_de(&mut self.lector, cuantos).await
    }

    async fn enviar(&mut self, datos: &[u8]) {
        escribir_en(&mut self.escritor, datos).await;
    }

    /// Saludo, seguridad None y ClientInit/ServerInit.
    async fn saludar_e_inicializar(&mut self) {
        self.enviar(b"RFB 003.008\n").await;
        assert_eq!(self.leer(12).await, b"RFB 003.008\n", "saludo del cliente");
        // Lista de seguridades con None y elección del cliente.
        self.enviar(&[1, 1]).await;
        assert_eq!(self.leer(1).await, vec![1], "seguridad elegida");
        // Resultado de seguridad (3.8).
        self.enviar(&0u32.to_be_bytes()).await;
        // ClientInit con shared-flag=1.
        assert_eq!(self.leer(1).await, vec![1], "shared-flag del ClientInit");
        let mut init = Vec::new();
        init.extend_from_slice(&ANCHO_RFB.to_be_bytes());
        init.extend_from_slice(&ALTO_RFB.to_be_bytes());
        init.extend_from_slice(&[32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 0, 8, 16, 0, 0, 0]);
        let nombre = b"servidor vnc de prueba";
        init.extend_from_slice(&(nombre.len() as u32).to_be_bytes());
        init.extend_from_slice(nombre);
        self.enviar(&init).await;
        // SetPixelFormat y SetEncodings del cliente.
        match self.leer_mensaje().await {
            MensajeRfb::Otro(0) => {}
            otro => panic!("se esperaba SetPixelFormat y llegó {otro:?}"),
        }
        match self.leer_mensaje().await {
            MensajeRfb::Otro(2) => {}
            otro => panic!("se esperaba SetEncodings y llegó {otro:?}"),
        }
    }

    async fn leer_mensaje(&mut self) -> MensajeRfb {
        leer_mensaje_de(&mut self.lector).await
    }
}

/// Tarea que lee del servidor RFB y reenvía los mensajes a la prueba.
async fn leer_mensajes(mut lector: OwnedReadHalf, reportados: mpsc::Sender<MensajeRfb>) {
    loop {
        let mensaje = leer_mensaje_de(&mut lector).await;
        if reportados.send(mensaje).await.is_err() {
            break;
        }
    }
}

fn rectangulo(codificacion: i32, x: u16, y: u16, ancho: u16, alto: u16, datos: &[u8]) -> Vec<u8> {
    let mut mensaje = Vec::with_capacity(12 + datos.len());
    mensaje.extend_from_slice(&x.to_be_bytes());
    mensaje.extend_from_slice(&y.to_be_bytes());
    mensaje.extend_from_slice(&ancho.to_be_bytes());
    mensaje.extend_from_slice(&alto.to_be_bytes());
    mensaje.extend_from_slice(&codificacion.to_be_bytes());
    mensaje.extend_from_slice(datos);
    mensaje
}

fn actualizacion(rectangulos: &[Vec<u8>]) -> Vec<u8> {
    let mut mensaje = vec![0u8, 0];
    mensaje.extend_from_slice(&(rectangulos.len() as u16).to_be_bytes());
    for rectangulo in rectangulos {
        mensaje.extend_from_slice(rectangulo);
    }
    mensaje
}

// ---------------------------------------------------------------- HTTP

struct RespuestaHttp {
    estado: u16,
    cuerpo: String,
}

impl RespuestaHttp {
    fn json(&self) -> Value {
        serde_json::from_str(&self.cuerpo).unwrap_or(Value::Null)
    }
}

async fn enviar_http(addr: SocketAddr, peticion: &str) -> RespuestaHttp {
    let mut flujo = TcpStream::connect(addr)
        .await
        .expect("conectar con el servidor del visor");
    flujo
        .write_all(peticion.as_bytes())
        .await
        .expect("escribir la petición HTTP");
    let mut datos = Vec::new();
    let mut bufer = [0u8; 8192];
    loop {
        match tokio::time::timeout(Duration::from_secs(5), flujo.read(&mut bufer)).await {
            Ok(Ok(0)) => break,
            Ok(Ok(leidos)) => datos.extend_from_slice(&bufer[..leidos]),
            _ => break,
        }
    }
    let texto = String::from_utf8_lossy(&datos).to_string();
    let (cabeza, cuerpo) = texto.split_once("\r\n\r\n").unwrap_or((texto.as_str(), ""));
    let estado = cabeza
        .split("\r\n")
        .next()
        .and_then(|linea| linea.split_whitespace().nth(1))
        .and_then(|codigo| codigo.parse().ok())
        .unwrap_or(0);
    RespuestaHttp {
        estado,
        cuerpo: cuerpo.to_string(),
    }
}

/// Canje del enlace de un solo uso con Host y Origin correctos.
async fn canjear(addr: SocketAddr, enlace: &str) -> RespuestaHttp {
    let origen = format!("http://127.0.0.1:{}", addr.port());
    let cuerpo = json!({ "enlace": enlace }).to_string();
    let peticion = format!(
        "POST /api/canjear HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nOrigin: {origen}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{cuerpo}",
        addr.port(),
        cuerpo.len()
    );
    enviar_http(addr, &peticion).await
}

// ---------------------------------------------------------------- WebSocket

async fn conectar_ws(puerto: u16) -> Ws {
    let url = format!("ws://127.0.0.1:{puerto}/api/ws");
    let mut peticion = url
        .into_client_request()
        .expect("petición WebSocket válida");
    peticion.headers_mut().insert(
        "Origin",
        HeaderValue::from_str(&format!("http://127.0.0.1:{puerto}")).expect("origen válido"),
    );
    tokio_tungstenite::connect_async(peticion)
        .await
        .expect("conectar el WebSocket del visor")
        .0
}

async fn enviar_hola(ws: &mut Ws, clave: &str) {
    let texto = json!({"t": "hola", "clave": clave, "h264": true}).to_string();
    ws.send(MensajeWs::Text(texto.into()))
        .await
        .expect("mandar el hola");
}

async fn enviar_texto(ws: &mut Ws, texto: &str) {
    ws.send(MensajeWs::Text(texto.to_string().into()))
        .await
        .expect("mandar la orden del visor");
}

async fn siguiente(ws: &mut Ws, plazo: Duration) -> Option<MensajeWs> {
    match tokio::time::timeout(plazo, ws.next()).await {
        Ok(Some(Ok(mensaje))) => Some(mensaje),
        _ => None,
    }
}

async fn esperar_json(ws: &mut Ws, plazo: Duration) -> Value {
    loop {
        match siguiente(ws, plazo).await {
            Some(MensajeWs::Text(texto)) => {
                return serde_json::from_str(texto.as_str()).expect("texto JSON del visor")
            }
            Some(MensajeWs::Binary(_)) | Some(MensajeWs::Ping(_)) | Some(MensajeWs::Pong(_)) => {
                continue
            }
            otro => panic!("se esperaba un texto del visor y llegó {otro:?}"),
        }
    }
}

async fn esperar_hello(ws: &mut Ws) -> Value {
    loop {
        let mensaje = esperar_json(ws, PLAZO).await;
        if mensaje["t"] == "hello" {
            return mensaje;
        }
    }
}

async fn esperar_binario(ws: &mut Ws, tipo: u8, plazo: Duration) -> Vec<u8> {
    loop {
        match siguiente(ws, plazo).await {
            Some(MensajeWs::Binary(datos)) => {
                if datos.first() == Some(&tipo) {
                    return datos.to_vec();
                }
            }
            Some(MensajeWs::Text(_)) | Some(MensajeWs::Ping(_)) | Some(MensajeWs::Pong(_)) => {
                continue
            }
            otro => panic!("se esperaba un binario {tipo:#04x} del visor y llegó {otro:?}"),
        }
    }
}

fn u16_en(datos: &[u8], inicio: usize) -> u16 {
    u16::from_be_bytes([datos[inicio], datos[inicio + 1]])
}

// ------------------------------------------------------------ utilidades

fn rgba(crudo: [u8; 3]) -> [u8; 4] {
    [crudo[0], crudo[1], crudo[2], 255]
}

/// Pixel RGBA de un Rect del visor en coordenadas del escritorio, si cae
/// dentro de ese rectángulo.
fn pixel_en_rect(rect: &[u8], x: u16, y: u16) -> Option<[u8; 4]> {
    let rx = u16_en(rect, 1);
    let ry = u16_en(rect, 3);
    let rw = u16_en(rect, 5);
    let rh = u16_en(rect, 7);
    if x < rx || y < ry || x >= rx + rw || y >= ry + rh {
        return None;
    }
    let indice = ((y - ry) * rw + (x - rx)) as usize * 4;
    let inicio = 9 + indice;
    rect.get(inicio..inicio + 4)
        .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
}

/// Espera a que llegue un Rect del visor que contenga todos los píxeles
/// pedidos con su color (el puente puede mandar primero el repintado completo
/// del búfer o una zona sucia concreta).
async fn esperar_pixeles(ws: &mut Ws, esperados: &[(u16, u16, [u8; 4])]) {
    let limite = tokio::time::Instant::now() + PLAZO;
    loop {
        let restante = limite.saturating_duration_since(tokio::time::Instant::now());
        assert!(
            !restante.is_zero(),
            "no llegó ningún Rect con los píxeles esperados: {esperados:?}"
        );
        let rect = esperar_binario(ws, 0x05, restante).await;
        let completo = esperados
            .iter()
            .all(|(x, y, color)| pixel_en_rect(&rect, *x, *y) == Some(*color));
        if completo {
            return;
        }
    }
}

/// Espera un mensaje del cliente RFB que cumpla el predicado, descartando los
/// demás (peticiones de actualización, etc.).
async fn esperar_rfb(
    mensajes: &mut mpsc::Receiver<MensajeRfb>,
    mut cumple: impl FnMut(&MensajeRfb) -> bool,
    descripcion: &str,
) -> MensajeRfb {
    let limite = tokio::time::Instant::now() + PLAZO;
    loop {
        let restante = limite.saturating_duration_since(tokio::time::Instant::now());
        assert!(
            !restante.is_zero(),
            "no llegó al servidor RFB: {descripcion}"
        );
        match tokio::time::timeout(restante, mensajes.recv()).await {
            Ok(Some(mensaje)) => {
                if cumple(&mensaje) {
                    return mensaje;
                }
            }
            _ => panic!("la tarea del servidor RFB terminó antes de tiempo"),
        }
    }
}

fn drenar_avisos(mut avisos: mpsc::Receiver<AvisoVisor>) -> Arc<std::sync::Mutex<Vec<AvisoVisor>>> {
    let vistos = Arc::new(std::sync::Mutex::new(Vec::new()));
    let copia = vistos.clone();
    tokio::spawn(async move {
        while let Some(aviso) = avisos.recv().await {
            if let Ok(mut guardia) = copia.lock() {
                guardia.push(aviso);
            }
        }
    });
    vistos
}

async fn esperar_hasta(plazo: Duration, condicion: impl Fn() -> bool) {
    let limite = tokio::time::Instant::now() + plazo;
    while tokio::time::Instant::now() < limite {
        if condicion() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("la condición no se cumplió en {plazo:?}");
}

// ------------------------------------------------------------------ prueba

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn extremo_a_extremo_rfb_puente_visor() {
    // 1. Servidor RFB de prueba a la escucha en un puerto libre.
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("servidor RFB de prueba");
    let direccion = listener.local_addr().expect("dirección del servidor RFB");

    // 2. Canales entre el cliente RFB real y el puente del visor.
    let (eventos_tx, eventos_rx) = mpsc::channel(64);
    let (ordenes_tx, ordenes_rx) = mpsc::channel(64);

    // 3. Cliente RFB real (rfb::abrir) contra el servidor de prueba.
    let tarea_rfb = tokio::spawn(async move {
        let flujo = TcpStream::connect(direccion)
            .await
            .expect("conectar con el servidor RFB de prueba");
        let _ = flujo.set_nodelay(true);
        let conexion = Conexion {
            flujo: Box::new(flujo),
            config: configuracion(direccion.port()),
            dialogo: Arc::new(DialogoVnc),
            eventos: eventos_tx,
            ordenes: ordenes_rx,
            intento: 1,
        };
        rfb::abrir(conexion).await
    });
    let (flujo, _) = tokio::time::timeout(PLAZO, listener.accept())
        .await
        .expect("aceptar al cliente RFB")
        .expect("aceptar al cliente RFB");
    let mut servidor = ServidorRfb::nuevo(flujo);
    servidor.saludar_e_inicializar().await;

    // 4. Registro del visor y puente con los canales del cliente RFB real.
    let (registro, avisos_rx) = RegistroVisor::nuevo_con_tiempos(tiempos_prueba());
    let avisos = drenar_avisos(avisos_rx);
    let puerto = registro
        .arrancar(0)
        .await
        .expect("arrancar el servidor del visor");
    let _sesion = registro.alta(info_sesion(), true, eventos_rx, ordenes_tx);

    // 5. El lector del servidor RFB pasa a una tarea y el escritor se queda en
    // la prueba para mandar las actualizaciones programadas.
    let ServidorRfb {
        lector,
        mut escritor,
    } = servidor;
    let (reportados_tx, mut reportados) = mpsc::channel(64);
    let tarea_lectora = tokio::spawn(leer_mensajes(lector, reportados_tx));

    // Petición inicial: completa y del escritorio entero.
    match esperar_rfb(
        &mut reportados,
        |mensaje| mensaje.como_peticion().is_some(),
        "petición inicial",
    )
    .await
    {
        MensajeRfb::Peticion {
            incremental,
            x,
            y,
            ancho,
            alto,
        } => {
            assert!(!incremental, "la primera actualización debe ser completa");
            assert_eq!((x, y, ancho, alto), (0, 0, ANCHO_RFB, ALTO_RFB));
        }
        otro => panic!("se esperaba la petición inicial y llegó {otro:?}"),
    }

    // 6. Actualización Raw con colores conocidos en (2, 2) de 2×2.
    let colores = [[10u8, 20, 30], [40, 50, 60], [70, 80, 90], [1, 2, 3]];
    let mut crudos = Vec::new();
    for color in colores {
        crudos.extend_from_slice(&[color[0], color[1], color[2], 0]);
    }
    escribir_en(
        &mut escritor,
        &actualizacion(&[rectangulo(0, 2, 2, 2, 2, &crudos)]),
    )
    .await;

    // 7. Enlace de un solo uso, canje y WebSocket del visor.
    let enlace = registro.enlace(ID_SESION).expect("enlace de la sesión");
    let canje = canjear(SocketAddr::from(([127, 0, 0, 1], puerto)), &enlace).await;
    assert_eq!(canje.estado, 200, "el canje del enlace debe valer");
    let clave = canje.json()["clave"]
        .as_str()
        .expect("clave del visor")
        .to_string();
    let mut ws = conectar_ws(puerto).await;
    enviar_hola(&mut ws, &clave).await;
    let hello = esperar_hello(&mut ws).await;
    assert_eq!(hello["w"], ANCHO_RFB);
    assert_eq!(hello["h"], ALTO_RFB);
    assert_eq!(hello["tipo"], "vnc");
    assert_eq!(hello["imagen"], "rectangulos");
    assert_eq!(hello["portapapeles"], true);

    // 8. Píxeles de ida: el RGBA de la actualización Raw llega al visor.
    esperar_pixeles(
        &mut ws,
        &[(2, 2, rgba([10, 20, 30])), (3, 3, rgba([1, 2, 3]))],
    )
    .await;

    // 9. Copia: CopyRect desde (2, 2) a (10, 10) de 2×2 llega como 0x06 Copia.
    escribir_en(
        &mut escritor,
        &actualizacion(&[rectangulo(1, 10, 10, 2, 2, &[0, 2, 0, 2])]),
    )
    .await;
    let copia = esperar_binario(&mut ws, 0x06, PLAZO).await;
    assert_eq!(u16_en(&copia, 1), 2, "origen x de la copia");
    assert_eq!(u16_en(&copia, 3), 2, "origen y de la copia");
    assert_eq!(u16_en(&copia, 5), 10, "destino x de la copia");
    assert_eq!(u16_en(&copia, 7), 10, "destino y de la copia");
    assert_eq!(u16_en(&copia, 9), 2, "ancho de la copia");
    assert_eq!(u16_en(&copia, 11), 2, "alto de la copia");

    // 10. Entrada de vuelta: las órdenes del visor llegan al servidor RFB.
    enviar_texto(&mut ws, "kc,97,KeyA,1").await;
    match esperar_rfb(
        &mut reportados,
        |mensaje| matches!(mensaje, MensajeRfb::Tecla { .. }),
        "KeyEvent de KeyA",
    )
    .await
    {
        MensajeRfb::Tecla { abajo, keysym } => {
            assert!(abajo, "la tecla debe ir pulsada");
            assert_eq!(keysym, 0x61, "keysym de KeyA");
        }
        otro => panic!("se esperaba KeyEvent y llegó {otro:?}"),
    }

    enviar_texto(&mut ws, "m,10,20").await;
    match esperar_rfb(
        &mut reportados,
        |mensaje| matches!(mensaje, MensajeRfb::Raton { .. }),
        "PointerEvent del ratón",
    )
    .await
    {
        MensajeRfb::Raton { botones, x, y } => {
            assert_eq!(
                (botones, x, y),
                (0, 10, 20),
                "movimiento absoluto del ratón"
            );
        }
        otro => panic!("se esperaba PointerEvent y llegó {otro:?}"),
    }

    enviar_texto(&mut ws, "b,1,1").await;
    match esperar_rfb(
        &mut reportados,
        |mensaje| matches!(mensaje, MensajeRfb::Raton { botones, .. } if *botones == 1),
        "PointerEvent con el botón 1 pulsado",
    )
    .await
    {
        MensajeRfb::Raton { botones, x, y } => {
            assert_eq!((botones, x, y), (1, 10, 20));
        }
        otro => panic!("se esperaba PointerEvent y llegó {otro:?}"),
    }

    enviar_texto(&mut ws, "b,1,0").await;
    match esperar_rfb(
        &mut reportados,
        |mensaje| matches!(mensaje, MensajeRfb::Raton { botones, .. } if *botones == 0),
        "PointerEvent al soltar el botón 1",
    )
    .await
    {
        MensajeRfb::Raton { botones, .. } => assert_eq!(botones, 0),
        otro => panic!("se esperaba PointerEvent y llegó {otro:?}"),
    }

    enviar_texto(&mut ws, "sp,0,1").await;
    match esperar_rfb(
        &mut reportados,
        |mensaje| matches!(mensaje, MensajeRfb::Raton { botones, .. } if *botones == 0x10),
        "rueda abajo (botón 5)",
    )
    .await
    {
        MensajeRfb::Raton { botones, .. } => assert_eq!(botones, 0x10),
        otro => panic!("se esperaba PointerEvent y llegó {otro:?}"),
    }
    match esperar_rfb(
        &mut reportados,
        |mensaje| matches!(mensaje, MensajeRfb::Raton { botones, .. } if *botones == 0),
        "soltar la rueda",
    )
    .await
    {
        MensajeRfb::Raton { botones, .. } => assert_eq!(botones, 0),
        otro => panic!("se esperaba PointerEvent y llegó {otro:?}"),
    }

    // 11. Avisos: el servidor de escritorios recibe que la sesión está abierta
    // y que el visor se ha conectado.
    esperar_hasta(PLAZO, || {
        avisos
            .lock()
            .expect("avisos")
            .iter()
            .any(|aviso| matches!(aviso, AvisoVisor::Abierto { id } if *id == ID_SESION))
    })
    .await;
    esperar_hasta(PLAZO, || {
        avisos
            .lock()
            .expect("avisos")
            .iter()
            .any(|aviso| matches!(aviso, AvisoVisor::VisorConectado { id, .. } if *id == ID_SESION))
    })
    .await;

    // 12. Cierre ordenado: al cerrar el WebSocket el registro lo nota; al caer
    // el servidor RFB, el cliente RFB real termina limpio (Cerrado + Fin).
    ws.close(None).await.expect("cerrar el WebSocket del visor");
    esperar_hasta(PLAZO, || !registro.visor_conectado(ID_SESION)).await;
    tarea_lectora.abort();
    drop(escritor);
    match tokio::time::timeout(PLAZO, tarea_rfb).await {
        Ok(Ok(Ok(()))) => {}
        otro => panic!("el cliente RFB no terminó limpio al caer el servidor: {otro:?}"),
    }
    registro.baja(ID_SESION);
}
