//! Pruebas del cliente RDP (Fase 12) contra un servidor ironrdp-server 0.13
//! en proceso, sobre un duplex: sin red externa y sin abrir puertos.
//!
//! Se prueban TLS, la política de huella (nueva, igual y cambiada), las
//! credenciales (bien y mal), los mapas de bits, el teclado y el ratón, el
//! portapapeles CLIPRDR y Display Control con la reactivación.

use std::num::{NonZeroU16, NonZeroUsize};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use ironrdp::cliprdr::backend::{ClipboardMessage, CliprdrBackend, CliprdrBackendFactory};
use ironrdp::cliprdr::pdu::{
    ClipboardFormat, ClipboardFormatId, ClipboardGeneralCapabilityFlags, FileContentsRequest,
    FileContentsResponse, FormatDataRequest, FormatDataResponse, LockDataId,
};
use ironrdp::core::impl_as_any;
use ironrdp::displaycontrol::pdu::DisplayControlMonitorLayout;
use ironrdp::pdu::rdp::capability_sets::BitmapCodecs;
use ironrdp_server::{
    BitmapUpdate, CliprdrServerFactory, Credentials as CredencialesServidor,
    DesktopSize as TamanoServidor, DisplayUpdate, KeyboardEvent, MouseEvent,
    PixelFormat as PixelServidor, RdpServer, RdpServerDisplay, RdpServerDisplayUpdates,
    RdpServerInputHandler, ServerEvent, ServerEventSender, TlsIdentityCtx,
};
use magi_nucleo::escritorio::rdp;
use magi_nucleo::escritorio::{
    Conexion, ConfigEscritorio, CredencialesEscritorio, DatosCertificado, DialogosEscritorio,
    ErrorEscritorio, EventoEscritorio, ImagenEscritorio, OpcionesEscritorio, OrdenEscritorio,
    PeticionCredenciales, TipoCambioCertificado, TipoHost,
};
use tokio::sync::mpsc;
use zeroize::Zeroizing;

/// Certificado autofirmado de pruebas (CN=localhost, SAN DNS:localhost y
/// 127.0.0.1), generado una vez y versionado aquí para no depender de openssl.
const CERT_PEM: &str = r#"-----BEGIN CERTIFICATE-----
MIIDUzCCAjugAwIBAgIUNNFvdK2efsedugs8Ki7ZE+5RKhMwDQYJKoZIhvcNAQEL
BQAwKzESMBAGA1UEAwwJbG9jYWxob3N0MRUwEwYDVQQKDAxNQUdJIFBydWViYXMw
HhcNMjYxMTA0MjIwNzI3WhcNMzYxMTAxMjIwNzI3WjArMRIwEAYDVQQDDAlsb2Nh
bGhvc3QxFTATBgNVBAoMDE1BR0kgUHJ1ZWJhczCCASIwDQYJKoZIhvcNAQEBBQAD
ggEPADCCAQoCggEBAPFwLRQyhLekdQSHIcg7vKSOGwE+ew1yhrPxxBkN2SyFTd1N
qvFMHFlCqkx+LSoMncqO+bABXFEuUruCU4YJqPbNujTeNG1U3S7IEl5Oonhdf9mT
Ux0pJXcHCGAdve45frUBCLJRi/QNGuHqDy/ZyBh+B/SRS4VANDiwk3iAMLH/TVXq
3ejUZGOXCRDZNRN01NokEcE3ZMaEZuZZBxTjCsB9cg/LkWGxIHe7Gv9Fc/qyluZA
sk4AscSiEYvk5c3MYn4UdNxObMOCGhEy8SA2TnEWBQEzFPRwPz58aco620WTHXPb
+Yjg2xyErH5IAh1TtsCnV21YIfCZthyAGpKUc5kCAwEAAaNvMG0wHQYDVR0OBBYE
FHLN4I3gMULtnYCr6oyOfF+4+u39MB8GA1UdIwQYMBaAFHLN4I3gMULtnYCr6oyO
fF+4+u39MA8GA1UdEwEB/wQFMAMBAf8wGgYDVR0RBBMwEYIJbG9jYWxob3N0hwR/
AAABMA0GCSqGSIb3DQEBCwUAA4IBAQAOXJfpHOzi0HI11PtNdG9GlMYDHIvElN1m
r3hPKcS1B8I9mESIOUmt8v5TjvP8bNkRjBT0KZ+1ih+nGydeP5wsCJWEkAdRbTMa
Df5FQCzpt4TNJm1t7SnuC5X/FjlnjR8MBcWTFXXElWIuMZjet6Z/Iw8B2Wf6ETrG
y2UZqLkyyMScD3ykLRdu+tbcYxxzfb6/k133x1/Umn6/BOG+QBrKYqW2TEVDd7if
jk3cdWYejp9ib1ItgwAf/CR2lELcoj+g3xAiswSNEg/B3iCqrF+ubbCw8c+VoznV
d8dfMYDjJ+9PH8zjoX5RV6BmuSsUKJuk0OPpHF3uWJFdQVTmocEz
-----END CERTIFICATE-----"#;

/// Clave del certificado de pruebas.
const CLAVE_PEM: &str = r#"-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQDxcC0UMoS3pHUE
hyHIO7ykjhsBPnsNcoaz8cQZDdkshU3dTarxTBxZQqpMfi0qDJ3KjvmwAVxRLlK7
glOGCaj2zbo03jRtVN0uyBJeTqJ4XX/Zk1MdKSV3BwhgHb3uOX61AQiyUYv0DRrh
6g8v2cgYfgf0kUuFQDQ4sJN4gDCx/01V6t3o1GRjlwkQ2TUTdNTaJBHBN2TGhGbm
WQcU4wrAfXIPy5FhsSB3uxr/RXP6spbmQLJOALHEohGL5OXNzGJ+FHTcTmzDghoR
MvEgNk5xFgUBMxT0cD8+fGnKOttFkx1z2/mI4NschKx+SAIdU7bAp1dtWCHwmbYc
gBqSlHOZAgMBAAECggEAL943Oc1oTcaf4rVuB7Z8SL3Z8c2y9PATb+LLp9H8kFyf
mvRWo3+dET9r5z2O88rLkAj3zSL/7CJ8iaulpqoMo14XgHrbtx9/uNI2/hamFKoo
+njAnkP1pxswWGodKXmhuv5I/J25Wzor7cWLnEeX6e824ZDbAcDAecrceiQkQ23B
j3yhcpuW7GlMem2z51YQLTGSaVaPNUbZ8qtxmvVWAOty/uAHXoeHKiQPxfko9oXi
OHg8tBDdN4jw6Ya5/7R7aIR9e8mV5nG11lxcWBmRfiznYz9+xmO69QhaQ8t/KZce
o5I3vSbVbx+ZS64S2NNkpsJRWIgLWzJ2UKczQPI6DwKBgQD7QTW0hLdO9mpHPFui
lSOdep498YG4GNg4DD1Kvsl/uCfqmiBdS0JvY7I+MyDC5bV9OTlHuJm0rq8sAeyK
XCsAwklO7TUZCjglCDAEy/0dWWg5LT8++3XGPiuy0hA8vnYD54i3zK+kH1GRbJOu
uZtR+pv1W6+n/XXkQ4V2/vu9twKBgQD1/4EephDbnszZZjG7af7ArqlUH5o9BQfh
3YnlF5iIu3KFxMrKgzC5f2MhLtUvVvRbiwiibGQTtK+dY+mmJ+K2Pvj3LN+qPEG+
5Gus7aBKlRDCR3z98M/PhyaYcqZPbAH+tcWDajNFoOyGO8pw3a6IfZmo/+2nIcTG
uTL9LdhZLwKBgG4wzSLcSaBhxX50ZCPLkJESI7A66I8apkj+ZSsQvUI8O5VR/vur
c/lyGcvZlHalU5EVtZ9UwGRTcBO2zExp25o7mvb4ZR8UMkdbbePMqrdibKkWxPQU
SDSnM3b7VY+uYAeaVYV2SVmLOrSyj5pDYHD1ncxfZ4wfRsLTMgdcEH/FAoGBAOw8
Kesc3yOJJH/BvBCWgabYFJRMpPnkUuV8smfmeP0Vxazghz5jLe91feTn7KJBhvhC
VFBmdtTVt7AVutNtijIp1zzHQyNgKmL/SMgQitYUwLtRm57iGy7YQyAzObMbre7E
8KB65DIBkCJ4WLh6qpk4xM+etsLBkw6j22t3E0TRAoGAH/UnSFLZaZUkPKMETkjT
JugPrMWRZRTrvap1apgeaRpLZH4kNGUE4PebmWdWCgiNA6orSw6eS0VBa5zS01i8
qR3RFsmCEKgi5dxDNx0Py9+LAQouKHpHXOmAAUcjqFWlx2xHQ9pLwEIFJweFB/KZ
rFzqzOJ3iH6AS5Yo4YX9XNw=
-----END PRIVATE KEY-----"#;

/// Plazo de cada espera de una prueba.
const PLAZO: Duration = Duration::from_secs(10);

// ------------------------------------------------------------------ ayudas

fn identidad_tls() -> TlsIdentityCtx {
    let directorio = tempfile::tempdir().expect("directorio temporal");
    std::fs::write(directorio.path().join("cert.pem"), CERT_PEM).expect("certificado de pruebas");
    std::fs::write(directorio.path().join("clave.pem"), CLAVE_PEM).expect("clave de pruebas");
    TlsIdentityCtx::init_from_paths(
        &directorio.path().join("cert.pem"),
        &directorio.path().join("clave.pem"),
    )
    .expect("identidad TLS de pruebas")
}

fn configuracion(h264: bool) -> ConfigEscritorio {
    ConfigEscritorio {
        tipo: TipoHost::Rdp,
        direccion: "localhost".to_string(),
        puerto: 3389,
        usuario: Some("usuario".to_string()),
        dominio: None,
        opciones: OpcionesEscritorio::por_defecto(TipoHost::Rdp),
        distribucion_teclado: "en-US".to_string(),
        h264,
        tamano_inicial: (800, 600),
        portapapeles: true,
        huella_escritorio: None,
    }
}

fn credenciales(usuario: &str, contrasena: &str) -> CredencialesEscritorio {
    CredencialesEscritorio {
        usuario: usuario.to_string(),
        dominio: None,
        contrasena: Zeroizing::new(contrasena.to_string()),
        recordar: false,
    }
}

fn servidor_hybrid<I, D>(
    usuario: &str,
    contrasena: &str,
    entrada: I,
    pantalla: D,
    portapapeles: Option<Box<dyn CliprdrServerFactory>>,
) -> RdpServer
where
    I: RdpServerInputHandler + 'static,
    D: RdpServerDisplay + 'static,
{
    let identidad = identidad_tls();
    let acceptor = identidad.make_acceptor().expect("acceptor TLS");
    let mut servidor = RdpServer::builder()
        .with_addr(
            "127.0.0.1:0"
                .parse::<std::net::SocketAddr>()
                .expect("dirección"),
        )
        .with_hybrid(acceptor, identidad.pub_key)
        .with_input_handler(entrada)
        .with_display_handler(pantalla)
        // Sin códecs el servidor manda mapas de bits en bruto (CODEC_ID_NONE):
        // los píxeles llegan exactos y no depende de QOI/zstd, que el cliente
        // de MAGI no ofrece en producción.
        .with_bitmap_codecs(BitmapCodecs(Vec::new()))
        .with_cliprdr_factory(portapapeles)
        .build();
    servidor.set_credentials(Some(CredencialesServidor {
        username: usuario.to_string(),
        password: contrasena.to_string(),
        domain: None,
    }));
    servidor
}

fn servidor_tls<I, D>(entrada: I, pantalla: D) -> RdpServer
where
    I: RdpServerInputHandler + 'static,
    D: RdpServerDisplay + 'static,
{
    let identidad = identidad_tls();
    let acceptor = identidad.make_acceptor().expect("acceptor TLS");
    RdpServer::builder()
        .with_addr(
            "127.0.0.1:0"
                .parse::<std::net::SocketAddr>()
                .expect("dirección"),
        )
        .with_tls(acceptor)
        .with_input_handler(entrada)
        .with_display_handler(pantalla)
        .with_bitmap_codecs(BitmapCodecs(Vec::new()))
        .with_cliprdr_factory(None)
        .build()
}

fn servidor_sin_tls<I, D>(entrada: I, pantalla: D) -> RdpServer
where
    I: RdpServerInputHandler + 'static,
    D: RdpServerDisplay + 'static,
{
    RdpServer::builder()
        .with_addr(
            "127.0.0.1:0"
                .parse::<std::net::SocketAddr>()
                .expect("dirección"),
        )
        .with_no_security()
        .with_input_handler(entrada)
        .with_display_handler(pantalla)
        .with_cliprdr_factory(None)
        .build()
}

fn lanzar_servidor(
    mut servidor: RdpServer,
    flujo: tokio::io::DuplexStream,
) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn_local(async move {
        let _ = servidor.run_connection(flujo).await;
    })
}

// ------------------------------------------------------------------ diálogos

/// Lo que ha pedido la tarea de conexión, en orden.
#[derive(Debug, Clone)]
enum DialogoVisto {
    Certificado(DatosCertificado),
    Credenciales(PeticionCredenciales),
}

struct DialogoPrueba {
    vistos: mpsc::UnboundedSender<DialogoVisto>,
    aceptar: bool,
    respuesta: Option<CredencialesEscritorio>,
}

#[async_trait]
impl DialogosEscritorio for DialogoPrueba {
    async fn certificado(&self, datos: DatosCertificado) -> Result<bool, ErrorEscritorio> {
        let _ = self.vistos.send(DialogoVisto::Certificado(datos));
        Ok(self.aceptar)
    }

    async fn credenciales(
        &self,
        peticion: PeticionCredenciales,
    ) -> Result<Option<CredencialesEscritorio>, ErrorEscritorio> {
        let _ = self.vistos.send(DialogoVisto::Credenciales(peticion));
        Ok(self.respuesta.clone())
    }
}

// ------------------------------------------------------------------ cliente

struct Cliente {
    eventos: mpsc::Receiver<EventoEscritorio>,
    ordenes: Option<mpsc::Sender<OrdenEscritorio>>,
    vistos: mpsc::UnboundedReceiver<DialogoVisto>,
    tarea: tokio::task::JoinHandle<Result<(), ErrorEscritorio>>,
}

fn arrancar_cliente(
    flujo: tokio::io::DuplexStream,
    config: ConfigEscritorio,
    aceptar: bool,
    respuesta: Option<CredencialesEscritorio>,
) -> Cliente {
    let (tx_eventos, eventos) = mpsc::channel(256);
    let (tx_ordenes, rx_ordenes) = mpsc::channel(64);
    let (tx_vistos, vistos) = mpsc::unbounded_channel();

    let dialogo = Arc::new(DialogoPrueba {
        vistos: tx_vistos,
        aceptar,
        respuesta,
    });

    let conexion = Conexion {
        flujo: Box::new(flujo),
        config,
        dialogo,
        eventos: tx_eventos,
        ordenes: rx_ordenes,
        intento: 1,
    };

    let tarea = tokio::task::spawn_local(async move { rdp::abrir(conexion).await });

    Cliente {
        eventos,
        ordenes: Some(tx_ordenes),
        vistos,
        tarea,
    }
}

impl Cliente {
    async fn siguiente(&mut self) -> EventoEscritorio {
        tokio::time::timeout(PLAZO, self.eventos.recv())
            .await
            .expect("tiempo agotado esperando un evento del escritorio")
            .expect("el canal de eventos se ha cerrado")
    }

    async fn siguiente_dialogo(&mut self) -> DialogoVisto {
        tokio::time::timeout(PLAZO, self.vistos.recv())
            .await
            .expect("tiempo agotado esperando un diálogo")
            .expect("el canal de diálogos se ha cerrado")
    }

    async fn certificado(&mut self) -> DatosCertificado {
        match self.siguiente_dialogo().await {
            DialogoVisto::Certificado(datos) => datos,
            DialogoVisto::Credenciales(peticion) => {
                panic!("se esperaba el certificado y ha llegado una petición de credenciales: {peticion:?}")
            }
        }
    }

    async fn peticion_credenciales(&mut self) -> PeticionCredenciales {
        match self.siguiente_dialogo().await {
            DialogoVisto::Credenciales(peticion) => peticion,
            DialogoVisto::Certificado(datos) => {
                panic!("se esperaban credenciales y ha llegado un certificado: {datos:?}")
            }
        }
    }

    fn dialogo_pendiente(&mut self) -> Option<DialogoVisto> {
        self.vistos.try_recv().ok()
    }

    async fn enviar(&self, orden: OrdenEscritorio) {
        self.ordenes
            .as_ref()
            .expect("canal de órdenes")
            .send(orden)
            .await
            .expect("la sesión debe seguir escuchando órdenes");
    }

    /// Espera el evento Tamano con el tamaño indicado, saltándose los demás.
    async fn esperar_tamano(&mut self, ancho: u16, alto: u16) {
        for _ in 0..40 {
            match self.siguiente().await {
                EventoEscritorio::Tamano {
                    ancho: vistos_ancho,
                    alto: vistos_alto,
                } if vistos_ancho == ancho && vistos_alto == alto => return,
                _ => continue,
            }
        }
        panic!("no ha llegado el tamaño {ancho}x{alto}");
    }

    /// Espera un rectángulo, saltándose los eventos que no lo son.
    async fn esperar_rect(&mut self) -> (u16, u16, u16, u16, Vec<u8>) {
        for _ in 0..60 {
            if let EventoEscritorio::Rect {
                x,
                y,
                ancho,
                alto,
                pixeles,
            } = self.siguiente().await
            {
                return (x, y, ancho, alto, pixeles);
            }
        }
        panic!("no ha llegado ningún rectángulo");
    }

    async fn cerrar(mut self) -> Result<(), ErrorEscritorio> {
        self.ordenes.take();
        tokio::time::timeout(PLAZO, self.tarea)
            .await
            .expect("tiempo agotado cerrando la sesión RDP")
            .expect("la tarea del cliente RDP no debe entrar en pánico")
    }

    async fn terminar(self) -> Result<(), ErrorEscritorio> {
        tokio::time::timeout(PLAZO, self.tarea)
            .await
            .expect("tiempo agotado esperando el fin de la sesión RDP")
            .expect("la tarea del cliente RDP no debe entrar en pánico")
    }

    fn abortar(self) {
        self.tarea.abort();
    }
}

// ------------------------------------------------------------------ pantalla

/// Canal por el que la prueba manda actualizaciones al servidor de pruebas.
type Envios = Arc<Mutex<Option<mpsc::UnboundedSender<DisplayUpdate>>>>;

struct Pantalla {
    tamano: TamanoServidor,
    envios: Envios,
    inicial: Option<DisplayUpdate>,
    disposiciones: mpsc::UnboundedSender<()>,
}

impl Pantalla {
    fn new(
        tamano: TamanoServidor,
        inicial: Option<DisplayUpdate>,
    ) -> (Self, Envios, mpsc::UnboundedReceiver<()>) {
        let envios = Arc::new(Mutex::new(None));
        let (tx_disposiciones, rx_disposiciones) = mpsc::unbounded_channel();
        (
            Self {
                tamano,
                envios: Arc::clone(&envios),
                inicial,
                disposiciones: tx_disposiciones,
            },
            envios,
            rx_disposiciones,
        )
    }
}

#[async_trait]
impl RdpServerDisplay for Pantalla {
    async fn size(&mut self) -> TamanoServidor {
        self.tamano
    }

    async fn updates(&mut self) -> anyhow::Result<Box<dyn RdpServerDisplayUpdates>> {
        let (tx, rx) = mpsc::unbounded_channel();
        if let Ok(mut guard) = self.envios.lock() {
            *guard = Some(tx.clone());
        }
        if let Some(actualizacion) = self.inicial.clone() {
            let _ = tx.send(actualizacion);
        }
        Ok(Box::new(Actualizaciones { rx }))
    }

    fn request_layout(&mut self, _disposicion: DisplayControlMonitorLayout) {
        let _ = self.disposiciones.send(());
        if let Ok(guard) = self.envios.lock() {
            if let Some(tx) = guard.as_ref() {
                let _ = tx.send(DisplayUpdate::Resize(TamanoServidor {
                    width: 1024,
                    height: 768,
                }));
            }
        }
    }
}

struct Actualizaciones {
    rx: mpsc::UnboundedReceiver<DisplayUpdate>,
}

#[async_trait]
impl RdpServerDisplayUpdates for Actualizaciones {
    async fn next_update(&mut self) -> anyhow::Result<Option<DisplayUpdate>> {
        Ok(self.rx.recv().await)
    }
}

fn bitmap(x: u16, y: u16, ancho: u16, alto: u16, color: [u8; 4]) -> DisplayUpdate {
    let mut datos = Vec::with_capacity(usize::from(ancho) * usize::from(alto) * 4);
    for _ in 0..(usize::from(ancho) * usize::from(alto)) {
        datos.extend_from_slice(&color);
    }
    DisplayUpdate::Bitmap(BitmapUpdate {
        x,
        y,
        width: NonZeroU16::new(ancho).expect("ancho distinto de cero"),
        height: NonZeroU16::new(alto).expect("alto distinto de cero"),
        format: PixelServidor::RgbA32,
        data: datos.into(),
        stride: NonZeroUsize::new(usize::from(ancho) * 4).expect("stride distinto de cero"),
    })
}

// ------------------------------------------------------------------ entrada

#[derive(Debug, Clone, PartialEq)]
enum Visto {
    Tecla {
        codigo: u8,
        extendida: bool,
        abajo: bool,
    },
    Unicode {
        codigo: u16,
        abajo: bool,
    },
    Raton {
        x: u16,
        y: u16,
    },
    Boton {
        cual: &'static str,
        abajo: bool,
    },
    Rueda(i16),
}

struct Entrada {
    vistos: mpsc::UnboundedSender<Visto>,
}

impl Entrada {
    fn vacia() -> Self {
        let (vistos, _receptor) = mpsc::unbounded_channel();
        Self { vistos }
    }
}

impl RdpServerInputHandler for Entrada {
    fn keyboard(&mut self, evento: KeyboardEvent) {
        let visto = match evento {
            KeyboardEvent::Pressed { code, extended } => Visto::Tecla {
                codigo: code,
                extendida: extended,
                abajo: true,
            },
            KeyboardEvent::Released { code, extended } => Visto::Tecla {
                codigo: code,
                extendida: extended,
                abajo: false,
            },
            KeyboardEvent::UnicodePressed(codigo) => Visto::Unicode {
                codigo,
                abajo: true,
            },
            KeyboardEvent::UnicodeReleased(codigo) => Visto::Unicode {
                codigo,
                abajo: false,
            },
            KeyboardEvent::Synchronize(_) => return,
        };
        let _ = self.vistos.send(visto);
    }

    fn mouse(&mut self, evento: MouseEvent) {
        let visto = match evento {
            MouseEvent::Move { x, y } => Visto::Raton { x, y },
            MouseEvent::LeftPressed => Visto::Boton {
                cual: "izquierdo",
                abajo: true,
            },
            MouseEvent::LeftReleased => Visto::Boton {
                cual: "izquierdo",
                abajo: false,
            },
            MouseEvent::RightPressed => Visto::Boton {
                cual: "derecho",
                abajo: true,
            },
            MouseEvent::RightReleased => Visto::Boton {
                cual: "derecho",
                abajo: false,
            },
            MouseEvent::MiddlePressed => Visto::Boton {
                cual: "central",
                abajo: true,
            },
            MouseEvent::MiddleReleased => Visto::Boton {
                cual: "central",
                abajo: false,
            },
            MouseEvent::Button4Pressed => Visto::Boton {
                cual: "atras",
                abajo: true,
            },
            MouseEvent::Button4Released => Visto::Boton {
                cual: "atras",
                abajo: false,
            },
            MouseEvent::Button5Pressed => Visto::Boton {
                cual: "adelante",
                abajo: true,
            },
            MouseEvent::Button5Released => Visto::Boton {
                cual: "adelante",
                abajo: false,
            },
            MouseEvent::VerticalScroll { value } => Visto::Rueda(value),
            other => {
                panic!("evento de ratón no esperado: {other:?}")
            }
        };
        let _ = self.vistos.send(visto);
    }
}

// --------------------------------------------------------------- portapapeles

#[derive(Debug, PartialEq)]
enum AvisoPortapapeles {
    FormatosRemotos(Vec<ClipboardFormatId>),
    DatosRemotos(String),
}

struct FabricaPortapapeles {
    eventos: Arc<Mutex<Option<mpsc::UnboundedSender<ServerEvent>>>>,
    avisos: mpsc::UnboundedSender<AvisoPortapapeles>,
}

impl ServerEventSender for FabricaPortapapeles {
    fn set_sender(&mut self, sender: mpsc::UnboundedSender<ServerEvent>) {
        if let Ok(mut guard) = self.eventos.lock() {
            *guard = Some(sender);
        }
    }
}

impl CliprdrBackendFactory for FabricaPortapapeles {
    fn build_cliprdr_backend(&self) -> Box<dyn CliprdrBackend> {
        Box::new(PortapapelesServidor {
            eventos: Arc::clone(&self.eventos),
            avisos: self.avisos.clone(),
        })
    }
}

impl CliprdrServerFactory for FabricaPortapapeles {}

#[derive(Debug)]
struct PortapapelesServidor {
    eventos: Arc<Mutex<Option<mpsc::UnboundedSender<ServerEvent>>>>,
    avisos: mpsc::UnboundedSender<AvisoPortapapeles>,
}

impl PortapapelesServidor {
    fn enviar(&self, mensaje: ClipboardMessage) {
        if let Ok(guard) = self.eventos.lock() {
            if let Some(tx) = guard.as_ref() {
                let _ = tx.send(ServerEvent::Clipboard(mensaje));
            }
        }
    }
}

impl_as_any!(PortapapelesServidor);

impl CliprdrBackend for PortapapelesServidor {
    fn temporary_directory(&self) -> &str {
        "/tmp"
    }

    fn client_capabilities(&self) -> ClipboardGeneralCapabilityFlags {
        ClipboardGeneralCapabilityFlags::empty()
    }

    fn on_ready(&mut self) {}

    fn on_request_format_list(&mut self) {}

    fn on_process_negotiated_capabilities(
        &mut self,
        _capacidades: ClipboardGeneralCapabilityFlags,
    ) {
    }

    fn on_remote_copy(&mut self, formatos: &[ClipboardFormat]) {
        let _ = self.avisos.send(AvisoPortapapeles::FormatosRemotos(
            formatos.iter().map(|formato| formato.id).collect(),
        ));
    }

    fn on_format_data_request(&mut self, peticion: FormatDataRequest) {
        if peticion.format == ClipboardFormatId::CF_UNICODETEXT {
            self.enviar(ClipboardMessage::SendFormatData(
                FormatDataResponse::new_unicode_string("texto remoto"),
            ));
        }
    }

    fn on_format_data_response(&mut self, respuesta: FormatDataResponse<'_>) {
        if let Ok(texto) = respuesta.to_unicode_string() {
            let _ = self.avisos.send(AvisoPortapapeles::DatosRemotos(texto));
        }
    }

    fn on_file_contents_request(&mut self, _peticion: FileContentsRequest) {}

    fn on_file_contents_response(&mut self, _respuesta: FileContentsResponse<'_>) {}

    fn on_lock(&mut self, _id: LockDataId) {}

    fn on_unlock(&mut self, _id: LockDataId) {}
}

// ------------------------------------------------------------------ pruebas

/// Envía una actualización de pantalla al servidor de pruebas.
fn enviar_actualizacion(envios: &Envios, actualizacion: DisplayUpdate) {
    let guard = envios.lock().expect("cerrojo de envíos");
    let tx = guard
        .as_ref()
        .expect("el servidor aún no ha pedido actualizaciones");
    tx.send(actualizacion).expect("canal de actualizaciones");
}

/// Espera el próximo evento Portapapeles con su texto.
async fn esperar_portapapeles(cliente: &mut Cliente) -> String {
    for _ in 0..40 {
        if let EventoEscritorio::Portapapeles(texto) = cliente.siguiente().await {
            return texto;
        }
    }
    panic!("no ha llegado el portapapeles remoto");
}

async fn esperar_aviso(rx: &mut mpsc::UnboundedReceiver<AvisoPortapapeles>) -> AvisoPortapapeles {
    tokio::time::timeout(PLAZO, rx.recv())
        .await
        .expect("tiempo agotado esperando un aviso del portapapeles")
        .expect("el canal de avisos se ha cerrado")
}

async fn huella_de_una_conexion() -> String {
    let (lado_cliente, lado_servidor) = tokio::io::duplex(1 << 20);
    let (pantalla, _envios, _disposiciones) = Pantalla::new(
        TamanoServidor {
            width: 800,
            height: 600,
        },
        None,
    );
    let servidor = servidor_hybrid("usuario", "contrasena", Entrada::vacia(), pantalla, None);
    let tarea_servidor = lanzar_servidor(servidor, lado_servidor);
    let mut cliente = arrancar_cliente(
        lado_cliente,
        configuracion(false),
        true,
        Some(credenciales("usuario", "contrasena")),
    );
    let datos = cliente.certificado().await;
    cliente.abortar();
    tarea_servidor.abort();
    datos.huella
}

#[tokio::test(flavor = "current_thread")]
async fn tls_certificado_nuevo_y_rectangulos() {
    let local = tokio::task::LocalSet::new();
    tokio::time::timeout(
        Duration::from_secs(40),
        local.run_until(async {
            let (lado_cliente, lado_servidor) = tokio::io::duplex(1 << 20);
            let (pantalla, _envios, _disposiciones) = Pantalla::new(
                TamanoServidor {
                    width: 800,
                    height: 600,
                },
                // En el cable el mapa de bits va en BGRX; el cliente lo pasa a
                // RGBA en el búfer del escritorio.
                Some(bitmap(0, 0, 64, 48, [0xC0, 0x80, 0x20, 0xFF])),
            );
            let servidor =
                servidor_hybrid("usuario", "contrasena", Entrada::vacia(), pantalla, None);
            let tarea_servidor = lanzar_servidor(servidor, lado_servidor);

            let mut cliente = arrancar_cliente(
                lado_cliente,
                configuracion(false),
                true,
                Some(credenciales("usuario", "contrasena")),
            );

            let datos = cliente.certificado().await;
            assert_eq!(datos.tipo, TipoCambioCertificado::Nuevo);
            assert!(datos.anterior.is_none());
            assert!(
                datos.huella.starts_with("SHA256:"),
                "huella inesperada: {}",
                datos.huella
            );
            assert_eq!(datos.huella.len(), "SHA256:".len() + 43);
            assert!(
                datos.sujeto.contains("localhost"),
                "sujeto: {}",
                datos.sujeto
            );
            assert!(!datos.caducidad.is_empty());

            let peticion = cliente.peticion_credenciales().await;
            assert_eq!(peticion.tipo, TipoHost::Rdp);
            assert_eq!(peticion.intento, 1);
            assert_eq!(peticion.usuario.as_deref(), Some("usuario"));

            match cliente.siguiente().await {
                EventoEscritorio::Abierto => {}
                otro => panic!("se esperaba Abierto: {otro:?}"),
            }
            match cliente.siguiente().await {
                EventoEscritorio::Imagen(ImagenEscritorio::Rectangulos) => {}
                otro => panic!("se esperaba Imagen(Rectangulos): {otro:?}"),
            }
            cliente.esperar_tamano(800, 600).await;

            let (x, y, ancho, alto, pixeles) = cliente.esperar_rect().await;
            assert_eq!((x, y, ancho, alto), (0, 0, 64, 48));
            assert_eq!(pixeles.len(), 64 * 48 * 4);
            assert_eq!(&pixeles[..4], &[0x20, 0x80, 0xC0, 0xFF]);
            assert_eq!(&pixeles[pixeles.len() - 4..], &[0x20, 0x80, 0xC0, 0xFF]);

            let resultado = cliente.cerrar().await;
            assert!(resultado.is_ok(), "cierre inesperado: {resultado:?}");
            tarea_servidor.abort();
        }),
    )
    .await
    .expect("la prueba ha agotado el tiempo");
}

#[tokio::test(flavor = "current_thread")]
async fn huella_igual_no_vuelve_a_preguntar() {
    let local = tokio::task::LocalSet::new();
    tokio::time::timeout(
        Duration::from_secs(40),
        local.run_until(async {
            let huella = huella_de_una_conexion().await;

            let (lado_cliente, lado_servidor) = tokio::io::duplex(1 << 20);
            let (pantalla, _envios, _disposiciones) = Pantalla::new(
                TamanoServidor {
                    width: 800,
                    height: 600,
                },
                None,
            );
            let servidor =
                servidor_hybrid("usuario", "contrasena", Entrada::vacia(), pantalla, None);
            let tarea_servidor = lanzar_servidor(servidor, lado_servidor);

            let mut config = configuracion(false);
            config.huella_escritorio = Some(huella);
            let mut cliente = arrancar_cliente(
                lado_cliente,
                config,
                true,
                Some(credenciales("usuario", "contrasena")),
            );

            let peticion = cliente.peticion_credenciales().await;
            assert_eq!(peticion.intento, 1);
            assert!(
                cliente.dialogo_pendiente().is_none(),
                "no debe volver a preguntar por el mismo certificado"
            );
            cliente.esperar_tamano(800, 600).await;

            cliente.abortar();
            tarea_servidor.abort();
        }),
    )
    .await
    .expect("la prueba ha agotado el tiempo");
}

#[tokio::test(flavor = "current_thread")]
async fn huella_cambiada_informa_de_la_anterior() {
    let local = tokio::task::LocalSet::new();
    tokio::time::timeout(
        Duration::from_secs(40),
        local.run_until(async {
            let anterior = "SHA256:OTRA-HUELLA-ANTIGUA".to_string();

            let (lado_cliente, lado_servidor) = tokio::io::duplex(1 << 20);
            let (pantalla, _envios, _disposiciones) = Pantalla::new(
                TamanoServidor {
                    width: 800,
                    height: 600,
                },
                None,
            );
            let servidor =
                servidor_hybrid("usuario", "contrasena", Entrada::vacia(), pantalla, None);
            let tarea_servidor = lanzar_servidor(servidor, lado_servidor);

            let mut config = configuracion(false);
            config.huella_escritorio = Some(anterior.clone());
            let mut cliente = arrancar_cliente(
                lado_cliente,
                config,
                true,
                Some(credenciales("usuario", "contrasena")),
            );

            let datos = cliente.certificado().await;
            assert_eq!(datos.tipo, TipoCambioCertificado::Cambiado);
            assert_eq!(datos.anterior.as_deref(), Some(anterior.as_str()));
            assert_ne!(datos.huella, anterior);

            let peticion = cliente.peticion_credenciales().await;
            assert_eq!(peticion.intento, 1);
            cliente.esperar_tamano(800, 600).await;

            cliente.abortar();
            tarea_servidor.abort();
        }),
    )
    .await
    .expect("la prueba ha agotado el tiempo");
}

#[tokio::test(flavor = "current_thread")]
async fn certificado_cancelado_no_manda_credenciales() {
    let local = tokio::task::LocalSet::new();
    tokio::time::timeout(
        Duration::from_secs(40),
        local.run_until(async {
            let (lado_cliente, lado_servidor) = tokio::io::duplex(1 << 20);
            let (pantalla, _envios, _disposiciones) = Pantalla::new(
                TamanoServidor {
                    width: 800,
                    height: 600,
                },
                None,
            );
            let servidor =
                servidor_hybrid("usuario", "contrasena", Entrada::vacia(), pantalla, None);
            let tarea_servidor = lanzar_servidor(servidor, lado_servidor);

            let mut cliente = arrancar_cliente(
                lado_cliente,
                configuracion(false),
                false,
                Some(credenciales("usuario", "contrasena")),
            );

            let datos = cliente.certificado().await;
            assert_eq!(datos.tipo, TipoCambioCertificado::Nuevo);

            let resultado = cliente.terminar().await;
            match resultado {
                Err(ErrorEscritorio::Cancelado(motivo)) => {
                    assert_eq!(motivo, "certificado no aceptado");
                }
                otro => panic!("se esperaba Cancelado: {otro:?}"),
            }
            tarea_servidor.abort();
        }),
    )
    .await
    .expect("la prueba ha agotado el tiempo");
}

#[tokio::test(flavor = "current_thread")]
async fn credenciales_incorrectas_es_autenticacion() {
    let local = tokio::task::LocalSet::new();
    tokio::time::timeout(
        Duration::from_secs(40),
        local.run_until(async {
            let (lado_cliente, lado_servidor) = tokio::io::duplex(1 << 20);
            let (pantalla, _envios, _disposiciones) = Pantalla::new(
                TamanoServidor {
                    width: 800,
                    height: 600,
                },
                None,
            );
            let servidor =
                servidor_hybrid("usuario", "contrasena", Entrada::vacia(), pantalla, None);
            let tarea_servidor = lanzar_servidor(servidor, lado_servidor);

            let mut cliente = arrancar_cliente(
                lado_cliente,
                configuracion(false),
                true,
                Some(credenciales("usuario", "mala")),
            );

            let _datos = cliente.certificado().await;
            let peticion = cliente.peticion_credenciales().await;
            assert_eq!(peticion.intento, 1);

            match cliente.terminar().await {
                Err(ErrorEscritorio::Autenticacion(_)) => {}
                otro => panic!("se esperaba Autenticacion: {otro:?}"),
            }
            tarea_servidor.abort();
        }),
    )
    .await
    .expect("la prueba ha agotado el tiempo");
}

#[tokio::test(flavor = "current_thread")]
async fn tls_sin_nla_conecta() {
    let local = tokio::task::LocalSet::new();
    tokio::time::timeout(
        Duration::from_secs(40),
        local.run_until(async {
            let (lado_cliente, lado_servidor) = tokio::io::duplex(1 << 20);
            let (pantalla, _envios, _disposiciones) = Pantalla::new(
                TamanoServidor {
                    width: 800,
                    height: 600,
                },
                None,
            );
            let servidor = servidor_tls(Entrada::vacia(), pantalla);
            let tarea_servidor = lanzar_servidor(servidor, lado_servidor);

            let mut cliente = arrancar_cliente(
                lado_cliente,
                configuracion(false),
                true,
                Some(credenciales("usuario", "contrasena")),
            );

            let datos = cliente.certificado().await;
            assert_eq!(datos.tipo, TipoCambioCertificado::Nuevo);
            let _peticion = cliente.peticion_credenciales().await;
            cliente.esperar_tamano(800, 600).await;

            cliente.abortar();
            tarea_servidor.abort();
        }),
    )
    .await
    .expect("la prueba ha agotado el tiempo");
}

#[tokio::test(flavor = "current_thread")]
async fn servidor_sin_tls_no_admitido() {
    let local = tokio::task::LocalSet::new();
    tokio::time::timeout(
        Duration::from_secs(40),
        local.run_until(async {
            let (lado_cliente, lado_servidor) = tokio::io::duplex(1 << 20);
            let (pantalla, _envios, _disposiciones) = Pantalla::new(
                TamanoServidor {
                    width: 800,
                    height: 600,
                },
                None,
            );
            let servidor = servidor_sin_tls(Entrada::vacia(), pantalla);
            let tarea_servidor = lanzar_servidor(servidor, lado_servidor);

            let cliente = arrancar_cliente(
                lado_cliente,
                configuracion(false),
                true,
                Some(credenciales("usuario", "contrasena")),
            );

            match cliente.terminar().await {
                Err(ErrorEscritorio::Protocolo(motivo)) => {
                    assert_eq!(
                        motivo,
                        "este servidor solo habla RDP antiguo, sin TLS: no admitido"
                    );
                }
                otro => panic!("se esperaba Protocolo sin TLS: {otro:?}"),
            }
            tarea_servidor.abort();
        }),
    )
    .await
    .expect("la prueba ha agotado el tiempo");
}

#[tokio::test(flavor = "current_thread")]
async fn teclado_raton_y_ctrl_alt_supr() {
    let local = tokio::task::LocalSet::new();
    tokio::time::timeout(
        Duration::from_secs(40),
        local.run_until(async {
            let (tx_vistos, mut rx_vistos) = mpsc::unbounded_channel();
            let entrada = Entrada { vistos: tx_vistos };

            let (lado_cliente, lado_servidor) = tokio::io::duplex(1 << 20);
            let (pantalla, _envios, _disposiciones) = Pantalla::new(
                TamanoServidor {
                    width: 800,
                    height: 600,
                },
                None,
            );
            let servidor = servidor_hybrid("usuario", "contrasena", entrada, pantalla, None);
            let tarea_servidor = lanzar_servidor(servidor, lado_servidor);

            let mut cliente = arrancar_cliente(
                lado_cliente,
                configuracion(false),
                true,
                Some(credenciales("usuario", "contrasena")),
            );
            let _datos = cliente.certificado().await;
            let _peticion = cliente.peticion_credenciales().await;
            cliente.esperar_tamano(800, 600).await;

            cliente
                .enviar(OrdenEscritorio::Tecla {
                    codigo: "KeyA".to_string(),
                    keysym: 0x0061,
                    abajo: true,
                })
                .await;
            cliente
                .enviar(OrdenEscritorio::Tecla {
                    codigo: "KeyA".to_string(),
                    keysym: 0x0061,
                    abajo: false,
                })
                .await;
            cliente
                .enviar(OrdenEscritorio::Tecla {
                    codigo: "ArrowUp".to_string(),
                    keysym: 0xFF52,
                    abajo: true,
                })
                .await;
            cliente
                .enviar(OrdenEscritorio::Tecla {
                    codigo: String::new(),
                    keysym: 0x00F1,
                    abajo: true,
                })
                .await;
            cliente
                .enviar(OrdenEscritorio::Raton { x: 100, y: 200 })
                .await;
            cliente
                .enviar(OrdenEscritorio::Boton {
                    boton: 1,
                    abajo: true,
                })
                .await;
            cliente
                .enviar(OrdenEscritorio::Boton {
                    boton: 1,
                    abajo: false,
                })
                .await;
            cliente
                .enviar(OrdenEscritorio::Rueda { dx: 0.0, dy: 120.0 })
                .await;
            cliente.enviar(OrdenEscritorio::CtrlAltSupr).await;
            cliente.enviar(OrdenEscritorio::TeclaWindows).await;

            let esperados = 16;
            let mut vistos = Vec::new();
            while vistos.len() < esperados {
                let visto = tokio::time::timeout(PLAZO, rx_vistos.recv())
                    .await
                    .expect("tiempo agotado esperando entrada en el servidor")
                    .expect("canal de entrada cerrado");
                vistos.push(visto);
            }

            let contiene = |buscado: &Visto| vistos.iter().any(|visto| visto == buscado);
            assert!(contiene(&Visto::Tecla {
                codigo: 0x1E,
                extendida: false,
                abajo: true
            }));
            assert!(contiene(&Visto::Tecla {
                codigo: 0x1E,
                extendida: false,
                abajo: false
            }));
            assert!(contiene(&Visto::Tecla {
                codigo: 0x48,
                extendida: true,
                abajo: true
            }));
            assert!(contiene(&Visto::Unicode {
                codigo: 0x00F1,
                abajo: true
            }));
            assert!(contiene(&Visto::Raton { x: 100, y: 200 }));
            assert!(contiene(&Visto::Boton {
                cual: "izquierdo",
                abajo: true
            }));
            assert!(contiene(&Visto::Boton {
                cual: "izquierdo",
                abajo: false
            }));
            assert!(contiene(&Visto::Rueda(-120)));
            for (codigo, extendida) in [(0x1D, false), (0x38, false), (0x53, true)] {
                assert!(contiene(&Visto::Tecla {
                    codigo,
                    extendida,
                    abajo: true
                }));
                assert!(contiene(&Visto::Tecla {
                    codigo,
                    extendida,
                    abajo: false
                }));
            }
            assert!(contiene(&Visto::Tecla {
                codigo: 0x5B,
                extendida: true,
                abajo: true
            }));
            assert!(contiene(&Visto::Tecla {
                codigo: 0x5B,
                extendida: true,
                abajo: false
            }));

            cliente.abortar();
            tarea_servidor.abort();
        }),
    )
    .await
    .expect("la prueba ha agotado el tiempo");
}

#[tokio::test(flavor = "current_thread")]
async fn portapapeles_cliprdr_en_los_dos_sentidos() {
    let local = tokio::task::LocalSet::new();
    tokio::time::timeout(
        Duration::from_secs(40),
        local.run_until(async {
            let sede = Arc::new(Mutex::new(None));
            let (tx_avisos, mut rx_avisos) = mpsc::unbounded_channel();
            let fabrica = FabricaPortapapeles {
                eventos: Arc::clone(&sede),
                avisos: tx_avisos,
            };

            let (lado_cliente, lado_servidor) = tokio::io::duplex(1 << 20);
            let (pantalla, _envios, _disposiciones) = Pantalla::new(
                TamanoServidor {
                    width: 800,
                    height: 600,
                },
                None,
            );
            let servidor = servidor_hybrid(
                "usuario",
                "contrasena",
                Entrada::vacia(),
                pantalla,
                Some(Box::new(fabrica)),
            );
            let tarea_servidor = lanzar_servidor(servidor, lado_servidor);

            let mut cliente = arrancar_cliente(
                lado_cliente,
                configuracion(false),
                true,
                Some(credenciales("usuario", "contrasena")),
            );
            let _datos = cliente.certificado().await;
            let _peticion = cliente.peticion_credenciales().await;
            cliente.esperar_tamano(800, 600).await;

            // El remoto tiene texto nuevo: el cliente lo pide y lo anuncia.
            let enviar = {
                let sede = Arc::clone(&sede);
                move |mensaje: ClipboardMessage| {
                    let guard = sede.lock().expect("cerrojo de eventos del servidor");
                    let tx = guard
                        .as_ref()
                        .expect("el servidor no ha fijado su canal de eventos");
                    tx.send(ServerEvent::Clipboard(mensaje))
                        .expect("canal de eventos del servidor");
                }
            };

            enviar(ClipboardMessage::SendInitiateCopy(vec![
                ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT),
            ]));
            assert_eq!(esperar_portapapeles(&mut cliente).await, "texto remoto");

            // El visor pega texto: el cliente lo anuncia al remoto.
            cliente
                .enviar(OrdenEscritorio::Portapapeles("texto local".to_string()))
                .await;
            // El cliente anunció una lista vacía al inicializar el canal: se salta
            // hasta la lista con texto que manda al copiar el visor.
            let mut anunciado = false;
            for _ in 0..10 {
                match esperar_aviso(&mut rx_avisos).await {
                    AvisoPortapapeles::FormatosRemotos(formatos) => {
                        if formatos.contains(&ClipboardFormatId::CF_UNICODETEXT) {
                            anunciado = true;
                            break;
                        }
                    }
                    otro => panic!("aviso inesperado: {otro:?}"),
                }
            }
            assert!(anunciado, "el cliente debe anunciar CF_UNICODETEXT");

            // El remoto pide los datos y el cliente responde con el texto local.
            enviar(ClipboardMessage::SendInitiatePaste(
                ClipboardFormatId::CF_UNICODETEXT,
            ));
            match esperar_aviso(&mut rx_avisos).await {
                AvisoPortapapeles::DatosRemotos(texto) => {
                    assert_eq!(texto, "texto local");
                }
                otro => panic!("aviso inesperado: {otro:?}"),
            }

            cliente.abortar();
            tarea_servidor.abort();
        }),
    )
    .await
    .expect("la prueba ha agotado el tiempo");
}

#[tokio::test(flavor = "current_thread")]
async fn display_control_redimensiona_y_reactiva() {
    let local = tokio::task::LocalSet::new();
    tokio::time::timeout(
        Duration::from_secs(60),
        local.run_until(async {
            let (lado_cliente, lado_servidor) = tokio::io::duplex(1 << 20);
            let (pantalla, _envios, mut rx_disposiciones) = Pantalla::new(
                TamanoServidor {
                    width: 800,
                    height: 600,
                },
                None,
            );
            let servidor =
                servidor_hybrid("usuario", "contrasena", Entrada::vacia(), pantalla, None);
            let tarea_servidor = lanzar_servidor(servidor, lado_servidor);

            let mut cliente = arrancar_cliente(
                lado_cliente,
                configuracion(false),
                true,
                Some(credenciales("usuario", "contrasena")),
            );
            let _datos = cliente.certificado().await;
            let _peticion = cliente.peticion_credenciales().await;
            cliente.esperar_tamano(800, 600).await;

            // Display Control no está disponible hasta que el servidor abre el
            // canal: se reintenta el cambio hasta que llega la petición.
            let plazo = tokio::time::Instant::now() + Duration::from_secs(10);
            loop {
                cliente
                    .enviar(OrdenEscritorio::Tamano {
                        ancho: 1024,
                        alto: 768,
                    })
                    .await;
                match tokio::time::timeout(Duration::from_millis(250), rx_disposiciones.recv())
                    .await
                {
                    Ok(Some(())) => break,
                    Ok(None) => panic!("el canal de disposiciones se ha cerrado"),
                    Err(_) => {
                        if tokio::time::Instant::now() > plazo {
                            panic!("el servidor no ha recibido la petición de tamaño");
                        }
                    }
                }
            }

            // La reactivación la conduce el cliente al recibir DeactivateAll.
            cliente.esperar_tamano(1024, 768).await;

            cliente.abortar();
            tarea_servidor.abort();
        }),
    )
    .await
    .expect("la prueba ha agotado el tiempo");
}

#[tokio::test(flavor = "current_thread")]
async fn cursor_remoto_se_manda_al_puente() {
    let local = tokio::task::LocalSet::new();
    tokio::time::timeout(
        Duration::from_secs(40),
        local.run_until(async {
            let (lado_cliente, lado_servidor) = tokio::io::duplex(1 << 20);
            let (pantalla, envios, _disposiciones) = Pantalla::new(
                TamanoServidor {
                    width: 800,
                    height: 600,
                },
                None,
            );
            let servidor =
                servidor_hybrid("usuario", "contrasena", Entrada::vacia(), pantalla, None);
            let tarea_servidor = lanzar_servidor(servidor, lado_servidor);

            let mut cliente = arrancar_cliente(
                lado_cliente,
                configuracion(false),
                true,
                Some(credenciales("usuario", "contrasena")),
            );
            let _datos = cliente.certificado().await;
            let _peticion = cliente.peticion_credenciales().await;
            cliente.esperar_tamano(800, 600).await;

            let pixeles = vec![0xFF_u8; 16 * 16 * 4];
            enviar_actualizacion(
                &envios,
                DisplayUpdate::RGBAPointer(ironrdp_server::RGBAPointer {
                    cache_index: 1,
                    width: 16,
                    height: 16,
                    hot_x: 2,
                    hot_y: 3,
                    data: pixeles,
                }),
            );

            let mut visto = None;
            for _ in 0..40 {
                if let EventoEscritorio::Cursor {
                    visible,
                    punto,
                    ancho,
                    alto,
                    pixeles,
                    ..
                } = cliente.siguiente().await
                {
                    visto = Some((visible, punto, ancho, alto, pixeles));
                    break;
                }
            }
            let (visible, punto, ancho, alto, pixeles) = visto.expect("no ha llegado el cursor");
            assert!(visible);
            assert_eq!(ancho, 16);
            assert_eq!(alto, 16);
            assert_eq!(punto, (2, 3));
            assert_eq!(pixeles.len(), 16 * 16 * 4);

            // NOTA: ironrdp-server 0.13 no manda las actualizaciones de puntero sin
            // datos (HidePointer y DefaultPointer): su fragmentador no escribe un
            // PDU vacío. El camino de ocultar y el del cursor por defecto quedan
            // cubiertos por el código, pero no se pueden probar con este servidor.

            cliente.abortar();
            tarea_servidor.abort();
        }),
    )
    .await
    .expect("la prueba ha agotado el tiempo");
}

#[tokio::test(flavor = "current_thread")]
async fn con_h264_se_anuncia_video() {
    let local = tokio::task::LocalSet::new();
    tokio::time::timeout(
        Duration::from_secs(40),
        local.run_until(async {
            let (lado_cliente, lado_servidor) = tokio::io::duplex(1 << 20);
            let (pantalla, _envios, _disposiciones) = Pantalla::new(
                TamanoServidor {
                    width: 800,
                    height: 600,
                },
                None,
            );
            let servidor =
                servidor_hybrid("usuario", "contrasena", Entrada::vacia(), pantalla, None);
            let tarea_servidor = lanzar_servidor(servidor, lado_servidor);

            let mut cliente = arrancar_cliente(
                lado_cliente,
                configuracion(true),
                true,
                Some(credenciales("usuario", "contrasena")),
            );
            let _datos = cliente.certificado().await;
            let _peticion = cliente.peticion_credenciales().await;

            match cliente.siguiente().await {
                EventoEscritorio::Abierto => {}
                otro => panic!("se esperaba Abierto: {otro:?}"),
            }
            match cliente.siguiente().await {
                EventoEscritorio::Imagen(ImagenEscritorio::H264) => {}
                otro => panic!("se esperaba Imagen(H264): {otro:?}"),
            }

            cliente.abortar();
            tarea_servidor.abort();
        }),
    )
    .await
    .expect("la prueba ha agotado el tiempo");
}
