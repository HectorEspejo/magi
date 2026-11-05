//! Pruebas del servidor HTTP/WebSocket del visor y del puente (Fase 12).
//!
//! Sin red externa: el servidor escucha en 127.0.0.1 en un puerto libre, el
//! cliente HTTP es un lector mínimo sobre TcpStream y el cliente WebSocket es
//! tokio-tungstenite. La prueba de extremo a extremo usa un canal de
//! EventoEscritorio como fuente de prueba y comprueba que los Rect, VideoRdp y
//! Marco llegan al visor y que las ordenes kc/m/b/… llegan a la fuente.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use magi_nucleo::escritorio::puente::COLA_MAX;
use magi_nucleo::escritorio::visor_http::{
    AvisoVisor, DatosVoxl, RegistroVisor, SesionVisor, Tiempos, MENSAJE_ENLACE_USADO,
};
use magi_nucleo::escritorio::{
    EstadoEscritorio, EventoEscritorio, ImagenEscritorio, InfoEscritorio, OrdenEscritorio, Region,
    TipoHost,
};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message as MensajeWs;
use tokio_tungstenite::MaybeTlsStream;
use tokio_tungstenite::WebSocketStream;

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

const CSP_EXACTA: &str = "default-src 'self'; script-src 'self'; worker-src 'self'; connect-src 'self'; img-src 'self' blob: data:; style-src 'self'; frame-ancestors 'none'; base-uri 'none'";

// ---------------------------------------------------------------- entorno

fn tiempos_prueba() -> Tiempos {
    Tiempos {
        vida_enlace: Duration::from_millis(200),
        ventana_fallos: Duration::from_millis(500),
        bloqueo_fallos: Duration::from_millis(400),
        espera_hola: Duration::from_millis(300),
        intervalo_ping: Duration::from_millis(100),
        intervalo_marcos: Duration::from_millis(16),
        plazo_envio: Duration::from_millis(300),
    }
}

struct Entorno {
    registro: Arc<RegistroVisor>,
    avisos: mpsc::Receiver<AvisoVisor>,
    puerto: u16,
}

impl Entorno {
    fn addr(&self) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], self.puerto))
    }
}

async fn entorno() -> Entorno {
    let (registro, avisos) = RegistroVisor::nuevo_con_tiempos(tiempos_prueba());
    let puerto = registro
        .arrancar(0)
        .await
        .expect("el servidor del visor tiene que arrancar");
    Entorno {
        registro,
        avisos,
        puerto,
    }
}

fn info(id: u32, tipo: TipoHost) -> InfoEscritorio {
    InfoEscritorio {
        id,
        host_id: 1,
        nombre: "win-01".to_string(),
        tipo,
        estado: EstadoEscritorio::Abierto,
        fase: None,
        motivo: None,
        visor: false,
        sin_visor_desde: None,
        ancho: 640,
        alto: 480,
        imagen: None,
        fps: 0,
        directo: false,
        a_traves_de: None,
        desde: 0,
        redimensionable: true,
    }
}

/// Alta de una sesión con sus canales; devuelve la sesión y los extremos de la
/// fuente de prueba.
fn alta_sesion(
    registro: &RegistroVisor,
    info: InfoEscritorio,
    portapapeles: bool,
) -> (
    Arc<SesionVisor>,
    mpsc::Sender<EventoEscritorio>,
    mpsc::Receiver<OrdenEscritorio>,
) {
    let (eventos_tx, eventos_rx) = mpsc::channel(64);
    let (ordenes_tx, ordenes_rx) = mpsc::channel(64);
    let sesion = registro.alta(info, portapapeles, eventos_rx, ordenes_tx);
    (sesion, eventos_tx, ordenes_rx)
}

/// Canales vacíos para las pruebas que no hablan con la fuente.
fn canales() -> (
    mpsc::Receiver<EventoEscritorio>,
    mpsc::Sender<OrdenEscritorio>,
) {
    let (_eventos_tx, eventos_rx) = mpsc::channel(4);
    let (ordenes_tx, _ordenes_rx) = mpsc::channel(4);
    (eventos_rx, ordenes_tx)
}

// ---------------------------------------------------------------- HTTP

struct RespuestaHttp {
    estado: u16,
    cabeceras: HashMap<String, String>,
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
        .expect("conectar con el servidor de prueba");
    flujo
        .write_all(peticion.as_bytes())
        .await
        .expect("escribir la petición");
    let mut datos = Vec::new();
    let mut bufer = [0u8; 8192];
    loop {
        match tokio::time::timeout(Duration::from_secs(5), flujo.read(&mut bufer)).await {
            Ok(Ok(0)) => break,
            Ok(Ok(leidos)) => datos.extend_from_slice(&bufer[..leidos]),
            Ok(Err(_)) => break,
            Err(_) => break,
        }
    }
    parsear_respuesta(&datos)
}

fn parsear_respuesta(datos: &[u8]) -> RespuestaHttp {
    let texto = String::from_utf8_lossy(datos).to_string();
    let (cabeza, cuerpo) = match texto.split_once("\r\n\r\n") {
        Some((cabeza, cuerpo)) => (cabeza, cuerpo),
        None => (texto.as_str(), ""),
    };
    let mut lineas = cabeza.split("\r\n");
    let estado = lineas
        .next()
        .and_then(|linea| linea.split_whitespace().nth(1))
        .and_then(|codigo| codigo.parse().ok())
        .unwrap_or(0);
    let mut cabeceras = HashMap::new();
    for linea in lineas {
        if let Some((nombre, valor)) = linea.split_once(':') {
            cabeceras.insert(nombre.trim().to_ascii_lowercase(), valor.trim().to_string());
        }
    }
    RespuestaHttp {
        estado,
        cabeceras,
        cuerpo: cuerpo.to_string(),
    }
}

async fn http_get_con(
    addr: SocketAddr,
    ruta: &str,
    host: Option<&str>,
    origen: Option<&str>,
) -> RespuestaHttp {
    let host = host
        .map(str::to_string)
        .unwrap_or_else(|| format!("127.0.0.1:{}", addr.port()));
    let mut peticion = format!("GET {ruta} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
    if let Some(origen) = origen {
        peticion.push_str(&format!("Origin: {origen}\r\n"));
    }
    peticion.push_str("\r\n");
    enviar_http(addr, &peticion).await
}

async fn http_get(addr: SocketAddr, ruta: &str) -> RespuestaHttp {
    let origen = format!("http://127.0.0.1:{}", addr.port());
    http_get_con(addr, ruta, None, Some(&origen)).await
}

async fn http_post_con(
    addr: SocketAddr,
    ruta: &str,
    host: Option<&str>,
    origen: Option<&str>,
    cuerpo: &str,
) -> RespuestaHttp {
    let host = host
        .map(str::to_string)
        .unwrap_or_else(|| format!("127.0.0.1:{}", addr.port()));
    let mut peticion = format!(
        "POST {ruta} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        cuerpo.len()
    );
    if let Some(origen) = origen {
        peticion.push_str(&format!("Origin: {origen}\r\n"));
    }
    peticion.push_str("\r\n");
    peticion.push_str(cuerpo);
    enviar_http(addr, &peticion).await
}

/// Canje con Host y Origin correctos.
async fn canjear(addr: SocketAddr, enlace: &str) -> RespuestaHttp {
    let origen = format!("http://127.0.0.1:{}", addr.port());
    let cuerpo = json!({ "enlace": enlace }).to_string();
    http_post_con(addr, "/api/canjear", None, Some(&origen), &cuerpo).await
}

fn comprobar_cabeceras(respuesta: &RespuestaHttp, ruta: &str) {
    assert_eq!(
        respuesta
            .cabeceras
            .get("content-security-policy")
            .map(String::as_str),
        Some(CSP_EXACTA),
        "CSP en {ruta}"
    );
    assert_eq!(
        respuesta
            .cabeceras
            .get("x-content-type-options")
            .map(String::as_str),
        Some("nosniff"),
        "nosniff en {ruta}"
    );
    assert_eq!(
        respuesta
            .cabeceras
            .get("referrer-policy")
            .map(String::as_str),
        Some("no-referrer"),
        "referrer-policy en {ruta}"
    );
    assert_eq!(
        respuesta.cabeceras.get("cache-control").map(String::as_str),
        Some("no-store"),
        "cache-control en {ruta}"
    );
}

// ---------------------------------------------------------------- HTTP: pruebas

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn las_cabeceras_de_seguridad_van_en_todas_las_respuestas() {
    let entorno = entorno().await;
    for ruta in ["/", "/entrar", "/visor.js", "/trabajador.js", "/visor.css"] {
        let respuesta = http_get(entorno.addr(), ruta).await;
        assert_eq!(respuesta.estado, 200, "estado de {ruta}");
        comprobar_cabeceras(&respuesta, ruta);
    }
    let salud = http_get(entorno.addr(), "/api/salud").await;
    assert_eq!(salud.estado, 200);
    assert_eq!(salud.cuerpo, "ok");
    comprobar_cabeceras(&salud, "/api/salud");

    let html = http_get(entorno.addr(), "/entrar").await;
    assert!(html.cuerpo.contains("MAGI"));
    assert!(html
        .cabeceras
        .get("content-type")
        .is_some_and(|tipo| tipo.starts_with("text/html")));
    let js = http_get(entorno.addr(), "/visor.js").await;
    assert!(!js.cuerpo.is_empty());
    assert!(js
        .cabeceras
        .get("content-type")
        .is_some_and(|tipo| tipo.starts_with("text/javascript")));
    let css = http_get(entorno.addr(), "/visor.css").await;
    assert!(css
        .cabeceras
        .get("content-type")
        .is_some_and(|tipo| tipo.starts_with("text/css")));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn el_host_se_comprueba_en_todas_las_rutas() {
    let entorno = entorno().await;
    let addr = entorno.addr();
    let malo = format!("evil.example:{}", entorno.puerto);
    for ruta in ["/", "/entrar", "/visor.js", "/api/salud"] {
        let respuesta = http_get_con(addr, ruta, Some(&malo), None).await;
        assert_eq!(respuesta.estado, 403, "host falso en {ruta}");
        comprobar_cabeceras(&respuesta, ruta);
    }
    // localhost con el puerto propio sí vale.
    let localhost = format!("localhost:{}", entorno.puerto);
    let respuesta = http_get_con(addr, "/entrar", Some(&localhost), None).await;
    assert_eq!(respuesta.estado, 200);
    // Sin puerto tampoco.
    let respuesta = http_get_con(addr, "/entrar", Some("127.0.0.1"), None).await;
    assert_eq!(respuesta.estado, 403);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn el_origin_es_obligatorio_en_la_api() {
    let entorno = entorno().await;
    let addr = entorno.addr();
    let salud = "/api/salud";
    assert_eq!(
        http_get_con(addr, salud, None, None).await.estado,
        403,
        "sin Origin"
    );
    assert_eq!(
        http_get_con(addr, salud, None, Some("http://evil.example"))
            .await
            .estado,
        403,
        "Origin ajeno"
    );
    let correcto = format!("http://localhost:{}", entorno.puerto);
    assert_eq!(
        http_get_con(addr, salud, None, Some(&correcto))
            .await
            .estado,
        200,
        "Origin propio"
    );
    // Fuera de /api/ el Origin no hace falta.
    assert_eq!(http_get_con(addr, "/entrar", None, None).await.estado, 200);

    // El POST del canje también exige Origin.
    let cuerpo = json!({"enlace": "inventado"}).to_string();
    let sin_origen = http_post_con(addr, "/api/canjear", None, None, &cuerpo).await;
    assert_eq!(sin_origen.estado, 403);
    let ajeno = http_post_con(
        addr,
        "/api/canjear",
        None,
        Some("http://evil.example"),
        &cuerpo,
    )
    .await;
    assert_eq!(ajeno.estado, 403);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn el_puerto_configurado_se_respeta_y_no_se_comparte() {
    let primero = entorno().await;
    let (registro_dos, _avisos_dos) = RegistroVisor::nuevo_con_tiempos(tiempos_prueba());
    // Mismo puerto: el segundo registro no lo puede abrir (no se roba).
    let resultado = registro_dos.arrancar(primero.puerto).await;
    assert!(resultado.is_err(), "el puerto ya está ocupado");
    // Arrancar otra vez el primero es idempotente.
    assert_eq!(
        primero.registro.arrancar(0).await.expect("idempotente"),
        primero.puerto
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn los_enlaces_son_de_un_solo_uso_caducan_y_se_anulan() {
    let entorno = entorno().await;
    let addr = entorno.addr();
    let _sesion_siete = {
        let (eventos, ordenes) = canales();
        entorno
            .registro
            .alta(info(7, TipoHost::Rdp), true, eventos, ordenes)
    };
    // Una segunda sesión mantiene el servidor en marcha al dar de baja la otra.
    let _sesion_ocho = {
        let (eventos, ordenes) = canales();
        entorno
            .registro
            .alta(info(8, TipoHost::Vnc), true, eventos, ordenes)
    };

    let enlace = entorno.registro.enlace(7).expect("enlace emitido");
    assert!(enlace.starts_with("http://127.0.0.1:"));
    assert!(enlace.contains("/entrar?t="));
    let token = enlace.split("t=").nth(1).expect("token en la URL");
    assert_eq!(token.len(), 43, "256 bits en base64 URL-safe");

    // Se canjea el token suelto, como manda la página del visor.
    let token = enlace.split("t=").nth(1).expect("token").to_string();
    let primero = canjear(addr, &token).await;
    assert_eq!(primero.estado, 200);
    let clave = primero.json()["clave"].as_str().expect("clave").to_string();
    assert!(!clave.is_empty());
    assert_eq!(primero.json()["escritorio"], "win-01");
    assert_eq!(primero.json()["tipo"], "rdp");

    // El segundo uso da 403 con el mensaje exacto.
    let segundo = canjear(addr, &enlace).await;
    assert_eq!(segundo.estado, 403);
    assert!(
        segundo.cuerpo.contains(MENSAJE_ENLACE_USADO),
        "{}",
        segundo.cuerpo
    );

    // Caducidad.
    let caduco = entorno.registro.enlace(7).expect("enlace");
    tokio::time::sleep(Duration::from_millis(260)).await;
    let respuesta = canjear(addr, &caduco).await;
    assert_eq!(respuesta.estado, 403);
    assert!(respuesta.cuerpo.contains(MENSAJE_ENLACE_USADO));

    // Anulación al cerrar la sesión.
    let anulado = entorno.registro.enlace(7).expect("enlace");
    entorno.registro.baja(7);
    let respuesta = canjear(addr, &anulado).await;
    assert_eq!(respuesta.estado, 403);
    assert!(entorno.registro.enlace(7).is_none());
    assert!(entorno.registro.enlace(99).is_none());
    entorno.registro.baja(8);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tras_diez_fallos_el_canje_responde_429() {
    let entorno = entorno().await;
    let addr = entorno.addr();
    let _sesion = {
        let (eventos, ordenes) = canales();
        entorno
            .registro
            .alta(info(11, TipoHost::Rdp), true, eventos, ordenes)
    };
    for intento in 0..10 {
        let respuesta = canjear(addr, "enlace-inventado").await;
        assert_eq!(respuesta.estado, 403, "fallo {intento}");
    }
    let bloqueado = canjear(addr, "enlace-inventado").await;
    assert_eq!(bloqueado.estado, 429);
    assert!(bloqueado.cuerpo.contains("demasiados intentos"));

    // El bloqueo caduca y un enlace bueno vuelve a valer.
    tokio::time::sleep(Duration::from_millis(460)).await;
    let enlace = entorno.registro.enlace(11).expect("enlace");
    let respuesta = canjear(addr, &enlace).await;
    assert_eq!(respuesta.estado, 200);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn el_canje_voxl_devuelve_la_url_del_tunel() {
    let entorno = entorno().await;
    let addr = entorno.addr();
    let sesion = entorno.registro.alta_voxl(
        info(9, TipoHost::Voxl),
        DatosVoxl {
            puerto_tunel: 41234,
            clave: zeroize::Zeroizing::new("secreto".to_string()),
        },
    );
    assert!(sesion.es_voxl());
    let enlace = entorno.registro.enlace(9).expect("enlace");
    let respuesta = canjear(addr, &enlace).await;
    assert_eq!(respuesta.estado, 200);
    assert_eq!(
        respuesta.json()["voxl"],
        "http://127.0.0.1:41234/?token=secreto"
    );
    // Un enlace voxl también es de un solo uso.
    assert_eq!(canjear(addr, &enlace).await.estado, 403);
    entorno.registro.baja(9);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn la_sesion_publica_visor_fps_y_tamano_para_la_lista() {
    let entorno = entorno().await;
    let (_sesion, eventos_tx, _ordenes) =
        alta_sesion(&entorno.registro, info(21, TipoHost::Rdp), true);
    assert!(!entorno.registro.visor_conectado(21));
    assert_eq!(entorno.registro.lista().len(), 1);
    eventos_tx
        .send(EventoEscritorio::Tamano {
            ancho: 1024,
            alto: 768,
        })
        .await
        .expect("mandar el tamaño");
    esperar_hasta(Duration::from_secs(2), || {
        entorno.registro.lista()[0].ancho == 1024
    })
    .await;
    let lista = entorno.registro.lista();
    assert_eq!(lista[0].ancho, 1024);
    assert_eq!(lista[0].alto, 768);
    assert_eq!(lista[0].imagen, Some(ImagenEscritorio::Rectangulos));
    entorno.registro.baja(21);
    assert!(entorno.registro.puerto().is_none(), "sin sesiones se para");
}

/// Espera a que se cumpla una condición simple (con plazo), sin red externa.
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

// ---------------------------------------------------------------- WebSocket

async fn conectar_ws(puerto: u16, origen: Option<&str>) -> Result<(Ws, u16), String> {
    let url = format!("ws://127.0.0.1:{puerto}/api/ws");
    let mut peticion = url
        .into_client_request()
        .map_err(|error| error.to_string())?;
    if let Some(origen) = origen {
        peticion.headers_mut().insert(
            "Origin",
            HeaderValue::from_str(origen).map_err(|error| error.to_string())?,
        );
    }
    match tokio_tungstenite::connect_async(peticion).await {
        Ok((ws, respuesta)) => Ok((ws, respuesta.status().as_u16())),
        Err(error) => Err(error.to_string()),
    }
}

async fn conectar_ws_correcto(puerto: u16) -> Ws {
    let origen = format!("http://127.0.0.1:{puerto}");
    conectar_ws(puerto, Some(&origen))
        .await
        .expect("conectar el WebSocket del visor")
        .0
}

async fn enviar_hola(ws: &mut Ws, clave: &str, h264: bool) {
    let texto = json!({"t": "hola", "clave": clave, "h264": h264}).to_string();
    ws.send(MensajeWs::Text(texto.into()))
        .await
        .expect("mandar el hola");
}

async fn enviar_texto(ws: &mut Ws, texto: &str) {
    ws.send(MensajeWs::Text(texto.to_string().into()))
        .await
        .expect("mandar la orden");
}

async fn siguiente(ws: &mut Ws, plazo: Duration) -> Option<MensajeWs> {
    match tokio::time::timeout(plazo, ws.next()).await {
        Ok(Some(Ok(mensaje))) => Some(mensaje),
        _ => None,
    }
}

/// Espera un mensaje de texto JSON, saltándose los binarios y los ping.
async fn esperar_json(ws: &mut Ws, plazo: Duration) -> Value {
    loop {
        match siguiente(ws, plazo).await {
            Some(MensajeWs::Text(texto)) => {
                return serde_json::from_str(texto.as_str()).expect("texto JSON válido")
            }
            Some(MensajeWs::Binary(_)) | Some(MensajeWs::Ping(_)) | Some(MensajeWs::Pong(_)) => {
                continue
            }
            otro => panic!("se esperaba un texto del servidor y llegó {otro:?}"),
        }
    }
}

async fn esperar_hello(ws: &mut Ws, plazo: Duration) -> Value {
    loop {
        let mensaje = esperar_json(ws, plazo).await;
        if mensaje["t"] == "hello" {
            return mensaje;
        }
    }
}

/// Espera un binario que empiece por el tipo pedido.
async fn esperar_binario(ws: &mut Ws, tipo: u8, plazo: Duration) -> Vec<u8> {
    loop {
        match siguiente(ws, plazo).await {
            Some(MensajeWs::Binary(datos)) => {
                if !datos.is_empty() && datos[0] == tipo {
                    return datos.to_vec();
                }
            }
            Some(MensajeWs::Text(_)) | Some(MensajeWs::Ping(_)) | Some(MensajeWs::Pong(_)) => {
                continue
            }
            otro => panic!("se esperaba un binario del servidor y llegó {otro:?}"),
        }
    }
}

fn u16_en(datos: &[u8], inicio: usize) -> u16 {
    u16::from_be_bytes([datos[inicio], datos[inicio + 1]])
}

fn u32_en(datos: &[u8], inicio: usize) -> u32 {
    u32::from_be_bytes([
        datos[inicio],
        datos[inicio + 1],
        datos[inicio + 2],
        datos[inicio + 3],
    ])
}

// ---------------------------------------------------------------- avisos

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

async fn esperar_orden(ordenes: &mut mpsc::Receiver<OrdenEscritorio>) -> OrdenEscritorio {
    tokio::time::timeout(Duration::from_secs(2), ordenes.recv())
        .await
        .expect("la orden llega a la fuente a tiempo")
        .expect("la fuente sigue viva")
}

async fn esperar_json_que(
    ws: &mut Ws,
    plazo: Duration,
    condicion: impl Fn(&Value) -> bool,
) -> Value {
    let limite = tokio::time::Instant::now() + plazo;
    loop {
        let restante = limite.saturating_duration_since(tokio::time::Instant::now());
        assert!(!restante.is_zero(), "no llegó el mensaje esperado");
        let mensaje = esperar_json(ws, restante).await;
        if condicion(&mensaje) {
            return mensaje;
        }
    }
}

/// Drena el repintado que el puente manda al conectar el visor.
async fn esperar_repintado_inicial(ws: &mut Ws, ancho: u16, alto: u16) {
    let rect = esperar_binario(ws, 0x05, Duration::from_secs(2)).await;
    assert_eq!(u16_en(&rect, 5), ancho);
    assert_eq!(u16_en(&rect, 7), alto);
    let _ = esperar_binario(ws, 0x08, Duration::from_secs(2)).await;
}

async fn esperar_estado(ws: &mut Ws, estado: &str) -> Value {
    esperar_json_que(ws, Duration::from_secs(2), |mensaje| {
        mensaje["t"] == "estado" && mensaje["estado"] == estado
    })
    .await
}

/// Espera el cierre del socket (Close, error o fin de flujo).
async fn esperar_cierre(ws: &mut Ws, plazo: Duration) -> bool {
    let cerrado = tokio::time::timeout(plazo, async {
        while let Some(mensaje) = ws.next().await {
            match mensaje {
                Ok(MensajeWs::Close(_)) | Err(_) => return true,
                Ok(_) => continue,
            }
        }
        true
    })
    .await;
    matches!(cerrado, Ok(true))
}

/// Comprueba que no llega ningún binario del tipo pedido en el plazo (para
/// v,0).
async fn no_llega_binario(ws: &mut Ws, tipo: u8, plazo: Duration) -> bool {
    let limite = tokio::time::Instant::now() + plazo;
    loop {
        let restante = limite.saturating_duration_since(tokio::time::Instant::now());
        if restante.is_zero() {
            return false;
        }
        match tokio::time::timeout(restante, ws.next()).await {
            Ok(Some(Ok(MensajeWs::Binary(datos)))) => {
                if datos.first() == Some(&tipo) {
                    return true;
                }
            }
            Ok(Some(Ok(_))) => continue,
            _ => return false,
        }
    }
}

// ---------------------------------------------------------------- WebSocket: pruebas

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn el_servidor_cierra_si_no_llega_el_hola() {
    let entorno = entorno().await;
    let mut ws = conectar_ws_correcto(entorno.puerto).await;
    // El plazo de prueba es de 300 ms; sin hola, se cierra.
    assert!(esperar_cierre(&mut ws, Duration::from_secs(2)).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn el_hola_tiene_que_ser_valido() {
    let entorno = entorno().await;
    let mut ws = conectar_ws_correcto(entorno.puerto).await;
    enviar_texto(&mut ws, "kf").await;
    assert!(esperar_cierre(&mut ws, Duration::from_secs(2)).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn el_websocket_exige_origin() {
    let entorno = entorno().await;
    assert!(
        conectar_ws(entorno.puerto, None).await.is_err(),
        "sin Origin no se puede abrir el WebSocket"
    );
    assert!(
        conectar_ws(entorno.puerto, Some("http://evil.example"))
            .await
            .is_err(),
        "un Origin ajeno no puede abrir el WebSocket"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn la_clave_es_por_sesion_y_el_recange_la_invalida() {
    let entorno = entorno().await;
    let addr = entorno.addr();
    let (_sesion, _eventos, _ordenes) =
        alta_sesion(&entorno.registro, info(1, TipoHost::Rdp), true);
    let (_sesion_dos, _eventos_dos, _ordenes_dos) =
        alta_sesion(&entorno.registro, info(2, TipoHost::Vnc), true);

    let enlace_uno = entorno.registro.enlace(1).expect("enlace de la sesión 1");
    let enlace_dos = entorno.registro.enlace(2).expect("enlace de la sesión 2");
    let canje_uno = canjear(addr, &enlace_uno).await;
    let canje_dos = canjear(addr, &enlace_dos).await;
    assert_eq!(canje_uno.estado, 200);
    assert_eq!(canje_dos.estado, 200);
    let clave_uno = canje_uno.json()["clave"].as_str().unwrap().to_string();
    let clave_dos = canje_dos.json()["clave"].as_str().unwrap().to_string();
    assert_ne!(clave_uno, clave_dos, "una clave por sesión");

    let mut ws_uno = conectar_ws_correcto(entorno.puerto).await;
    enviar_hola(&mut ws_uno, &clave_uno, true).await;
    assert_eq!(
        esperar_hello(&mut ws_uno, Duration::from_secs(2)).await["tipo"],
        "rdp"
    );
    let mut ws_dos = conectar_ws_correcto(entorno.puerto).await;
    enviar_hola(&mut ws_dos, &clave_dos, true).await;
    assert_eq!(
        esperar_hello(&mut ws_dos, Duration::from_secs(2)).await["tipo"],
        "vnc"
    );

    // Un canje nuevo invalida la clave anterior y echa al visor que la usaba.
    let enlace_tres = entorno.registro.enlace(1).expect("enlace nuevo");
    let canje_tres = canjear(addr, &enlace_tres).await;
    assert_eq!(canje_tres.estado, 200);
    let clave_tres = canje_tres.json()["clave"].as_str().unwrap().to_string();
    assert_ne!(clave_tres, clave_uno);
    assert_eq!(
        esperar_estado(&mut ws_uno, "otra_ventana").await["estado"],
        "otra_ventana"
    );
    assert!(esperar_cierre(&mut ws_uno, Duration::from_secs(2)).await);

    // La clave vieja ya no conecta (el socket se cierra sin hello).
    let mut ws_viejo = conectar_ws_correcto(entorno.puerto).await;
    enviar_hola(&mut ws_viejo, &clave_uno, true).await;
    assert!(esperar_cierre(&mut ws_viejo, Duration::from_secs(2)).await);

    // La nueva sí.
    let mut ws_nuevo = conectar_ws_correcto(entorno.puerto).await;
    enviar_hola(&mut ws_nuevo, &clave_tres, true).await;
    let hello = esperar_hello(&mut ws_nuevo, Duration::from_secs(2)).await;
    assert_eq!(hello["tipo"], "rdp");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn un_visor_sustituye_al_anterior() {
    let entorno = entorno().await;
    let addr = entorno.addr();
    let (_sesion, eventos_tx, _ordenes) =
        alta_sesion(&entorno.registro, info(3, TipoHost::Rdp), true);
    let enlace = entorno.registro.enlace(3).expect("enlace");
    let clave = canjear(addr, &enlace).await.json()["clave"]
        .as_str()
        .unwrap()
        .to_string();

    let mut primero = conectar_ws_correcto(entorno.puerto).await;
    enviar_hola(&mut primero, &clave, true).await;
    let _ = esperar_hello(&mut primero, Duration::from_secs(2)).await;
    assert!(entorno.registro.visor_conectado(3));

    let mut segundo = conectar_ws_correcto(entorno.puerto).await;
    enviar_hola(&mut segundo, &clave, true).await;
    let _ = esperar_hello(&mut segundo, Duration::from_secs(2)).await;
    esperar_repintado_inicial(&mut segundo, 640, 480).await;
    assert_eq!(
        esperar_estado(&mut primero, "otra_ventana").await["estado"],
        "otra_ventana"
    );
    assert!(esperar_cierre(&mut primero, Duration::from_secs(2)).await);

    // El nuevo sigue recibiendo imagen.
    eventos_tx
        .send(EventoEscritorio::Rect {
            x: 1,
            y: 2,
            ancho: 1,
            alto: 1,
            pixeles: vec![10, 20, 30, 255],
        })
        .await
        .expect("mandar un rectángulo");
    let rect = esperar_binario(&mut segundo, 0x05, Duration::from_secs(2)).await;
    assert_eq!(u16_en(&rect, 1), 1);
    assert_eq!(u16_en(&rect, 3), 2);
    assert_eq!(&rect[9..], &[10, 20, 30, 255]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn extremo_a_extremo_eventos_y_ordenes() {
    let entorno = entorno().await;
    let addr = entorno.addr();
    let avisos = drenar_avisos(entorno.avisos);
    let (_sesion, eventos_tx, mut ordenes_rx) =
        alta_sesion(&entorno.registro, info(7, TipoHost::Rdp), true);

    let enlace = entorno.registro.enlace(7).expect("enlace");
    let canje = canjear(addr, &enlace).await;
    assert_eq!(canje.estado, 200);
    let clave = canje.json()["clave"].as_str().unwrap().to_string();
    let mut ws = conectar_ws_correcto(entorno.puerto).await;
    enviar_hola(&mut ws, &clave, true).await;

    let hello = esperar_hello(&mut ws, Duration::from_secs(2)).await;
    assert_eq!(hello["w"], 640);
    assert_eq!(hello["h"], 480);
    assert_eq!(hello["tipo"], "rdp");
    assert_eq!(hello["imagen"], "rectangulos");
    assert_eq!(hello["redimensionable"], true);
    assert_eq!(hello["portapapeles"], true);
    esperar_repintado_inicial(&mut ws, 640, 480).await;

    // Abierto: el servidor recibe el aviso para pasar la sesión a abierta.
    eventos_tx
        .send(EventoEscritorio::Abierto)
        .await
        .expect("abierto");
    esperar_hasta(Duration::from_secs(2), || {
        avisos
            .lock()
            .unwrap()
            .iter()
            .any(|aviso| matches!(aviso, AvisoVisor::Abierto { id: 7 }))
    })
    .await;

    // Rect + Marco.
    let pixeles = vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 9, 9, 9, 255];
    eventos_tx
        .send(EventoEscritorio::Rect {
            x: 0,
            y: 0,
            ancho: 2,
            alto: 2,
            pixeles: pixeles.clone(),
        })
        .await
        .expect("rect");
    let rect = esperar_binario(&mut ws, 0x05, Duration::from_secs(2)).await;
    assert_eq!(u16_en(&rect, 1), 0);
    assert_eq!(u16_en(&rect, 3), 0);
    assert_eq!(u16_en(&rect, 5), 2);
    assert_eq!(u16_en(&rect, 7), 2);
    assert_eq!(&rect[9..], pixeles.as_slice());
    let marco = esperar_binario(&mut ws, 0x08, Duration::from_secs(2)).await;
    assert_eq!(marco, vec![0x08]);

    // VideoRdp tal cual: marco, regiones y Annex-B.
    let h264 = vec![0u8, 0, 0, 1, 0x65, 0x88, 0x84];
    eventos_tx
        .send(EventoEscritorio::Video {
            marco: 7,
            regiones: vec![Region::nueva(0, 0, 2, 2)],
            h264: h264.clone(),
        })
        .await
        .expect("vídeo");
    let video = esperar_binario(&mut ws, 0x07, Duration::from_secs(2)).await;
    assert_eq!(u32_en(&video, 1), 7);
    assert_eq!(u16_en(&video, 5), 1);
    assert_eq!(u16_en(&video, 7), 0);
    assert_eq!(u16_en(&video, 9), 0);
    assert_eq!(u16_en(&video, 11), 2);
    assert_eq!(u16_en(&video, 13), 2);
    assert_eq!(&video[15..], h264.as_slice());

    // Cursor remoto.
    let cursor = vec![7u8; 16];
    eventos_tx
        .send(EventoEscritorio::Cursor {
            visible: true,
            x: 1,
            y: 1,
            punto: (1, 0),
            ancho: 2,
            alto: 2,
            pixeles: cursor.clone(),
        })
        .await
        .expect("cursor");
    let mensaje = esperar_binario(&mut ws, 0x03, Duration::from_secs(2)).await;
    assert_eq!(mensaje.len(), 9 + 16);
    assert_eq!(u16_en(&mensaje, 1), 2);
    assert_eq!(u16_en(&mensaje, 3), 2);
    assert_eq!(u16_en(&mensaje, 5), 1);
    assert_eq!(u16_en(&mensaje, 7), 0);
    assert_eq!(&mensaje[9..], cursor.as_slice());

    // CopyRect sin pintura pendiente.
    eventos_tx
        .send(EventoEscritorio::Copia {
            origen: (0, 0),
            destino: (2, 2),
            ancho: 2,
            alto: 2,
        })
        .await
        .expect("copia");
    let copia = esperar_binario(&mut ws, 0x06, Duration::from_secs(2)).await;
    assert_eq!(u16_en(&copia, 1), 0);
    assert_eq!(u16_en(&copia, 3), 0);
    assert_eq!(u16_en(&copia, 5), 2);
    assert_eq!(u16_en(&copia, 7), 2);
    assert_eq!(u16_en(&copia, 9), 2);
    assert_eq!(u16_en(&copia, 11), 2);
    // La copia aislada se presenta con un Marco inmediato (m1).
    let _ = esperar_binario(&mut ws, 0x08, Duration::from_secs(2)).await;

    // Órdenes del visor: llegan a la fuente.
    enviar_texto(&mut ws, "kc,97,KeyA,1").await;
    assert_eq!(
        esperar_orden(&mut ordenes_rx).await,
        OrdenEscritorio::Tecla {
            codigo: "KeyA".to_string(),
            keysym: 97,
            abajo: true
        }
    );
    enviar_texto(&mut ws, "m,10,20").await;
    assert_eq!(
        esperar_orden(&mut ordenes_rx).await,
        OrdenEscritorio::Raton { x: 10, y: 20 }
    );
    enviar_texto(&mut ws, "b,1,0").await;
    assert_eq!(
        esperar_orden(&mut ordenes_rx).await,
        OrdenEscritorio::Boton {
            boton: 1,
            abajo: false
        }
    );
    enviar_texto(&mut ws, "b,8,1").await;
    assert_eq!(
        esperar_orden(&mut ordenes_rx).await,
        OrdenEscritorio::Boton {
            boton: 8,
            abajo: true
        }
    );
    enviar_texto(&mut ws, "sp,1.5,-2.5").await;
    assert_eq!(
        esperar_orden(&mut ordenes_rx).await,
        OrdenEscritorio::Rueda { dx: 1.5, dy: -2.5 }
    );
    enviar_texto(&mut ws, "r").await;
    assert_eq!(
        esperar_orden(&mut ordenes_rx).await,
        OrdenEscritorio::SoltarTodo
    );
    enviar_texto(&mut ws, "c,hola, mundo").await;
    assert_eq!(
        esperar_orden(&mut ordenes_rx).await,
        OrdenEscritorio::Portapapeles("hola, mundo".to_string())
    );
    enviar_texto(&mut ws, "sz,800,600").await;
    assert_eq!(
        esperar_orden(&mut ordenes_rx).await,
        OrdenEscritorio::Tamano {
            ancho: 800,
            alto: 600
        }
    );
    enviar_texto(&mut ws, "cad").await;
    assert_eq!(
        esperar_orden(&mut ordenes_rx).await,
        OrdenEscritorio::CtrlAltSupr
    );
    enviar_texto(&mut ws, "win").await;
    assert_eq!(
        esperar_orden(&mut ordenes_rx).await,
        OrdenEscritorio::TeclaWindows
    );
    // Una orden inválida no llega y el visor sigue vivo.
    enviar_texto(&mut ws, "kc,0,,1").await;
    enviar_texto(&mut ws, "m,99999,0").await;
    enviar_texto(&mut ws, "orden-desconocida").await;

    // El modo vuelve a rectángulos (como un RDP sin AVC420) y kf repinta el
    // escritorio entero desde el búfer.
    eventos_tx
        .send(EventoEscritorio::Imagen(ImagenEscritorio::Rectangulos))
        .await
        .expect("imagen de nuevo en rectángulos");
    let _ = esperar_json_que(&mut ws, Duration::from_secs(2), |mensaje| {
        mensaje["t"] == "hello" && mensaje["imagen"] == "rectangulos"
    })
    .await;
    enviar_texto(&mut ws, "kf").await;
    let repintado = esperar_binario(&mut ws, 0x05, Duration::from_secs(2)).await;
    assert_eq!(u16_en(&repintado, 5), 640);
    assert_eq!(u16_en(&repintado, 7), 480);
    let _ = esperar_binario(&mut ws, 0x08, Duration::from_secs(2)).await;

    // El ping del puente se contesta con po y produce latencia.
    let ping = esperar_json_que(&mut ws, Duration::from_secs(3), |mensaje| {
        mensaje["t"] == "ping"
    })
    .await;
    let s = ping["s"].as_u64().expect("epoch ms en el ping");
    enviar_texto(&mut ws, &format!("po,{s}")).await;
    let latencia = esperar_json_que(&mut ws, Duration::from_secs(3), |mensaje| {
        mensaje["t"] == "latencia"
    })
    .await;
    assert!(latencia["ms"].as_u64().is_some());

    // Avisos para el servidor de escritorios.
    esperar_hasta(Duration::from_secs(2), || {
        let vistos = avisos.lock().unwrap();
        vistos
            .iter()
            .any(|aviso| matches!(aviso, AvisoVisor::VisorConectado { id: 7, h264: true }))
    })
    .await;
    assert!(entorno.registro.visor_conectado(7));

    // El portapapeles del remoto llega como clip.
    eventos_tx
        .send(EventoEscritorio::Portapapeles("texto remoto".to_string()))
        .await
        .expect("portapapeles");
    let clip = esperar_json_que(&mut ws, Duration::from_secs(2), |mensaje| {
        mensaje["t"] == "clip"
    })
    .await;
    assert_eq!(clip["texto"], "texto remoto");

    // Cerrar el visor se nota en el registro.
    ws.close(None).await.expect("cerrar el WebSocket");
    esperar_hasta(Duration::from_secs(2), || {
        !entorno.registro.visor_conectado(7)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn con_la_pestana_oculta_deja_de_mandar_y_al_volver_repinta() {
    let entorno = entorno().await;
    let addr = entorno.addr();
    let _avisos = drenar_avisos(entorno.avisos);
    let (_sesion, eventos_tx, mut ordenes_rx) =
        alta_sesion(&entorno.registro, info(17, TipoHost::Vnc), true);
    let enlace = entorno.registro.enlace(17).expect("enlace");
    let clave = canjear(addr, &enlace).await.json()["clave"]
        .as_str()
        .unwrap()
        .to_string();
    let mut ws = conectar_ws_correcto(entorno.puerto).await;
    enviar_hola(&mut ws, &clave, true).await;
    let _ = esperar_hello(&mut ws, Duration::from_secs(2)).await;
    esperar_repintado_inicial(&mut ws, 640, 480).await;

    enviar_texto(&mut ws, "v,0").await;
    assert_eq!(
        esperar_orden(&mut ordenes_rx).await,
        OrdenEscritorio::Visible(false)
    );
    eventos_tx
        .send(EventoEscritorio::Rect {
            x: 0,
            y: 0,
            ancho: 1,
            alto: 1,
            pixeles: vec![1, 2, 3, 255],
        })
        .await
        .expect("rect con la pestaña oculta");
    // Con v,0 tampoco se mandan cursor ni copias (M7); el vídeo tiene su
    // propia prueba porque cambia la imagen a H.264.
    eventos_tx
        .send(EventoEscritorio::Cursor {
            visible: true,
            x: 0,
            y: 0,
            punto: (0, 0),
            ancho: 2,
            alto: 2,
            pixeles: vec![1; 16],
        })
        .await
        .expect("cursor con la pestaña oculta");
    eventos_tx
        .send(EventoEscritorio::Copia {
            origen: (0, 0),
            destino: (1, 1),
            ancho: 1,
            alto: 1,
        })
        .await
        .expect("copia con la pestaña oculta");
    assert!(
        !no_llega_binario(&mut ws, 0x05, Duration::from_millis(300)).await,
        "con v,0 no se manda imagen"
    );
    // Ninguno de los demás binarios de imagen puede adelantarse.
    assert!(
        !no_llega_binario(&mut ws, 0x03, Duration::from_millis(50)).await,
        "con v,0 no se manda cursor"
    );
    assert!(
        !no_llega_binario(&mut ws, 0x06, Duration::from_millis(50)).await,
        "con v,0 no se manda copia"
    );

    enviar_texto(&mut ws, "v,1").await;
    assert_eq!(
        esperar_orden(&mut ordenes_rx).await,
        OrdenEscritorio::Visible(true)
    );
    let repintado = esperar_binario(&mut ws, 0x05, Duration::from_secs(2)).await;
    assert_eq!(u16_en(&repintado, 5), 640);
    assert_eq!(u16_en(&repintado, 7), 480);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sin_h264_avisa_y_no_manda_video() {
    let entorno = entorno().await;
    let addr = entorno.addr();
    let avisos = drenar_avisos(entorno.avisos);
    let mut base = info(31, TipoHost::Rdp);
    base.imagen = Some(ImagenEscritorio::H264);
    let (_sesion, eventos_tx, _ordenes) = alta_sesion(&entorno.registro, base, true);
    let enlace = entorno.registro.enlace(31).expect("enlace");
    let clave = canjear(addr, &enlace).await.json()["clave"]
        .as_str()
        .unwrap()
        .to_string();

    let mut ws = conectar_ws_correcto(entorno.puerto).await;
    enviar_hola(&mut ws, &clave, false).await;
    let hello = esperar_hello(&mut ws, Duration::from_secs(2)).await;
    assert_eq!(hello["imagen"], "h264");
    esperar_hasta(Duration::from_secs(2), || {
        avisos
            .lock()
            .unwrap()
            .iter()
            .any(|aviso| matches!(aviso, AvisoVisor::SinH264 { id: 31 }))
    })
    .await;

    eventos_tx
        .send(EventoEscritorio::Video {
            marco: 1,
            regiones: vec![Region::nueva(0, 0, 2, 2)],
            h264: vec![0, 0, 0, 1, 0x65],
        })
        .await
        .expect("vídeo");
    let mut video_recibido = false;
    let limite = tokio::time::Instant::now() + Duration::from_millis(300);
    loop {
        let restante = limite.saturating_duration_since(tokio::time::Instant::now());
        if restante.is_zero() {
            break;
        }
        match tokio::time::timeout(restante, ws.next()).await {
            Ok(Some(Ok(MensajeWs::Binary(datos)))) => {
                if datos.first() == Some(&0x07) {
                    video_recibido = true;
                    break;
                }
            }
            Ok(Some(Ok(_))) => continue,
            _ => break,
        }
    }
    assert!(!video_recibido, "sin H.264 no se manda VideoRdp");

    // Con H.264, otro visor sí lo recibe.
    let mut ws_h264 = conectar_ws_correcto(entorno.puerto).await;
    enviar_hola(&mut ws_h264, &clave, true).await;
    let _ = esperar_hello(&mut ws_h264, Duration::from_secs(2)).await;
    eventos_tx
        .send(EventoEscritorio::Video {
            marco: 2,
            regiones: vec![Region::nueva(0, 0, 2, 2)],
            h264: vec![0, 0, 0, 1, 0x65],
        })
        .await
        .expect("vídeo");
    let video = esperar_binario(&mut ws_h264, 0x07, Duration::from_secs(2)).await;
    assert_eq!(u32_en(&video, 1), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn la_reconexion_renueva_la_fuente_sin_recrear_el_visor() {
    let entorno = entorno().await;
    let addr = entorno.addr();
    let _avisos = drenar_avisos(entorno.avisos);
    let (sesion, _eventos_tx, _ordenes) =
        alta_sesion(&entorno.registro, info(41, TipoHost::Rdp), true);
    let enlace = entorno.registro.enlace(41).expect("enlace");
    let clave = canjear(addr, &enlace).await.json()["clave"]
        .as_str()
        .unwrap()
        .to_string();
    let mut ws = conectar_ws_correcto(entorno.puerto).await;
    enviar_hola(&mut ws, &clave, true).await;
    let _ = esperar_hello(&mut ws, Duration::from_secs(2)).await;
    esperar_repintado_inicial(&mut ws, 640, 480).await;

    sesion.estado_visor("caido", Some("sin conexión"));
    let estado = esperar_estado(&mut ws, "caido").await;
    assert_eq!(estado["motivo"], "sin conexión");

    // Se cambia la fuente sin recrear el visor: el WebSocket sigue.
    let (nuevos_eventos_tx, nuevos_eventos_rx) = mpsc::channel(64);
    let (nuevas_ordenes_tx, _nuevas_ordenes_rx) = mpsc::channel(64);
    assert!(
        sesion
            .renovar_fuente(nuevos_eventos_rx, nuevas_ordenes_tx)
            .await
    );
    let _ = esperar_estado(&mut ws, "conectando").await;
    let _ = esperar_hello(&mut ws, Duration::from_secs(2)).await;
    esperar_repintado_inicial(&mut ws, 640, 480).await;

    nuevos_eventos_tx
        .send(EventoEscritorio::Rect {
            x: 3,
            y: 3,
            ancho: 1,
            alto: 1,
            pixeles: vec![4, 5, 6, 255],
        })
        .await
        .expect("rect de la fuente nueva");
    let rect = esperar_binario(&mut ws, 0x05, Duration::from_secs(2)).await;
    assert_eq!(u16_en(&rect, 1), 3);
    assert_eq!(&rect[9..], &[4, 5, 6, 255]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn al_cerrar_la_sesion_el_visor_recibe_el_estado_y_el_close() {
    let entorno = entorno().await;
    let addr = entorno.addr();
    let _avisos = drenar_avisos(entorno.avisos);
    let (_sesion, _eventos, _ordenes) =
        alta_sesion(&entorno.registro, info(51, TipoHost::Vnc), true);
    let enlace = entorno.registro.enlace(51).expect("enlace");
    let clave = canjear(addr, &enlace).await.json()["clave"]
        .as_str()
        .unwrap()
        .to_string();
    let mut ws = conectar_ws_correcto(entorno.puerto).await;
    enviar_hola(&mut ws, &clave, true).await;
    let _ = esperar_hello(&mut ws, Duration::from_secs(2)).await;

    entorno.registro.baja(51);
    let estado = esperar_estado(&mut ws, "cerrado").await;
    assert_eq!(estado["estado"], "cerrado");
    assert!(esperar_cierre(&mut ws, Duration::from_secs(2)).await);
    assert!(!entorno.registro.visor_conectado(51));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn el_puente_manda_estado_y_avisos_de_caida_y_cierre() {
    let entorno = entorno().await;
    let avisos = drenar_avisos(entorno.avisos);
    let (_sesion, eventos_tx, _ordenes) =
        alta_sesion(&entorno.registro, info(61, TipoHost::Vnc), true);
    eventos_tx
        .send(EventoEscritorio::Caido("el servidor cerró".to_string()))
        .await
        .expect("caída");
    eventos_tx
        .send(EventoEscritorio::Cerrado(
            "cerrado por el remoto".to_string(),
        ))
        .await
        .expect("cierre");
    esperar_hasta(Duration::from_secs(2), || {
        let vistos = avisos.lock().unwrap();
        vistos.iter().any(|aviso| {
            matches!(aviso, AvisoVisor::Caido { id: 61, motivo } if motivo == "el servidor cerró")
        }) && vistos.iter().any(|aviso| {
            matches!(aviso, AvisoVisor::Cerrado { id: 61, motivo } if motivo == "cerrado por el remoto")
        })
    })
    .await;
}

// ---------------------------------------------------------------- regresiones
// de la revisión adversarial (C2, M4, M5, M7 y menores m1/m2/m7/m8/m9)

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn los_textos_no_congelan_el_puente() {
    // C2: los bytes de los textos (ping, clip…) también se descuentan. Con el
    // contador sin descontar, ocho clip de 1 MB dejaban el puente a 8 MB y no
    // volvía a mandar imagen.
    let entorno = entorno().await;
    let addr = entorno.addr();
    let _avisos = drenar_avisos(entorno.avisos);
    let (_sesion, eventos_tx, _ordenes) =
        alta_sesion(&entorno.registro, info(113, TipoHost::Vnc), true);
    let enlace = entorno.registro.enlace(113).expect("enlace");
    let clave = canjear(addr, &enlace).await.json()["clave"]
        .as_str()
        .unwrap()
        .to_string();
    let mut ws = conectar_ws_correcto(entorno.puerto).await;
    enviar_hola(&mut ws, &clave, true).await;
    let _ = esperar_hello(&mut ws, Duration::from_secs(2)).await;
    esperar_repintado_inicial(&mut ws, 640, 480).await;

    let texto = "a".repeat(1024 * 1024);
    for vuelta in 0..10 {
        eventos_tx
            .send(EventoEscritorio::Portapapeles(texto.clone()))
            .await
            .expect("clip");
        let clip = esperar_json_que(&mut ws, Duration::from_secs(5), |mensaje| {
            mensaje["t"] == "clip"
        })
        .await;
        assert!(
            clip["texto"].as_str().unwrap().len() >= 1024 * 1024,
            "clip {vuelta}"
        );
    }
    // El puente sigue mandando imagen después de 10 MB de texto.
    eventos_tx
        .send(EventoEscritorio::Rect {
            x: 0,
            y: 0,
            ancho: 1,
            alto: 1,
            pixeles: vec![1, 2, 3, 255],
        })
        .await
        .expect("rect");
    let rect = esperar_binario(&mut ws, 0x05, Duration::from_secs(2)).await;
    assert_eq!(&rect[9..], &[1, 2, 3, 255]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn un_rectangulo_mayor_que_la_cola_se_trocea() {
    // C2: el repintado de un escritorio de 2048×2048 pesa 16 MB; se manda en
    // trozos que caben en COLA_MAX y el visor los presenta con un Marco.
    let entorno = entorno().await;
    let addr = entorno.addr();
    let _avisos = drenar_avisos(entorno.avisos);
    let mut base = info(123, TipoHost::Vnc);
    base.ancho = 2048;
    base.alto = 2048;
    let (_sesion, _eventos, _ordenes) = alta_sesion(&entorno.registro, base, true);
    let enlace = entorno.registro.enlace(123).expect("enlace");
    let clave = canjear(addr, &enlace).await.json()["clave"]
        .as_str()
        .unwrap()
        .to_string();
    let mut ws = conectar_ws_correcto(entorno.puerto).await;
    enviar_hola(&mut ws, &clave, true).await;
    let hello = esperar_hello(&mut ws, Duration::from_secs(2)).await;
    assert_eq!(hello["w"], 2048);

    let mut area = 0u32;
    let mut trozos = 0usize;
    while area < 2048 * 2048 {
        let rect = esperar_binario(&mut ws, 0x05, Duration::from_secs(5)).await;
        assert!(
            rect.len() <= COLA_MAX,
            "cada Rect cabe en la cola ({} bytes)",
            rect.len()
        );
        let ancho = u32::from(u16_en(&rect, 5));
        let alto = u32::from(u16_en(&rect, 7));
        assert_eq!(ancho, 2048);
        area += ancho * alto;
        trozos += 1;
        assert!(trozos <= 16, "no se manda un rectángulo gigante de una vez");
    }
    // Los trozos cubren el escritorio entero (el último puede pasarse de
    // filas porque se manda completo).
    assert!(area >= 2048 * 2048, "área {area}");
    let _ = esperar_binario(&mut ws, 0x08, Duration::from_secs(2)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn arrancar_y_parar_concurrentes_quedan_coherentes() {
    // M4: arrancar y parar comparten el cerrojo de ciclo de vida. Tras cada
    // carrera, o hay puerto y /api/salud responde, o no hay puerto; nunca un
    // puerto publicado sin listener ni un listener vivo con el puerto a cero.
    let (registro, _avisos) = RegistroVisor::nuevo_con_tiempos(tiempos_prueba());
    let registro = Arc::new(registro);
    for vuelta in 0..30 {
        let arranque = {
            let registro = registro.clone();
            tokio::spawn(async move { registro.arrancar(0).await })
        };
        let parada = {
            let registro = registro.clone();
            tokio::spawn(async move {
                registro.parar();
            })
        };
        let arrancado = arranque.await.expect("tarea de arranque");
        parada.await.expect("tarea de parada");
        match (arrancado, registro.puerto()) {
            (Ok(_), None) => {}
            (Ok(puerto), Some(actual)) => {
                assert_eq!(puerto, actual);
                let origen = format!("http://127.0.0.1:{actual}");
                let addr = SocketAddr::from(([127, 0, 0, 1], actual));
                let salud = http_get_con(addr, "/api/salud", None, Some(&origen)).await;
                assert_eq!(salud.estado, 200, "vuelta {vuelta}");
            }
            (Err(_), None) => {}
            (Err(_), Some(_)) => {
                panic!("vuelta {vuelta}: arranque fallido con puerto publicado");
            }
        }
        registro.parar();
        assert!(registro.puerto().is_none(), "vuelta {vuelta}");
    }
    // Después de las carreras el registro sigue sirviendo.
    let puerto = registro.arrancar(0).await.expect("arrancar al final");
    let origen = format!("http://127.0.0.1:{puerto}");
    let addr = SocketAddr::from(([127, 0, 0, 1], puerto));
    let salud = http_get_con(addr, "/api/salud", None, Some(&origen)).await;
    assert_eq!(salud.estado, 200);
    registro.parar();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn un_visor_que_no_lee_se_cierra_por_plazo() {
    // M5: si el visor deja de leer (pestaña congelada), los envíos tienen
    // plazo y la tarea se cierra: el servidor deja de creer que hay visor.
    let entorno = entorno().await;
    let addr = entorno.addr();
    let _avisos = drenar_avisos(entorno.avisos);
    // El repintado inicial de un escritorio de 2048×2048 se manda en trozos de
    // unos 8 MB que no caben en los búferes del socket si el cliente no lee.
    let mut base = info(133, TipoHost::Vnc);
    base.ancho = 2048;
    base.alto = 2048;
    let (_sesion, _eventos, _ordenes) = alta_sesion(&entorno.registro, base, true);
    let enlace = entorno.registro.enlace(133).expect("enlace");
    let clave = canjear(addr, &enlace).await.json()["clave"]
        .as_str()
        .unwrap()
        .to_string();
    let mut ws = conectar_ws_correcto(entorno.puerto).await;
    enviar_hola(&mut ws, &clave, true).await;
    // El visor llega a conectar y, a partir de aquí, el cliente NO lee nada.
    esperar_hasta(Duration::from_secs(2), || {
        entorno.registro.visor_conectado(133)
    })
    .await;
    // El repintado de 16 MB no cabe en los búferes del socket: el plazo de
    // escritura cierra la conexión y el registro deja de tener visor.
    esperar_hasta(Duration::from_secs(6), || {
        !entorno.registro.visor_conectado(133)
    })
    .await;
    assert!(esperar_cierre(&mut ws, Duration::from_secs(8)).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn un_cambio_de_tamano_no_pierde_la_imagen_valida() {
    // m7: si llega un Rect y justo después un tamaño menor, la zona vieja no
    // puede vaciar el sucio sin pintar la parte que sí cabe en el búfer.
    let entorno = entorno().await;
    let addr = entorno.addr();
    let _avisos = drenar_avisos(entorno.avisos);
    let (_sesion, eventos_tx, _ordenes) =
        alta_sesion(&entorno.registro, info(143, TipoHost::Vnc), true);
    let enlace = entorno.registro.enlace(143).expect("enlace");
    let clave = canjear(addr, &enlace).await.json()["clave"]
        .as_str()
        .unwrap()
        .to_string();
    let mut ws = conectar_ws_correcto(entorno.puerto).await;
    enviar_hola(&mut ws, &clave, true).await;
    let _ = esperar_hello(&mut ws, Duration::from_secs(2)).await;
    esperar_repintado_inicial(&mut ws, 640, 480).await;

    eventos_tx
        .send(EventoEscritorio::Rect {
            x: 0,
            y: 0,
            ancho: 640,
            alto: 480,
            pixeles: vec![9; 640 * 480 * 4],
        })
        .await
        .expect("rect completo");
    eventos_tx
        .send(EventoEscritorio::Tamano {
            ancho: 320,
            alto: 240,
        })
        .await
        .expect("tamaño menor");

    let mut visto = false;
    for _ in 0..5 {
        let rect = esperar_binario(&mut ws, 0x05, Duration::from_secs(3)).await;
        let ancho = u16_en(&rect, 5);
        let alto = u16_en(&rect, 7);
        assert!(ancho <= 640 && alto <= 480);
        if ancho == 320 && alto == 240 {
            visto = true;
            break;
        }
    }
    assert!(visto, "la parte válida del búfer nuevo se pinta igual");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn los_marcos_de_video_cuentan_para_los_fps() {
    // m8: el H.264 no pasa por Marco; aun así cuenta en los fps de la lista.
    let entorno = entorno().await;
    let addr = entorno.addr();
    let _avisos = drenar_avisos(entorno.avisos);
    let mut base = info(153, TipoHost::Rdp);
    base.imagen = Some(ImagenEscritorio::H264);
    let (_sesion, eventos_tx, _ordenes) = alta_sesion(&entorno.registro, base, true);
    let enlace = entorno.registro.enlace(153).expect("enlace");
    let clave = canjear(addr, &enlace).await.json()["clave"]
        .as_str()
        .unwrap()
        .to_string();
    let mut ws = conectar_ws_correcto(entorno.puerto).await;
    enviar_hola(&mut ws, &clave, true).await;
    let _ = esperar_hello(&mut ws, Duration::from_secs(2)).await;
    for marco in 0..20u32 {
        eventos_tx
            .send(EventoEscritorio::Video {
                marco,
                regiones: vec![Region::nueva(0, 0, 2, 2)],
                h264: vec![0, 0, 0, 1, 0x65],
            })
            .await
            .expect("vídeo");
    }
    esperar_hasta(Duration::from_secs(3), || {
        entorno.registro.lista()[0].fps > 0
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn los_avisos_de_estado_no_se_pierden_con_el_canal_de_ordenes_lleno() {
    // m9: el aviso de estado va por su canal sin pérdida. Con el canal de
    // comandos lleno (la fuente no consume órdenes), un estado_visor con
    // try_send se perdía y el servidor seguía creyendo que había visor.
    let entorno = entorno().await;
    let addr = entorno.addr();
    let _avisos = drenar_avisos(entorno.avisos);
    let (sesion, _eventos, mut ordenes_rx) =
        alta_sesion(&entorno.registro, info(163, TipoHost::Vnc), true);
    let enlace = entorno.registro.enlace(163).expect("enlace");
    let clave = canjear(addr, &enlace).await.json()["clave"]
        .as_str()
        .unwrap()
        .to_string();
    let mut ws = conectar_ws_correcto(entorno.puerto).await;
    enviar_hola(&mut ws, &clave, true).await;
    let _ = esperar_hello(&mut ws, Duration::from_secs(2)).await;
    esperar_repintado_inicial(&mut ws, 640, 480).await;

    // El bucle se atasca porque la fuente no lee órdenes (canal de 64 lleno);
    // la tarea del WebSocket se atasca al llenarse el canal de comandos.
    for _ in 0..300 {
        enviar_texto(&mut ws, "kc,97,KeyA,1").await;
    }
    tokio::time::sleep(Duration::from_millis(150)).await;
    sesion.estado_visor("caido", Some("prueba"));
    // La fuente empieza a consumir: el bucle se desbloquea y procesa el aviso.
    let mut leidas = 0;
    while leidas < 300 {
        match tokio::time::timeout(Duration::from_secs(2), ordenes_rx.recv()).await {
            Ok(Some(_)) => leidas += 1,
            _ => break,
        }
    }
    let estado = esperar_estado(&mut ws, "caido").await;
    assert_eq!(estado["motivo"], "prueba");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn con_la_pestana_oculta_no_se_manda_video() {
    // M7: el H.264 no se manda con la pestaña oculta; al volver, vuelve.
    let entorno = entorno().await;
    let addr = entorno.addr();
    let _avisos = drenar_avisos(entorno.avisos);
    let mut base = info(173, TipoHost::Rdp);
    base.imagen = Some(ImagenEscritorio::H264);
    let (_sesion, eventos_tx, mut ordenes_rx) = alta_sesion(&entorno.registro, base, true);
    let enlace = entorno.registro.enlace(173).expect("enlace");
    let clave = canjear(addr, &enlace).await.json()["clave"]
        .as_str()
        .unwrap()
        .to_string();
    let mut ws = conectar_ws_correcto(entorno.puerto).await;
    enviar_hola(&mut ws, &clave, true).await;
    let hello = esperar_hello(&mut ws, Duration::from_secs(2)).await;
    assert_eq!(hello["imagen"], "h264");

    enviar_texto(&mut ws, "v,0").await;
    assert_eq!(
        esperar_orden(&mut ordenes_rx).await,
        OrdenEscritorio::Visible(false)
    );
    eventos_tx
        .send(EventoEscritorio::Video {
            marco: 1,
            regiones: vec![Region::nueva(0, 0, 2, 2)],
            h264: vec![0, 0, 0, 1, 0x65],
        })
        .await
        .expect("vídeo oculto");
    assert!(
        !no_llega_binario(&mut ws, 0x07, Duration::from_millis(300)).await,
        "con v,0 no se manda vídeo"
    );

    enviar_texto(&mut ws, "v,1").await;
    assert_eq!(
        esperar_orden(&mut ordenes_rx).await,
        OrdenEscritorio::Visible(true)
    );
    eventos_tx
        .send(EventoEscritorio::Video {
            marco: 2,
            regiones: vec![Region::nueva(0, 0, 2, 2)],
            h264: vec![0, 0, 0, 1, 0x65],
        })
        .await
        .expect("vídeo visible");
    let video = esperar_binario(&mut ws, 0x07, Duration::from_secs(2)).await;
    assert_eq!(u32_en(&video, 1), 2);
}
