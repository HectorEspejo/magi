//! Pruebas del cliente RFB (pista VNC de la Fase 12) contra un servidor RFB de
//! prueba en proceso, en 127.0.0.1:0 y sin red externa.
//!
//! El servidor de prueba implementa el saludo de las versiones 3.3, 3.7 y 3.8,
//! las seguridades None y VNC Authentication, el manual de servidor y las
//! actualizaciones programadas (Raw, CopyRect, Hextile, ZRLE, DesktopSize,
//! ExtendedDesktopSize y Cursor), y comprueba byte a byte lo que manda el
//! cliente (formato de píxel, codificaciones, peticiones, teclas, ratón,
//! SetDesktopSize y portapapeles).

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use magi_nucleo::escritorio::{
    rfb, Conexion, ConfigEscritorio, CredencialesEscritorio, DatosCertificado, DialogosEscritorio,
    ErrorEscritorio, EventoEscritorio, ImagenEscritorio, OpcionesEscritorio, OrdenEscritorio,
    PeticionCredenciales, TipoHost,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use zeroize::Zeroizing;

/// Plazo de las esperas en las pruebas.
const PLAZO: Duration = Duration::from_secs(5);
/// Tamaño del escritorio que anuncia el servidor de prueba por defecto.
const ANCHO: u16 = 100;
/// Alto del escritorio que anuncia el servidor de prueba por defecto.
const ALTO: u16 = 60;

// --------------------------------------------------------- dobles del cliente

/// Diálogo de prueba: entrega las contraseñas en cola y apunta los intentos.
struct DialogoPrueba {
    contrasenas: Mutex<std::collections::VecDeque<Option<String>>>,
    intentos: Mutex<Vec<u8>>,
}

impl DialogoPrueba {
    fn nuevo(contrasenas: &[Option<&str>]) -> Arc<Self> {
        Arc::new(Self {
            contrasenas: Mutex::new(
                contrasenas
                    .iter()
                    .map(|contrasena| contrasena.map(str::to_string))
                    .collect(),
            ),
            intentos: Mutex::new(Vec::new()),
        })
    }

    fn intentos(&self) -> Vec<u8> {
        self.intentos.lock().expect("cerrojo de intentos").clone()
    }
}

#[async_trait]
impl DialogosEscritorio for DialogoPrueba {
    async fn certificado(&self, _datos: DatosCertificado) -> Result<bool, ErrorEscritorio> {
        Ok(true)
    }

    async fn credenciales(
        &self,
        peticion: PeticionCredenciales,
    ) -> Result<Option<CredencialesEscritorio>, ErrorEscritorio> {
        self.intentos
            .lock()
            .expect("cerrojo de intentos")
            .push(peticion.intento);
        let contrasena = self
            .contrasenas
            .lock()
            .expect("cerrojo de contraseñas")
            .pop_front()
            .unwrap_or(None);
        Ok(contrasena.map(|texto| CredencialesEscritorio {
            usuario: String::new(),
            dominio: None,
            contrasena: Zeroizing::new(texto),
            recordar: false,
        }))
    }
}

fn configuracion(portapapeles: bool) -> ConfigEscritorio {
    ConfigEscritorio {
        tipo: TipoHost::Vnc,
        direccion: "127.0.0.1".to_string(),
        puerto: 5900,
        usuario: None,
        dominio: None,
        opciones: OpcionesEscritorio::por_defecto(TipoHost::Vnc),
        distribucion_teclado: "es-ES".to_string(),
        h264: false,
        tamano_inicial: (1024, 768),
        portapapeles,
        huella_escritorio: None,
    }
}

/// Cliente RFB corriendo en una tarea con los canales de eventos y órdenes.
struct ClientePrueba {
    eventos: mpsc::Receiver<EventoEscritorio>,
    ordenes: mpsc::Sender<OrdenEscritorio>,
    tarea: tokio::task::JoinHandle<Result<(), ErrorEscritorio>>,
}

impl ClientePrueba {
    fn lanzar(direccion: SocketAddr, dialogo: Arc<DialogoPrueba>, intento: u8) -> Self {
        Self::lanzar_con(direccion, dialogo, intento, true)
    }

    fn lanzar_con(
        direccion: SocketAddr,
        dialogo: Arc<DialogoPrueba>,
        intento: u8,
        portapapeles: bool,
    ) -> Self {
        let (eventos_tx, eventos) = mpsc::channel(1024);
        let (ordenes, ordenes_rx) = mpsc::channel(64);
        let tarea = tokio::spawn(async move {
            let flujo = match TcpStream::connect(direccion).await {
                Ok(flujo) => {
                    let _ = flujo.set_nodelay(true);
                    flujo
                }
                Err(error) => {
                    return Err(ErrorEscritorio::Conexion(format!(
                        "no se pudo conectar al servidor de prueba: {error}"
                    )));
                }
            };
            let conexion = Conexion {
                flujo: Box::new(flujo),
                config: configuracion(portapapeles),
                dialogo,
                eventos: eventos_tx,
                ordenes: ordenes_rx,
                intento,
            };
            rfb::abrir(conexion).await
        });
        Self {
            eventos,
            ordenes,
            tarea,
        }
    }

    /// Siguiente evento, con plazo (nunca colgar la prueba).
    async fn evento(&mut self) -> EventoEscritorio {
        match tokio::time::timeout(PLAZO, self.eventos.recv()).await {
            Ok(Some(evento)) => evento,
            Ok(None) => panic!("el canal de eventos se cerró antes de tiempo"),
            Err(_) => panic!("plazo esperando un evento del cliente RFB"),
        }
    }

    /// Espera Abierto, Imagen(Rectangulos) y Tamano inicial, y devuelve el
    /// tamaño que anuncia el servidor.
    async fn listo(&mut self) -> (u16, u16) {
        match self.evento().await {
            EventoEscritorio::Abierto => {}
            otro => panic!("se esperaba Abierto y llegó {otro:?}"),
        }
        match self.evento().await {
            EventoEscritorio::Imagen(ImagenEscritorio::Rectangulos) => {}
            otro => panic!("se esperaba Imagen(Rectangulos) y llegó {otro:?}"),
        }
        match self.evento().await {
            EventoEscritorio::Tamano { ancho, alto } => (ancho, alto),
            otro => panic!("se esperaba el Tamano inicial y llegó {otro:?}"),
        }
    }

    async fn rect(&mut self) -> (u16, u16, u16, u16, Vec<u8>) {
        match self.evento().await {
            EventoEscritorio::Rect {
                x,
                y,
                ancho,
                alto,
                pixeles,
            } => (x, y, ancho, alto, pixeles),
            otro => panic!("se esperaba Rect y llegó {otro:?}"),
        }
    }

    fn ordenar(&self, orden: OrdenEscritorio) {
        self.ordenes
            .try_send(orden)
            .expect("el canal de órdenes del cliente RFB está lleno o cerrado");
    }

    /// Espera Caido(motivo) y Fin, y devuelve el error con el que terminó.
    async fn fallo(mut self, fragmento: &str) -> ErrorEscritorio {
        let motivo = loop {
            match self.evento().await {
                EventoEscritorio::Caido(motivo) => break motivo,
                EventoEscritorio::Abierto
                | EventoEscritorio::Imagen(_)
                | EventoEscritorio::Tamano { .. } => continue,
                otro => panic!("se esperaba Caido y llegó {otro:?}"),
            }
        };
        assert!(
            motivo.contains("datos inválidos del servidor"),
            "motivo sin el prefijo esperado: {motivo}"
        );
        assert!(
            motivo.contains(fragmento),
            "el motivo «{motivo}» no contiene «{fragmento}»"
        );
        match self.evento().await {
            EventoEscritorio::Fin => {}
            otro => panic!("se esperaba Fin y llegó {otro:?}"),
        }
        match self.terminar().await {
            Err(error) => error,
            Ok(()) => panic!("el cliente terminó bien y se esperaba un error con «{fragmento}»"),
        }
    }

    /// Espera el fin limpio (Cerrado y Fin) y devuelve el resultado de abrir.
    async fn fin_limpio(mut self) -> Result<(), ErrorEscritorio> {
        loop {
            match self.evento().await {
                EventoEscritorio::Cerrado(_) => break,
                EventoEscritorio::Abierto
                | EventoEscritorio::Imagen(_)
                | EventoEscritorio::Tamano { .. } => continue,
                otro => panic!("se esperaba Cerrado y llegó {otro:?}"),
            }
        }
        match self.evento().await {
            EventoEscritorio::Fin => {}
            otro => panic!("se esperaba Fin y llegó {otro:?}"),
        }
        self.terminar().await
    }

    async fn terminar(self) -> Result<(), ErrorEscritorio> {
        match tokio::time::timeout(PLAZO, self.tarea).await {
            Ok(Ok(resultado)) => resultado,
            Ok(Err(error)) => panic!("la tarea del cliente RFB entró en pánico: {error}"),
            Err(_) => panic!("plazo esperando el fin de la tarea del cliente RFB"),
        }
    }
}

// --------------------------------------------------------- servidor de prueba

/// Versión que anuncia el servidor de prueba.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VersionPrueba {
    V33,
    V37,
    V38,
}

impl VersionPrueba {
    fn saludo(self) -> &'static [u8; 12] {
        match self {
            VersionPrueba::V33 => b"RFB 003.003\n",
            VersionPrueba::V37 => b"RFB 003.007\n",
            VersionPrueba::V38 => b"RFB 003.008\n",
        }
    }
}

/// Seguridad que ofrece el servidor de prueba.
#[derive(Debug, Clone)]
enum SeguridadPrueba {
    Ninguna,
    Vnc { contrasena: &'static str },
}

/// Mensajes que el cliente RFB manda al servidor.
#[derive(Debug, Clone)]
enum MensajeCliente {
    FormatoPixel(Vec<u8>),
    Codificaciones(Vec<i32>),
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
    TeclaQemu {
        abajo: bool,
        keysym: u32,
        keycode: u32,
    },
    Raton {
        botones: u8,
        x: u16,
        y: u16,
    },
    Corte {
        largo: i32,
        datos: Vec<u8>,
    },
    CambiarTamano {
        ancho: u16,
        alto: u16,
        pantallas: Vec<Pantalla>,
    },
    Otro(#[allow(dead_code)] u8),
}

#[derive(Debug, Clone, Copy)]
struct Pantalla {
    id: u32,
    x: u16,
    y: u16,
    ancho: u16,
    alto: u16,
    banderas: u32,
}

struct ServidorPrueba {
    lector: OwnedReadHalf,
    escritor: OwnedWriteHalf,
    guardados: std::collections::VecDeque<MensajeCliente>,
}

impl ServidorPrueba {
    fn nuevo(flujo: TcpStream) -> Self {
        let _ = flujo.set_nodelay(true);
        let (lector, escritor) = flujo.into_split();
        Self {
            lector,
            escritor,
            guardados: std::collections::VecDeque::new(),
        }
    }

    async fn leer(&mut self, cuantos: usize) -> Result<Vec<u8>, String> {
        let mut datos = vec![0u8; cuantos];
        match tokio::time::timeout(PLAZO, self.lector.read_exact(&mut datos)).await {
            Ok(Ok(_)) => Ok(datos),
            Ok(Err(error)) => Err(format!("el cliente cerró la conexión: {error}")),
            Err(_) => Err("plazo esperando bytes del cliente".to_string()),
        }
    }

    async fn enviar(&mut self, datos: &[u8]) -> Result<(), String> {
        match tokio::time::timeout(PLAZO, self.escritor.write_all(datos)).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(format!("no se pudo escribir al cliente: {error}")),
            Err(_) => Err("plazo escribiendo al cliente".to_string()),
        }
    }

    async fn enviar_actualizacion(&mut self, rectangulos: &[Vec<u8>]) -> Result<(), String> {
        self.enviar(&mensaje_actualizacion(rectangulos)).await
    }

    /// Saludo, negociación de seguridad y, si se acepta, espera al ClientInit.
    /// Devuelve true si la autenticación fue buena (o no hacía falta).
    async fn saludar(
        &mut self,
        version: VersionPrueba,
        seguridad: &SeguridadPrueba,
    ) -> Result<bool, String> {
        let saludo = version.saludo();
        self.enviar(saludo).await?;
        let respuesta = self.leer(12).await?;
        if respuesta != *saludo {
            return Err(format!(
                "el cliente respondió al saludo con {:?}",
                String::from_utf8_lossy(&respuesta)
            ));
        }
        match seguridad {
            SeguridadPrueba::Ninguna => {
                if version == VersionPrueba::V33 {
                    self.enviar(&1u32.to_be_bytes()).await?;
                } else {
                    self.enviar(&[1, 1]).await?;
                    let elegida = self.leer(1).await?;
                    if elegida != [1] {
                        return Err(format!("el cliente eligió la seguridad {elegida:?}"));
                    }
                }
                if version == VersionPrueba::V38 {
                    self.enviar(&0u32.to_be_bytes()).await?;
                }
                Ok(true)
            }
            SeguridadPrueba::Vnc { contrasena } => {
                if version == VersionPrueba::V33 {
                    self.enviar(&2u32.to_be_bytes()).await?;
                } else {
                    self.enviar(&[1, 2]).await?;
                    let elegida = self.leer(1).await?;
                    if elegida != [2] {
                        return Err(format!("el cliente eligió la seguridad {elegida:?}"));
                    }
                }
                let reto = reto_de_prueba();
                self.enviar(&reto).await?;
                let respuesta = self.leer(16).await?;
                let esperada = respuesta_vnc_referencia(&reto, contrasena);
                if respuesta == esperada {
                    self.enviar(&0u32.to_be_bytes()).await?;
                    Ok(true)
                } else {
                    self.enviar(&1u32.to_be_bytes()).await?;
                    if version == VersionPrueba::V38 {
                        let motivo = b"Authentication failed";
                        self.enviar(&(motivo.len() as u32).to_be_bytes()).await?;
                        self.enviar(motivo).await?;
                    }
                    Ok(false)
                }
            }
        }
    }

    /// ClientInit (shared-flag=1), ServerInit, SetPixelFormat y SetEncodings.
    async fn inicializar(&mut self, ancho: u16, alto: u16) -> Result<(), String> {
        let client_init = self.leer(1).await?;
        if client_init != [1] {
            return Err(format!(
                "ClientInit con shared-flag distinto de 1: {client_init:?}"
            ));
        }
        let mut init = Vec::new();
        init.extend_from_slice(&ancho.to_be_bytes());
        init.extend_from_slice(&alto.to_be_bytes());
        init.extend_from_slice(&formato_natural());
        let nombre = b"servidor de prueba";
        init.extend_from_slice(&(nombre.len() as u32).to_be_bytes());
        init.extend_from_slice(nombre);
        self.enviar(&init).await?;
        match self.leer_mensaje().await? {
            MensajeCliente::FormatoPixel(formato) => {
                let esperado = [32u8, 24, 0, 1, 0, 255, 0, 255, 0, 255, 0, 8, 16, 0, 0, 0];
                if formato != esperado {
                    return Err(format!("formato de píxel inesperado: {formato:?}"));
                }
            }
            otro => return Err(format!("se esperaba SetPixelFormat y llegó {otro:?}")),
        }
        match self.leer_mensaje().await? {
            MensajeCliente::Codificaciones(lista) => {
                for esperada in [16, 5, 1, 0, -223, -308, -239, -258, 0xC0A1E5CEu32 as i32] {
                    if !lista.contains(&esperada) {
                        return Err(format!(
                            "SetEncodings no pide la codificación {esperada}: {lista:?}"
                        ));
                    }
                }
            }
            otro => return Err(format!("se esperaba SetEncodings y llegó {otro:?}")),
        }
        Ok(())
    }

    async fn leer_mensaje(&mut self) -> Result<MensajeCliente, String> {
        if let Some(mensaje) = self.guardados.pop_front() {
            return Ok(mensaje);
        }
        let tipo = self.leer(1).await?[0];
        match tipo {
            0 => {
                let resto = self.leer(19).await?;
                Ok(MensajeCliente::FormatoPixel(resto[3..].to_vec()))
            }
            2 => {
                let cabecera = self.leer(3).await?;
                let cuantas = u16::from_be_bytes([cabecera[1], cabecera[2]]) as usize;
                let datos = self.leer(cuantas * 4).await?;
                let mut lista = Vec::with_capacity(cuantas);
                for trozo in datos.chunks_exact(4) {
                    lista.push(i32::from_be_bytes([trozo[0], trozo[1], trozo[2], trozo[3]]));
                }
                Ok(MensajeCliente::Codificaciones(lista))
            }
            3 => {
                let resto = self.leer(9).await?;
                Ok(MensajeCliente::Peticion {
                    incremental: resto[0] != 0,
                    x: u16::from_be_bytes([resto[1], resto[2]]),
                    y: u16::from_be_bytes([resto[3], resto[4]]),
                    ancho: u16::from_be_bytes([resto[5], resto[6]]),
                    alto: u16::from_be_bytes([resto[7], resto[8]]),
                })
            }
            4 => {
                let resto = self.leer(7).await?;
                Ok(MensajeCliente::Tecla {
                    abajo: resto[0] != 0,
                    keysym: u32::from_be_bytes([resto[3], resto[4], resto[5], resto[6]]),
                })
            }
            5 => {
                let resto = self.leer(5).await?;
                Ok(MensajeCliente::Raton {
                    botones: resto[0],
                    x: u16::from_be_bytes([resto[1], resto[2]]),
                    y: u16::from_be_bytes([resto[3], resto[4]]),
                })
            }
            6 => {
                let resto = self.leer(7).await?;
                let largo = i32::from_be_bytes([resto[3], resto[4], resto[5], resto[6]]);
                let cuantos = largo.unsigned_abs() as usize;
                let datos = self.leer(cuantos).await?;
                Ok(MensajeCliente::Corte { largo, datos })
            }
            251 => {
                let resto = self.leer(7).await?;
                let pantallas = resto[5];
                let datos = self.leer(usize::from(pantallas) * 16).await?;
                let mut lista = Vec::with_capacity(usize::from(pantallas));
                for trozo in datos.chunks_exact(16) {
                    lista.push(Pantalla {
                        id: u32::from_be_bytes([trozo[0], trozo[1], trozo[2], trozo[3]]),
                        x: u16::from_be_bytes([trozo[4], trozo[5]]),
                        y: u16::from_be_bytes([trozo[6], trozo[7]]),
                        ancho: u16::from_be_bytes([trozo[8], trozo[9]]),
                        alto: u16::from_be_bytes([trozo[10], trozo[11]]),
                        banderas: u32::from_be_bytes([trozo[12], trozo[13], trozo[14], trozo[15]]),
                    });
                }
                Ok(MensajeCliente::CambiarTamano {
                    ancho: u16::from_be_bytes([resto[1], resto[2]]),
                    alto: u16::from_be_bytes([resto[3], resto[4]]),
                    pantallas: lista,
                })
            }
            255 => {
                let resto = self.leer(11).await?;
                if resto[0] != 0 {
                    return Ok(MensajeCliente::Otro(tipo));
                }
                Ok(MensajeCliente::TeclaQemu {
                    abajo: u16::from_be_bytes([resto[1], resto[2]]) != 0,
                    keysym: u32::from_be_bytes([resto[3], resto[4], resto[5], resto[6]]),
                    keycode: u32::from_be_bytes([resto[7], resto[8], resto[9], resto[10]]),
                })
            }
            otro => Ok(MensajeCliente::Otro(otro)),
        }
    }

    /// Espera a que llegue un mensaje que cumpla el predicado, guardando los
    /// que no (para comprobarlos después).
    async fn esperar(
        &mut self,
        mut cumple: impl FnMut(&MensajeCliente) -> bool,
        descripcion: &str,
    ) -> Result<MensajeCliente, String> {
        for _ in 0..64 {
            let mensaje = self.leer_mensaje().await?;
            if cumple(&mensaje) {
                return Ok(mensaje);
            }
            self.guardados.push_back(mensaje);
        }
        Err(format!("no llegó el mensaje esperado: {descripcion}"))
    }

    /// Comprueba que el cliente no manda nada en el plazo indicado.
    async fn silencio(&mut self, plazo: Duration) -> Result<(), String> {
        if let Some(mensaje) = self.guardados.pop_front() {
            return Err(format!("llegó un mensaje inesperado: {mensaje:?}"));
        }
        match tokio::time::timeout(plazo, self.leer_mensaje()).await {
            Err(_) => Ok(()),
            Ok(Ok(mensaje)) => Err(format!("llegó un mensaje inesperado: {mensaje:?}")),
            Ok(Err(error)) => Err(error),
        }
    }
}

// --------------------------------------------------------------- utilidades

fn formato_natural() -> [u8; 16] {
    [32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 0, 8, 16, 0, 0, 0]
}

fn reto_de_prueba() -> [u8; 16] {
    [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
        0x0f,
    ]
}

/// Implementación de referencia de VNC Authentication para el servidor de
/// prueba: invierte los bits de cada byte de la contraseña y cifra los dos
/// bloques del reto con DES ECB.
fn respuesta_vnc_referencia(reto: &[u8; 16], contrasena: &str) -> [u8; 16] {
    use des::cipher::{Block, BlockCipherEncrypt, KeyInit};
    let mut clave = [0u8; 8];
    for (posicion, byte) in contrasena.as_bytes().iter().take(8).enumerate() {
        clave[posicion] = byte.reverse_bits();
    }
    let cifrador = des::Des::new_from_slice(&clave).expect("clave DES de 8 bytes");
    let mut salida = [0u8; 16];
    for bloque in 0..2 {
        let mut datos = [0u8; 8];
        datos.copy_from_slice(&reto[bloque * 8..bloque * 8 + 8]);
        let mut cifrado = Block::<des::Des>::from(datos);
        cifrador.encrypt_block(&mut cifrado);
        salida[bloque * 8..bloque * 8 + 8].copy_from_slice(&cifrado[..]);
    }
    salida
}

fn texto_hex(bytes: &[u8]) -> String {
    let mut salida = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        salida.push_str(&format!("{byte:02x}"));
    }
    salida
}

/// Cabecera de rectángulo más sus datos.
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

fn mensaje_actualizacion(rectangulos: &[Vec<u8>]) -> Vec<u8> {
    let mut mensaje = vec![0u8, 0];
    mensaje.extend_from_slice(&(rectangulos.len() as u16).to_be_bytes());
    for rectangulo in rectangulos {
        mensaje.extend_from_slice(rectangulo);
    }
    mensaje
}

/// Mensaje ServerCutText clásico (texto Latin-1).
fn corte_servidor_basico(texto: &[u8]) -> Vec<u8> {
    let mut mensaje = vec![3u8, 0, 0, 0];
    mensaje.extend_from_slice(&(texto.len() as u32).to_be_bytes());
    mensaje.extend_from_slice(texto);
    mensaje
}

/// Mensaje ServerCutText extendido: cabecera de banderas y cuerpo.
fn corte_servidor_extendido(banderas: u32, cuerpo: &[u8]) -> Vec<u8> {
    let total = 4 + cuerpo.len();
    let mut mensaje = vec![3u8, 0, 0, 0];
    mensaje.extend_from_slice(&(-(total as i32)).to_be_bytes());
    mensaje.extend_from_slice(&banderas.to_be_bytes());
    mensaje.extend_from_slice(cuerpo);
    mensaje
}

const CORTE_TEXTO: u32 = 0x0000_0001;
const CORTE_CAPS: u32 = 0x0100_0000;
const CORTE_PETICION: u32 = 0x0200_0000;
const CORTE_PROVISION: u32 = 0x1000_0000;

/// Compresor zlib que mantiene el flujo entre rectángulos ZRLE.
struct CompresorZrle {
    codificador: ZlibEncoder<Vec<u8>>,
    anterior: usize,
}

impl CompresorZrle {
    fn nuevo() -> Self {
        Self {
            codificador: ZlibEncoder::new(Vec::new(), Compression::default()),
            anterior: 0,
        }
    }

    fn comprimir(&mut self, datos: &[u8]) -> Vec<u8> {
        use std::io::Write;
        self.codificador
            .write_all(datos)
            .expect("escribir en el zlib de prueba");
        self.codificador.flush().expect("vaciar el zlib de prueba");
        let todos = self.codificador.get_mut();
        let trozo = todos[self.anterior..].to_vec();
        self.anterior = todos.len();
        trozo
    }
}

fn rectangulo_zrle(
    x: u16,
    y: u16,
    ancho: u16,
    alto: u16,
    compresor: &mut CompresorZrle,
    datos: &[u8],
) -> Vec<u8> {
    let comprimido = compresor.comprimir(datos);
    let mut cuerpo = (comprimido.len() as u32).to_be_bytes().to_vec();
    cuerpo.extend_from_slice(&comprimido);
    rectangulo(16, x, y, ancho, alto, &cuerpo)
}

/// Abre un escenario completo: servidor a la escucha, cliente y manual.
struct Escenario {
    cliente: ClientePrueba,
    servidor: ServidorPrueba,
    dialogo: Arc<DialogoPrueba>,
}

async fn nuevo_escenario(
    version: VersionPrueba,
    seguridad: SeguridadPrueba,
    ancho: u16,
    alto: u16,
    contrasenas: &[Option<&str>],
    intento: u8,
) -> Escenario {
    nuevo_escenario_portapapeles(version, seguridad, ancho, alto, contrasenas, intento, true).await
}

async fn nuevo_escenario_portapapeles(
    version: VersionPrueba,
    seguridad: SeguridadPrueba,
    ancho: u16,
    alto: u16,
    contrasenas: &[Option<&str>],
    intento: u8,
    portapapeles: bool,
) -> Escenario {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("abrir el servidor de prueba");
    let direccion = listener
        .local_addr()
        .expect("dirección del servidor de prueba");
    let dialogo = DialogoPrueba::nuevo(contrasenas);
    let cliente = ClientePrueba::lanzar_con(direccion, dialogo.clone(), intento, portapapeles);
    let (flujo, _) = tokio::time::timeout(PLAZO, listener.accept())
        .await
        .expect("plazo aceptando al cliente")
        .expect("aceptar al cliente");
    let mut servidor = ServidorPrueba::nuevo(flujo);
    let autenticado = servidor
        .saludar(version, &seguridad)
        .await
        .expect("saludo del servidor de prueba");
    if autenticado {
        servidor
            .inicializar(ancho, alto)
            .await
            .expect("inicialización del servidor de prueba");
    }
    Escenario {
        cliente,
        servidor,
        dialogo,
    }
}

/// Lee la petición de actualización inicial (completa).
async fn peticion_inicial(servidor: &mut ServidorPrueba, ancho: u16, alto: u16) {
    match servidor.leer_mensaje().await.expect("petición inicial") {
        MensajeCliente::Peticion {
            incremental,
            x,
            y,
            ancho: ancho_pedido,
            alto: alto_pedido,
        } => {
            assert!(!incremental, "la primera actualización debe ser completa");
            assert_eq!(
                (x, y, ancho_pedido, alto_pedido),
                (0, 0, ancho, alto),
                "la petición inicial debe cubrir el escritorio entero"
            );
        }
        otro => panic!("se esperaba la petición inicial y llegó {otro:?}"),
    }
}

// ------------------------------------------------------------------- pruebas

#[test]
fn vector_conocido_de_des_con_bits_invertidos() {
    // Vector calculado con una implementación independiente (OpenSSL 3.x,
    // DES-ECB): contraseña "password", clave con los bits de cada byte
    // invertidos y reto 00 01 … 0f.
    let reto = reto_de_prueba();
    let respuesta = respuesta_vnc_referencia(&reto, "password");
    assert_eq!(
        texto_hex(&respuesta),
        "b866924125c8eebb9debc1db61c538e2",
        "el vector conocido de VNC Authentication no coincide"
    );
    // La contraseña larga se recorta a 8 bytes.
    let recortada = respuesta_vnc_referencia(&reto, "password123");
    assert_eq!(
        recortada, respuesta,
        "la contraseña debe truncarse a 8 bytes"
    );
}

#[tokio::test]
async fn versiones_3_3_3_7_y_3_8_con_seguridad_none() {
    for version in [VersionPrueba::V33, VersionPrueba::V37, VersionPrueba::V38] {
        let mut escenario = nuevo_escenario(version, SeguridadPrueba::Ninguna, 4, 2, &[], 1).await;
        peticion_inicial(&mut escenario.servidor, 4, 2).await;
        assert_eq!(escenario.cliente.listo().await, (4, 2));
        let crudos = [
            10u8, 20, 30, 0, 40, 50, 60, 0, 70, 80, 90, 0, 1, 2, 3, 0, 4, 5, 6, 0, 7, 8, 9, 0, 10,
            11, 12, 0, 13, 14, 15, 0,
        ];
        escenario
            .servidor
            .enviar_actualizacion(&[rectangulo(0, 0, 0, 4, 2, &crudos)])
            .await
            .expect("mandar la actualización Raw");
        assert_eq!(
            escenario.cliente.rect().await,
            (
                0,
                0,
                4,
                2,
                vec![
                    10, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255, 1, 2, 3, 255, 4, 5, 6, 255,
                    7, 8, 9, 255, 10, 11, 12, 255, 13, 14, 15, 255
                ]
            )
        );
        match escenario
            .servidor
            .leer_mensaje()
            .await
            .expect("siguiente petición")
        {
            MensajeCliente::Peticion {
                incremental,
                x,
                y,
                ancho,
                alto,
            } => {
                assert!(
                    incremental,
                    "después de una actualización toca una incremental"
                );
                assert_eq!((x, y, ancho, alto), (0, 0, 4, 2));
            }
            otro => panic!("se esperaba otra petición y llegó {otro:?}"),
        }
        drop(escenario.servidor);
        assert!(
            escenario.cliente.fin_limpio().await.is_ok(),
            "la sesión debe cerrarse limpiamente al caer el servidor"
        );
    }
}

#[tokio::test]
async fn autenticacion_vnc_correcta_incorrecta_y_cancelada() {
    // Contraseña correcta: el cliente manda la respuesta DES y sigue.
    let mut escenario = nuevo_escenario(
        VersionPrueba::V38,
        SeguridadPrueba::Vnc {
            contrasena: "password",
        },
        4,
        2,
        &[Some("password")],
        1,
    )
    .await;
    assert_eq!(escenario.dialogo.intentos(), vec![1]);
    peticion_inicial(&mut escenario.servidor, 4, 2).await;
    drop(escenario.servidor);
    assert!(escenario.cliente.fin_limpio().await.is_ok());

    // Contraseña incorrecta: Autenticacion con el intento que fija la sesión.
    let mut escenario = nuevo_escenario(
        VersionPrueba::V38,
        SeguridadPrueba::Vnc {
            contrasena: "password",
        },
        4,
        2,
        &[Some("otra cosa")],
        1,
    )
    .await;
    assert_eq!(escenario.dialogo.intentos(), vec![1]);
    match escenario.cliente.evento().await {
        EventoEscritorio::Caido(motivo) => {
            assert!(
                motivo.contains("contraseña incorrecta"),
                "motivo inesperado: {motivo}"
            );
        }
        otro => panic!("se esperaba Caido y llegó {otro:?}"),
    }
    match escenario.cliente.evento().await {
        EventoEscritorio::Fin => {}
        otro => panic!("se esperaba Fin y llegó {otro:?}"),
    }
    match escenario.cliente.terminar().await {
        Err(ErrorEscritorio::Autenticacion(motivo)) => {
            assert!(motivo.contains("intento 1 de 3"), "motivo: {motivo}");
        }
        otro => panic!("se esperaba Autenticacion y llegó {otro:?}"),
    }
    let _ = escenario.servidor;

    // Segundo intento: el diálogo cancela y la sesión acaba en Cancelado.
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let direccion = listener.local_addr().expect("dirección");
    let dialogo = DialogoPrueba::nuevo(&[None]);
    let mut cliente = ClientePrueba::lanzar(direccion, dialogo.clone(), 2);
    let (flujo, _) = tokio::time::timeout(PLAZO, listener.accept())
        .await
        .expect("aceptar")
        .expect("aceptar al cliente");
    let mut servidor = ServidorPrueba::nuevo(flujo);
    servidor.enviar(b"RFB 003.008\n").await.expect("saludo");
    assert_eq!(servidor.leer(12).await.expect("versión"), b"RFB 003.008\n");
    servidor.enviar(&[1, 2]).await.expect("seguridades");
    assert_eq!(servidor.leer(1).await.expect("elegida"), vec![2]);
    servidor.enviar(&reto_de_prueba()).await.expect("reto");
    match cliente.evento().await {
        EventoEscritorio::Caido(motivo) => {
            assert!(motivo.contains("cancelado"), "motivo inesperado: {motivo}");
        }
        otro => panic!("se esperaba Caido y llegó {otro:?}"),
    }
    match cliente.evento().await {
        EventoEscritorio::Fin => {}
        otro => panic!("se esperaba Fin y llegó {otro:?}"),
    }
    match cliente.terminar().await {
        Err(ErrorEscritorio::Cancelado(_)) => {}
        otro => panic!("se esperaba Cancelado y llegó {otro:?}"),
    }
    assert_eq!(dialogo.intentos(), vec![2]);
    drop(servidor);
}

#[tokio::test]
async fn raw_y_copyrect() {
    let mut escenario =
        nuevo_escenario(VersionPrueba::V38, SeguridadPrueba::Ninguna, 8, 4, &[], 1).await;
    peticion_inicial(&mut escenario.servidor, 8, 4).await;
    escenario.cliente.listo().await;

    // Raw de 2×2 en (1, 1).
    let crudos = [1u8, 2, 3, 0, 4, 5, 6, 0, 7, 8, 9, 0, 10, 11, 12, 0];
    escenario
        .servidor
        .enviar_actualizacion(&[rectangulo(0, 1, 1, 2, 2, &crudos)])
        .await
        .expect("actualización Raw");
    assert_eq!(
        escenario.cliente.rect().await,
        (
            1,
            1,
            2,
            2,
            vec![1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]
        )
    );
    match escenario.servidor.leer_mensaje().await.expect("petición") {
        MensajeCliente::Peticion { incremental, .. } => assert!(incremental),
        otro => panic!("se esperaba una petición y llegó {otro:?}"),
    }

    // CopyRect: copia esa zona en (4, 0).
    escenario
        .servidor
        .enviar_actualizacion(&[rectangulo(1, 4, 0, 2, 2, &[0, 1, 0, 1])])
        .await
        .expect("actualización CopyRect");
    match escenario.cliente.evento().await {
        EventoEscritorio::Copia {
            origen,
            destino,
            ancho,
            alto,
        } => {
            assert_eq!(origen, (1, 1));
            assert_eq!(destino, (4, 0));
            assert_eq!((ancho, alto), (2, 2));
        }
        otro => panic!("se esperaba Copia y llegó {otro:?}"),
    }
    drop(escenario.servidor);
    assert!(escenario.cliente.fin_limpio().await.is_ok());
}

fn pintar(destino: &mut [u8], ancho: u16, x: u16, y: u16, w: u16, h: u16, color: [u8; 4]) {
    for fila in y..y + h {
        for columna in x..x + w {
            let posicion = (usize::from(fila) * usize::from(ancho) + usize::from(columna)) * 4;
            destino[posicion..posicion + 4].copy_from_slice(&color);
        }
    }
}

#[tokio::test]
async fn hextile_con_fondo_frente_y_subrectangulos() {
    let ancho = 48u16;
    let alto = 32u16;
    let mut escenario = nuevo_escenario(
        VersionPrueba::V38,
        SeguridadPrueba::Ninguna,
        ANCHO,
        ALTO,
        &[],
        1,
    )
    .await;
    peticion_inicial(&mut escenario.servidor, ANCHO, ALTO).await;
    escenario.cliente.listo().await;

    let mut datos = Vec::new();
    // Tesela 0: fondo y frente explícitos y dos subrectángulos del color de
    // frente.
    datos.push(2 | 4 | 8);
    datos.extend_from_slice(&[1, 2, 3, 0]);
    datos.extend_from_slice(&[10, 20, 30, 0]);
    datos.push(2);
    datos.extend_from_slice(&[0x00, 0x11]);
    datos.extend_from_slice(&[0x34, 0x45]);
    // Tesela 1: cruda de 16×16.
    datos.push(1);
    for _ in 0..256 {
        datos.extend_from_slice(&[5, 6, 7, 0]);
    }
    // Tesela 2: fondo explícito y un subrectángulo con su propio color.
    datos.push(2 | 8 | 16);
    datos.extend_from_slice(&[70, 80, 90, 0]);
    datos.push(1);
    datos.extend_from_slice(&[100, 110, 120, 0]);
    datos.extend_from_slice(&[0x12, 0x00]);
    // Teselas de la segunda fila con subcodificación 0: heredan el fondo de
    // la última tesela (70, 80, 90) y no llevan más bytes.
    datos.extend_from_slice(&[0, 0, 0]);

    escenario
        .servidor
        .enviar_actualizacion(&[rectangulo(5, 0, 0, ancho, alto, &datos)])
        .await
        .expect("actualización Hextile");

    let mut esperado = vec![0u8; usize::from(ancho) * usize::from(alto) * 4];
    pintar(&mut esperado, ancho, 0, 0, 16, 16, [1, 2, 3, 255]);
    pintar(&mut esperado, ancho, 0, 0, 2, 2, [10, 20, 30, 255]);
    pintar(&mut esperado, ancho, 3, 4, 5, 6, [10, 20, 30, 255]);
    pintar(&mut esperado, ancho, 16, 0, 16, 16, [5, 6, 7, 255]);
    pintar(&mut esperado, ancho, 32, 0, 16, 16, [70, 80, 90, 255]);
    pintar(&mut esperado, ancho, 33, 2, 1, 1, [100, 110, 120, 255]);
    for tesela in 0..3u16 {
        pintar(
            &mut esperado,
            ancho,
            tesela * 16,
            16,
            16,
            16,
            [70, 80, 90, 255],
        );
    }
    let (x, y, ancho_rect, alto_rect, pixeles) = escenario.cliente.rect().await;
    assert_eq!((x, y, ancho_rect, alto_rect), (0, 0, ancho, alto));
    assert_eq!(pixeles, esperado, "los píxeles de Hextile no coinciden");
    drop(escenario.servidor);
    assert!(escenario.cliente.fin_limpio().await.is_ok());
}

fn cpixel(r: u8, g: u8, b: u8) -> [u8; 3] {
    [r, g, b]
}

#[tokio::test]
async fn zrle_teselas_crudas_paletas_y_rle() {
    let mut escenario = nuevo_escenario(
        VersionPrueba::V38,
        SeguridadPrueba::Ninguna,
        ANCHO,
        ALTO,
        &[],
        1,
    )
    .await;
    peticion_inicial(&mut escenario.servidor, ANCHO, ALTO).await;
    escenario.cliente.listo().await;
    let mut compresor = CompresorZrle::nuevo();
    let mut rectangulos = Vec::new();

    // Rectángulo 1: tesela cruda de 4×4.
    let mut cruda = vec![0u8];
    for i in 0..16u8 {
        cruda.extend_from_slice(&cpixel(i, i.wrapping_mul(2), i.wrapping_mul(3)));
    }
    rectangulos.push(rectangulo_zrle(0, 0, 4, 4, &mut compresor, &cruda));

    // Rectángulo 2: tesela de un solo color.
    let solida = [1u8, 9, 8, 7];
    rectangulos.push(rectangulo_zrle(8, 0, 3, 2, &mut compresor, &solida));

    // Rectángulo 3: paleta empaquetada de 3 colores, 10×4.
    let mut paleta = vec![3u8];
    paleta.extend_from_slice(&cpixel(1, 2, 3));
    paleta.extend_from_slice(&cpixel(4, 5, 6));
    paleta.extend_from_slice(&cpixel(7, 8, 9));
    let mut empaquetado = vec![0u8; 3 * 4];
    for fila in 0..4usize {
        for columna in 0..10usize {
            let indice = ((columna + fila) % 3) as u8;
            let turno = columna % 4;
            empaquetado[fila * 3 + columna / 4] |= indice << (6 - 2 * turno);
        }
    }
    paleta.extend_from_slice(&empaquetado);
    rectangulos.push(rectangulo_zrle(0, 8, 10, 4, &mut compresor, &paleta));

    // Rectángulo 4: paleta RLE de 2 colores, 5×3 (corrida de 5 y de 10).
    let mut paleta_rle = vec![130u8];
    paleta_rle.extend_from_slice(&cpixel(10, 11, 12));
    paleta_rle.extend_from_slice(&cpixel(20, 21, 22));
    paleta_rle.extend_from_slice(&[0x80, 4, 0x81, 9]);
    rectangulos.push(rectangulo_zrle(12, 8, 5, 3, &mut compresor, &paleta_rle));

    escenario
        .servidor
        .enviar_actualizacion(&rectangulos)
        .await
        .expect("actualización ZRLE");

    // Rectángulo 1.
    let (x, y, ancho, alto, pixeles) = escenario.cliente.rect().await;
    assert_eq!((x, y, ancho, alto), (0, 0, 4, 4));
    let mut esperado = Vec::new();
    for i in 0..16u8 {
        esperado.extend_from_slice(&[i, i.wrapping_mul(2), i.wrapping_mul(3), 255]);
    }
    assert_eq!(pixeles, esperado);
    // Rectángulo 2.
    let (x, y, ancho, alto, pixeles) = escenario.cliente.rect().await;
    assert_eq!((x, y, ancho, alto), (8, 0, 3, 2));
    assert_eq!(pixeles, [9, 8, 7, 255].repeat(6));
    // Rectángulo 3.
    let (_, _, ancho, alto, pixeles) = escenario.cliente.rect().await;
    assert_eq!((ancho, alto), (10, 4));
    let tabla = [[1u8, 2, 3, 255], [4, 5, 6, 255], [7, 8, 9, 255]];
    let mut esperado = Vec::new();
    for fila in 0..4usize {
        for columna in 0..10usize {
            esperado.extend_from_slice(&tabla[(columna + fila) % 3]);
        }
    }
    assert_eq!(pixeles, esperado);
    // Rectángulo 4.
    let (_, _, ancho, alto, pixeles) = escenario.cliente.rect().await;
    assert_eq!((ancho, alto), (5, 3));
    let mut esperado = [10, 11, 12, 255].repeat(5);
    esperado.extend_from_slice(&[20, 21, 22, 255].repeat(10));
    assert_eq!(pixeles, esperado);
    drop(escenario.servidor);
    assert!(escenario.cliente.fin_limpio().await.is_ok());
}

#[tokio::test]
async fn un_fotograma_con_muchos_rectangulos_grandes_se_entrega_entero() {
    // C3: 24 rectángulos ZRLE de 512×512 (1 MB decodificado cada uno) en un
    // solo FramebufferUpdate. El cliente los entrega por tandas y llegan
    // todos, en orden, sin acumular el fotograma completo en memoria.
    let mut escenario = nuevo_escenario(
        VersionPrueba::V38,
        SeguridadPrueba::Ninguna,
        512,
        512,
        &[],
        1,
    )
    .await;
    peticion_inicial(&mut escenario.servidor, 512, 512).await;
    escenario.cliente.listo().await;
    let mut compresor = CompresorZrle::nuevo();
    let mut rectangulos = Vec::new();
    for i in 0..24u8 {
        let mut crudo = Vec::new();
        for _ in 0..(8 * 8) {
            crudo.extend_from_slice(&[1u8, i, i.wrapping_add(1), i.wrapping_add(2)]);
        }
        rectangulos.push(rectangulo_zrle(0, 0, 512, 512, &mut compresor, &crudo));
    }
    escenario
        .servidor
        .enviar_actualizacion(&rectangulos)
        .await
        .expect("fotograma con muchos rectángulos");
    for i in 0..24u8 {
        let (x, y, ancho, alto, pixeles) = escenario.cliente.rect().await;
        assert_eq!((x, y, ancho, alto), (0, 0, 512, 512));
        assert_eq!(pixeles.len(), 512 * 512 * 4);
        let color = [i, i.wrapping_add(1), i.wrapping_add(2), 255];
        assert_eq!(&pixeles[..4], &color);
        assert_eq!(&pixeles[pixeles.len() - 4..], &color);
    }
    drop(escenario.servidor);
    assert!(escenario.cliente.fin_limpio().await.is_ok());
}

#[tokio::test]
async fn desktop_size_y_peticion_completa() {
    let mut escenario =
        nuevo_escenario(VersionPrueba::V38, SeguridadPrueba::Ninguna, 8, 4, &[], 1).await;
    peticion_inicial(&mut escenario.servidor, 8, 4).await;
    escenario.cliente.listo().await;
    escenario
        .servidor
        .enviar_actualizacion(&[rectangulo(-223, 0, 0, 6, 3, &[])])
        .await
        .expect("DesktopSize");
    match escenario.cliente.evento().await {
        EventoEscritorio::Tamano { ancho, alto } => assert_eq!((ancho, alto), (6, 3)),
        otro => panic!("se esperaba Tamano y llegó {otro:?}"),
    }
    match escenario.servidor.leer_mensaje().await.expect("petición") {
        MensajeCliente::Peticion {
            incremental,
            x,
            y,
            ancho,
            alto,
        } => {
            assert!(
                !incremental,
                "tras un DesktopSize el cliente debe pedir el escritorio entero"
            );
            assert_eq!((x, y, ancho, alto), (0, 0, 6, 3));
        }
        otro => panic!("se esperaba una petición y llegó {otro:?}"),
    }
    drop(escenario.servidor);
    assert!(escenario.cliente.fin_limpio().await.is_ok());
}

fn datos_extended_desktop_size(pantallas: &[Pantalla]) -> Vec<u8> {
    let mut datos = vec![pantallas.len() as u8, 0, 0, 0];
    for pantalla in pantallas {
        datos.extend_from_slice(&pantalla.id.to_be_bytes());
        datos.extend_from_slice(&pantalla.x.to_be_bytes());
        datos.extend_from_slice(&pantalla.y.to_be_bytes());
        datos.extend_from_slice(&pantalla.ancho.to_be_bytes());
        datos.extend_from_slice(&pantalla.alto.to_be_bytes());
        datos.extend_from_slice(&pantalla.banderas.to_be_bytes());
    }
    datos
}

#[tokio::test]
async fn extended_desktop_size_y_set_desktop_size() {
    let mut escenario =
        nuevo_escenario(VersionPrueba::V38, SeguridadPrueba::Ninguna, 8, 4, &[], 1).await;
    peticion_inicial(&mut escenario.servidor, 8, 4).await;
    escenario.cliente.listo().await;
    // El servidor anuncia ExtendedDesktopSize con una pantalla del tamaño
    // actual: el cliente lo apunta pero no cambia nada.
    let pantallas = [Pantalla {
        id: 7,
        x: 0,
        y: 0,
        ancho: 8,
        alto: 4,
        banderas: 0,
    }];
    escenario
        .servidor
        .enviar_actualizacion(&[rectangulo(
            -308,
            0,
            0,
            8,
            4,
            &datos_extended_desktop_size(&pantallas),
        )])
        .await
        .expect("ExtendedDesktopSize");
    match escenario.servidor.leer_mensaje().await.expect("petición") {
        MensajeCliente::Peticion {
            incremental,
            ancho,
            alto,
            ..
        } => {
            assert!(
                incremental,
                "el cliente no debe pedir una completa al recibir ExtendedDesktopSize"
            );
            assert_eq!((ancho, alto), (8, 4));
        }
        otro => panic!("se esperaba una petición y llegó {otro:?}"),
    }
    // El visor pide 800×600: SetDesktopSize con la identidad de la pantalla.
    escenario.cliente.ordenar(OrdenEscritorio::Tamano {
        ancho: 800,
        alto: 600,
    });
    match escenario
        .servidor
        .esperar(
            |mensaje| matches!(mensaje, MensajeCliente::CambiarTamano { .. }),
            "SetDesktopSize",
        )
        .await
        .expect("SetDesktopSize")
    {
        MensajeCliente::CambiarTamano {
            ancho,
            alto,
            pantallas,
        } => {
            assert_eq!((ancho, alto), (800, 600));
            assert_eq!(pantallas.len(), 1);
            assert_eq!(pantallas[0].id, 7);
            assert_eq!((pantallas[0].ancho, pantallas[0].alto), (800, 600));
        }
        otro => panic!("se esperaba SetDesktopSize y llegó {otro:?}"),
    }
    // Respuesta del servidor (la pidió el cliente): cambia el tamaño.
    let nuevas = [Pantalla {
        id: 7,
        x: 0,
        y: 0,
        ancho: 800,
        alto: 600,
        banderas: 0,
    }];
    escenario
        .servidor
        .enviar_actualizacion(&[rectangulo(
            -308,
            1,
            0,
            800,
            600,
            &datos_extended_desktop_size(&nuevas),
        )])
        .await
        .expect("respuesta ExtendedDesktopSize");
    match escenario.cliente.evento().await {
        EventoEscritorio::Tamano { ancho, alto } => assert_eq!((ancho, alto), (800, 600)),
        otro => panic!("se esperaba Tamano y llegó {otro:?}"),
    }
    drop(escenario.servidor);
    assert!(escenario.cliente.fin_limpio().await.is_ok());
}

#[tokio::test]
async fn sin_extended_desktop_size_el_tamano_se_ignora() {
    let mut escenario =
        nuevo_escenario(VersionPrueba::V38, SeguridadPrueba::Ninguna, 8, 4, &[], 1).await;
    peticion_inicial(&mut escenario.servidor, 8, 4).await;
    escenario.cliente.listo().await;
    escenario
        .cliente
        .ordenar(OrdenEscritorio::Tamano { ancho: 6, alto: 3 });
    escenario
        .servidor
        .silencio(Duration::from_millis(200))
        .await
        .expect("sin ExtendedDesktopSize no debe mandarse SetDesktopSize");
    drop(escenario.servidor);
    assert!(escenario.cliente.fin_limpio().await.is_ok());
}

// ------------------------------------------------ entrada, cursor y portapapeles

async fn leer_tecla(servidor: &mut ServidorPrueba) -> (bool, u32) {
    match servidor.leer_mensaje().await.expect("tecla") {
        MensajeCliente::Tecla { abajo, keysym } => (abajo, keysym),
        otro => panic!("se esperaba KeyEvent y llegó {otro:?}"),
    }
}

async fn leer_raton(servidor: &mut ServidorPrueba) -> (u8, u16, u16) {
    match servidor.leer_mensaje().await.expect("ratón") {
        MensajeCliente::Raton { botones, x, y } => (botones, x, y),
        otro => panic!("se esperaba PointerEvent y llegó {otro:?}"),
    }
}

async fn leer_corte(servidor: &mut ServidorPrueba) -> (i32, Vec<u8>) {
    match servidor.leer_mensaje().await.expect("portapapeles") {
        MensajeCliente::Corte { largo, datos } => (largo, datos),
        otro => panic!("se esperaba ClientCutText y llegó {otro:?}"),
    }
}

#[tokio::test]
async fn cursor_rgba_y_oculto() {
    let mut escenario = nuevo_escenario(
        VersionPrueba::V38,
        SeguridadPrueba::Ninguna,
        ANCHO,
        ALTO,
        &[],
        1,
    )
    .await;
    peticion_inicial(&mut escenario.servidor, ANCHO, ALTO).await;
    escenario.cliente.listo().await;
    let mut datos = vec![1u8, 2, 3, 0, 4, 5, 6, 0, 7, 8, 9, 0, 10, 11, 12, 0];
    datos.extend_from_slice(&[0x80, 0x40]);
    escenario
        .servidor
        .enviar_actualizacion(&[rectangulo(-239, 1, 0, 2, 2, &datos)])
        .await
        .expect("cursor");
    match escenario.cliente.evento().await {
        EventoEscritorio::Cursor {
            visible,
            x,
            y,
            punto,
            ancho,
            alto,
            pixeles,
        } => {
            assert!(visible);
            assert_eq!(x, 1);
            assert_eq!(y, 0);
            assert_eq!(punto, (1, 0));
            assert_eq!((ancho, alto), (2, 2));
            assert_eq!(
                pixeles,
                vec![1, 2, 3, 255, 4, 5, 6, 0, 7, 8, 9, 0, 10, 11, 12, 255,]
            );
        }
        otro => panic!("se esperaba Cursor y llegó {otro:?}"),
    }
    // Cursor de tamaño cero: el puntero se oculta.
    escenario
        .servidor
        .enviar_actualizacion(&[rectangulo(-239, 0, 0, 0, 0, &[])])
        .await
        .expect("cursor oculto");
    match escenario.cliente.evento().await {
        EventoEscritorio::Cursor {
            visible,
            ancho,
            alto,
            ..
        } => {
            assert!(!visible);
            assert_eq!((ancho, alto), (0, 0));
        }
        otro => panic!("se esperaba Cursor oculto y llegó {otro:?}"),
    }
    drop(escenario.servidor);
    assert!(escenario.cliente.fin_limpio().await.is_ok());
}

#[tokio::test]
async fn portapapeles_latin1_con_sustitucion() {
    let mut escenario = nuevo_escenario(
        VersionPrueba::V38,
        SeguridadPrueba::Ninguna,
        ANCHO,
        ALTO,
        &[],
        1,
    )
    .await;
    peticion_inicial(&mut escenario.servidor, ANCHO, ALTO).await;
    escenario.cliente.listo().await;
    // El servidor manda Latin-1 (0xE9 es «é»).
    escenario
        .servidor
        .enviar(&corte_servidor_basico(b"hola \xe9\n"))
        .await
        .expect("corte del servidor");
    match escenario.cliente.evento().await {
        EventoEscritorio::Portapapeles(texto) => assert_eq!(texto, "hola é\n"),
        otro => panic!("se esperaba Portapapeles y llegó {otro:?}"),
    }
    // El visor pega texto con caracteres fuera de Latin-1: se sustituyen.
    escenario
        .cliente
        .ordenar(OrdenEscritorio::Portapapeles("€ y ñ".to_string()));
    let (largo, datos) = leer_corte(&mut escenario.servidor).await;
    assert_eq!(largo as usize, datos.len());
    assert_eq!(datos, b"? y \xf1".to_vec());
    drop(escenario.servidor);
    assert!(escenario.cliente.fin_limpio().await.is_ok());
}

#[tokio::test]
async fn portapapeles_extendido_utf8() {
    let mut escenario = nuevo_escenario(
        VersionPrueba::V38,
        SeguridadPrueba::Ninguna,
        ANCHO,
        ALTO,
        &[],
        1,
    )
    .await;
    peticion_inicial(&mut escenario.servidor, ANCHO, ALTO).await;
    escenario.cliente.listo().await;
    // El servidor anuncia el portapapeles extendido y sus capacidades.
    escenario
        .servidor
        .enviar(&corte_servidor_extendido(
            CORTE_CAPS | CORTE_TEXTO,
            &(1_000_000u32).to_be_bytes(),
        ))
        .await
        .expect("caps del portapapeles");
    // El cliente responde con sus propias capacidades (longitud negativa).
    let (largo, datos) = leer_corte(&mut escenario.servidor).await;
    assert!(largo < 0, "las capacidades deben ir en formato extendido");
    let banderas = u32::from_be_bytes([datos[0], datos[1], datos[2], datos[3]]);
    assert_eq!(banderas, CORTE_CAPS | CORTE_TEXTO);
    let tope = u32::from_be_bytes([datos[4], datos[5], datos[6], datos[7]]);
    assert_eq!(tope, 1024 * 1024);

    // El servidor manda texto UTF-8 de más de 16 KB en un «provide»
    // comprimido con zlib (M1: el bucle de descompresión reutilizaba la
    // rebanada de flate2 y fallaba a partir de 16 KB).
    let texto = "ñ€ fin ".repeat(20_000);
    assert!(texto.len() > 16 * 1024);
    let mut dentro = ((texto.len() + 1) as u32).to_be_bytes().to_vec();
    dentro.extend_from_slice(texto.as_bytes());
    dentro.push(0);
    let mut codificador = ZlibEncoder::new(Vec::new(), Compression::default());
    {
        use std::io::Write;
        codificador.write_all(&dentro).expect("comprimir");
    }
    let comprimido = codificador.finish().expect("terminar zlib");
    escenario
        .servidor
        .enviar(&corte_servidor_extendido(
            CORTE_PROVISION | CORTE_TEXTO,
            &comprimido,
        ))
        .await
        .expect("provide del portapapeles");
    match escenario.cliente.evento().await {
        EventoEscritorio::Portapapeles(texto_recibido) => assert_eq!(texto_recibido, texto),
        otro => panic!("se esperaba Portapapeles y llegó {otro:?}"),
    }
    // El visor pega texto grande: el cliente lo manda como provide UTF-8.
    let texto_local = "texto ñ ".repeat(3_000);
    escenario
        .cliente
        .ordenar(OrdenEscritorio::Portapapeles(texto_local.clone()));
    let (largo, datos) = leer_corte(&mut escenario.servidor).await;
    assert!(largo < 0);
    let banderas = u32::from_be_bytes([datos[0], datos[1], datos[2], datos[3]]);
    assert_eq!(banderas, CORTE_PROVISION | CORTE_TEXTO);
    let mut descompresor = flate2::read::ZlibDecoder::new(&datos[4..]);
    let mut descomprimido = Vec::new();
    {
        use std::io::Read;
        descompresor
            .read_to_end(&mut descomprimido)
            .expect("descomprimir");
    }
    let tamano = u32::from_be_bytes([
        descomprimido[0],
        descomprimido[1],
        descomprimido[2],
        descomprimido[3],
    ]) as usize;
    assert_eq!(tamano, texto_local.len() + 1);
    let recibido = String::from_utf8(descomprimido[4..4 + tamano - 1].to_vec()).expect("utf-8");
    assert_eq!(recibido, texto_local);

    // El visor pide el portapapeles del remoto: petición en formato extendido.
    escenario
        .cliente
        .ordenar(OrdenEscritorio::PedirPortapapeles);
    let (largo, datos) = leer_corte(&mut escenario.servidor).await;
    assert!(largo < 0);
    let banderas = u32::from_be_bytes([datos[0], datos[1], datos[2], datos[3]]);
    assert_eq!(banderas, CORTE_PETICION | CORTE_TEXTO);
    drop(escenario.servidor);
    assert!(escenario.cliente.fin_limpio().await.is_ok());
}

#[tokio::test]
async fn sin_portapapeles_no_se_intercambia_nada() {
    let mut escenario = nuevo_escenario_portapapeles(
        VersionPrueba::V38,
        SeguridadPrueba::Ninguna,
        ANCHO,
        ALTO,
        &[],
        1,
        false,
    )
    .await;
    peticion_inicial(&mut escenario.servidor, ANCHO, ALTO).await;
    escenario.cliente.listo().await;
    // El servidor manda texto, pero el host tiene el portapapeles apagado: no
    // llega ningún evento y el cliente no contesta.
    escenario
        .servidor
        .enviar(&corte_servidor_basico(b"hola"))
        .await
        .expect("corte del servidor");
    escenario
        .servidor
        .silencio(Duration::from_millis(120))
        .await
        .expect("sin portapapeles no se responde");
    escenario
        .cliente
        .ordenar(OrdenEscritorio::Portapapeles("texto".to_string()));
    escenario
        .cliente
        .ordenar(OrdenEscritorio::PedirPortapapeles);
    escenario
        .servidor
        .silencio(Duration::from_millis(150))
        .await
        .expect("sin portapapeles no se manda ClientCutText");
    drop(escenario.servidor);
    assert!(escenario.cliente.fin_limpio().await.is_ok());
}

#[tokio::test]
async fn teclado_por_keysyms_y_ctrl_alt_supr() {
    let mut escenario = nuevo_escenario(
        VersionPrueba::V38,
        SeguridadPrueba::Ninguna,
        ANCHO,
        ALTO,
        &[],
        1,
    )
    .await;
    peticion_inicial(&mut escenario.servidor, ANCHO, ALTO).await;
    escenario.cliente.listo().await;
    escenario.cliente.ordenar(OrdenEscritorio::Tecla {
        codigo: "KeyA".to_string(),
        keysym: 0x61,
        abajo: true,
    });
    assert_eq!(leer_tecla(&mut escenario.servidor).await, (true, 0x61));
    escenario.cliente.ordenar(OrdenEscritorio::Tecla {
        codigo: "KeyA".to_string(),
        keysym: 0x61,
        abajo: false,
    });
    assert_eq!(leer_tecla(&mut escenario.servidor).await, (false, 0x61));

    escenario.cliente.ordenar(OrdenEscritorio::CtrlAltSupr);
    let esperado = [
        (true, 0xFFE3),
        (true, 0xFFE9),
        (true, 0xFFFF),
        (false, 0xFFFF),
        (false, 0xFFE9),
        (false, 0xFFE3),
    ];
    for (abajo, keysym) in esperado {
        assert_eq!(leer_tecla(&mut escenario.servidor).await, (abajo, keysym));
    }

    escenario.cliente.ordenar(OrdenEscritorio::TeclaWindows);
    assert_eq!(leer_tecla(&mut escenario.servidor).await, (true, 0xFFEB));
    assert_eq!(leer_tecla(&mut escenario.servidor).await, (false, 0xFFEB));

    // SoltarTodo libera las teclas pulsadas en orden inverso.
    escenario.cliente.ordenar(OrdenEscritorio::Tecla {
        codigo: "ShiftLeft".to_string(),
        keysym: 0xFFE1,
        abajo: true,
    });
    assert_eq!(leer_tecla(&mut escenario.servidor).await, (true, 0xFFE1));
    escenario.cliente.ordenar(OrdenEscritorio::Tecla {
        codigo: "ControlLeft".to_string(),
        keysym: 0xFFE3,
        abajo: true,
    });
    assert_eq!(leer_tecla(&mut escenario.servidor).await, (true, 0xFFE3));
    escenario.cliente.ordenar(OrdenEscritorio::SoltarTodo);
    assert_eq!(leer_tecla(&mut escenario.servidor).await, (false, 0xFFE3));
    assert_eq!(leer_tecla(&mut escenario.servidor).await, (false, 0xFFE1));
    drop(escenario.servidor);
    assert!(escenario.cliente.fin_limpio().await.is_ok());
}

#[tokio::test]
async fn raton_botones_y_rueda() {
    let mut escenario = nuevo_escenario(
        VersionPrueba::V38,
        SeguridadPrueba::Ninguna,
        ANCHO,
        ALTO,
        &[],
        1,
    )
    .await;
    peticion_inicial(&mut escenario.servidor, ANCHO, ALTO).await;
    escenario.cliente.listo().await;
    escenario
        .cliente
        .ordenar(OrdenEscritorio::Raton { x: 10, y: 20 });
    assert_eq!(leer_raton(&mut escenario.servidor).await, (0, 10, 20));
    // Las coordenadas fuera del escritorio se recortan.
    escenario
        .cliente
        .ordenar(OrdenEscritorio::Raton { x: 5000, y: 5000 });
    assert_eq!(
        leer_raton(&mut escenario.servidor).await,
        (0, ANCHO - 1, ALTO - 1)
    );
    escenario.cliente.ordenar(OrdenEscritorio::Boton {
        boton: 1,
        abajo: true,
    });
    assert_eq!(
        leer_raton(&mut escenario.servidor).await,
        (1, ANCHO - 1, ALTO - 1)
    );
    escenario.cliente.ordenar(OrdenEscritorio::Boton {
        boton: 1,
        abajo: false,
    });
    assert_eq!(
        leer_raton(&mut escenario.servidor).await,
        (0, ANCHO - 1, ALTO - 1)
    );
    escenario.cliente.ordenar(OrdenEscritorio::Boton {
        boton: 2,
        abajo: true,
    });
    assert_eq!(
        leer_raton(&mut escenario.servidor).await,
        (2, ANCHO - 1, ALTO - 1)
    );
    // Rueda hacia abajo: botón 5 (bit 4), pulsar y soltar.
    escenario
        .cliente
        .ordenar(OrdenEscritorio::Rueda { dx: 0.0, dy: 1.0 });
    assert_eq!(
        leer_raton(&mut escenario.servidor).await,
        (2 | 0x10, ANCHO - 1, ALTO - 1)
    );
    assert_eq!(
        leer_raton(&mut escenario.servidor).await,
        (2, ANCHO - 1, ALTO - 1)
    );
    // Rueda hacia arriba: botón 4 (bit 3).
    escenario
        .cliente
        .ordenar(OrdenEscritorio::Rueda { dx: 0.0, dy: -1.0 });
    assert_eq!(
        leer_raton(&mut escenario.servidor).await,
        (2 | 0x08, ANCHO - 1, ALTO - 1)
    );
    assert_eq!(
        leer_raton(&mut escenario.servidor).await,
        (2, ANCHO - 1, ALTO - 1)
    );
    escenario.cliente.ordenar(OrdenEscritorio::Boton {
        boton: 2,
        abajo: false,
    });
    assert_eq!(
        leer_raton(&mut escenario.servidor).await,
        (0, ANCHO - 1, ALTO - 1)
    );
    // Rueda horizontal: botones 6 (izquierda) y 7 (derecha).
    escenario
        .cliente
        .ordenar(OrdenEscritorio::Rueda { dx: -1.0, dy: 0.0 });
    assert_eq!(
        leer_raton(&mut escenario.servidor).await,
        (0x20, ANCHO - 1, ALTO - 1)
    );
    assert_eq!(
        leer_raton(&mut escenario.servidor).await,
        (0, ANCHO - 1, ALTO - 1)
    );
    escenario
        .cliente
        .ordenar(OrdenEscritorio::Rueda { dx: 1.0, dy: 0.0 });
    assert_eq!(
        leer_raton(&mut escenario.servidor).await,
        (0x40, ANCHO - 1, ALTO - 1)
    );
    assert_eq!(
        leer_raton(&mut escenario.servidor).await,
        (0, ANCHO - 1, ALTO - 1)
    );
    // El desplazamiento suave se acumula antes de mandar un clic.
    escenario
        .cliente
        .ordenar(OrdenEscritorio::Rueda { dx: 0.0, dy: 0.5 });
    escenario
        .servidor
        .silencio(Duration::from_millis(150))
        .await
        .expect("media rueda no manda nada");
    escenario
        .cliente
        .ordenar(OrdenEscritorio::Rueda { dx: 0.0, dy: 0.5 });
    assert_eq!(
        leer_raton(&mut escenario.servidor).await,
        (0x10, ANCHO - 1, ALTO - 1)
    );
    assert_eq!(
        leer_raton(&mut escenario.servidor).await,
        (0, ANCHO - 1, ALTO - 1)
    );
    drop(escenario.servidor);
    assert!(escenario.cliente.fin_limpio().await.is_ok());
}

#[tokio::test]
async fn teclas_de_qemu_con_scancodes() {
    let mut escenario = nuevo_escenario(
        VersionPrueba::V38,
        SeguridadPrueba::Ninguna,
        ANCHO,
        ALTO,
        &[],
        1,
    )
    .await;
    peticion_inicial(&mut escenario.servidor, ANCHO, ALTO).await;
    escenario.cliente.listo().await;
    // El servidor anuncia la extensión de teclas de QEMU con un rectángulo
    // vacío.
    escenario
        .servidor
        .enviar_actualizacion(&[rectangulo(-258, 0, 0, 0, 0, &[])])
        .await
        .expect("anuncio QEMU");
    match escenario.servidor.leer_mensaje().await.expect("petición") {
        MensajeCliente::Peticion { incremental, .. } => assert!(incremental),
        otro => panic!("se esperaba una petición y llegó {otro:?}"),
    }
    escenario.cliente.ordenar(OrdenEscritorio::Tecla {
        codigo: "KeyA".to_string(),
        keysym: 0x61,
        abajo: true,
    });
    match escenario.servidor.leer_mensaje().await.expect("tecla QEMU") {
        MensajeCliente::TeclaQemu {
            abajo,
            keysym,
            keycode,
        } => {
            assert!(abajo);
            assert_eq!(keysym, 0x61);
            assert_eq!(keycode, 0x1E);
        }
        otro => panic!("se esperaba la tecla extendida de QEMU y llegó {otro:?}"),
    }
    escenario.cliente.ordenar(OrdenEscritorio::Tecla {
        codigo: "ArrowUp".to_string(),
        keysym: 0xFF52,
        abajo: true,
    });
    match escenario
        .servidor
        .leer_mensaje()
        .await
        .expect("flecha QEMU")
    {
        MensajeCliente::TeclaQemu { keycode, .. } => assert_eq!(keycode, 0xC8),
        otro => panic!("se esperaba la tecla extendida de QEMU y llegó {otro:?}"),
    }
    // Si el código no está en la tabla, se manda un KeyEvent normal.
    escenario.cliente.ordenar(OrdenEscritorio::Tecla {
        codigo: "Rara".to_string(),
        keysym: 0x0100_0041,
        abajo: true,
    });
    assert_eq!(
        leer_tecla(&mut escenario.servidor).await,
        (true, 0x0100_0041)
    );
    drop(escenario.servidor);
    assert!(escenario.cliente.fin_limpio().await.is_ok());
}

#[tokio::test]
async fn ritmo_de_peticiones_como_mucho_60_por_segundo() {
    let mut escenario =
        nuevo_escenario(VersionPrueba::V38, SeguridadPrueba::Ninguna, 8, 4, &[], 1).await;
    peticion_inicial(&mut escenario.servidor, 8, 4).await;
    escenario.cliente.listo().await;
    // La petición inicial ya se ha leído. La siguiente se mide desde ese
    // momento: no puede llegar antes de 16 ms (60 por segundo como mucho).
    let mut anterior = std::time::Instant::now();
    escenario
        .servidor
        .enviar_actualizacion(&[rectangulo(0, 0, 0, 1, 1, &[1, 2, 3, 0])])
        .await
        .expect("actualización");
    escenario.cliente.rect().await;
    match escenario.servidor.leer_mensaje().await.expect("petición") {
        MensajeCliente::Peticion { incremental, .. } => assert!(incremental),
        otro => panic!("se esperaba una petición y llegó {otro:?}"),
    }
    assert!(
        anterior.elapsed() >= Duration::from_millis(10),
        "la siguiente petición llegó antes de 10 ms (más de 60 por segundo)"
    );
    // Sin actualización en curso no se piden más.
    escenario
        .servidor
        .silencio(Duration::from_millis(120))
        .await
        .expect("no debe haber peticiones sin respuesta pendiente");
    // Una actualización inmediata tras recibir la petición vuelve a respetar
    // el intervalo entre peticiones.
    escenario
        .servidor
        .enviar_actualizacion(&[rectangulo(0, 0, 0, 1, 1, &[4, 5, 6, 0])])
        .await
        .expect("actualización");
    escenario.cliente.rect().await;
    match escenario.servidor.leer_mensaje().await.expect("petición") {
        MensajeCliente::Peticion { incremental, .. } => assert!(incremental),
        otro => panic!("se esperaba una petición y llegó {otro:?}"),
    }
    anterior = std::time::Instant::now();
    escenario
        .servidor
        .enviar_actualizacion(&[rectangulo(0, 0, 0, 1, 1, &[7, 8, 9, 0])])
        .await
        .expect("actualización");
    escenario.cliente.rect().await;
    match escenario.servidor.leer_mensaje().await.expect("petición") {
        MensajeCliente::Peticion { incremental, .. } => assert!(incremental),
        otro => panic!("se esperaba una petición y llegó {otro:?}"),
    }
    assert!(
        anterior.elapsed() >= Duration::from_millis(10),
        "la siguiente petición llegó antes de 10 ms (más de 60 por segundo)"
    );
    drop(escenario.servidor);
    assert!(escenario.cliente.fin_limpio().await.is_ok());
}

// ------------------------------------------------------------------- límites

#[tokio::test]
async fn escritorio_enorme_en_server_init_cae() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("servidor de prueba");
    let direccion = listener.local_addr().expect("dirección");
    let dialogo = DialogoPrueba::nuevo(&[]);
    let cliente = ClientePrueba::lanzar(direccion, dialogo, 1);
    let (flujo, _) = tokio::time::timeout(PLAZO, listener.accept())
        .await
        .expect("aceptar")
        .expect("aceptar al cliente");
    let mut servidor = ServidorPrueba::nuevo(flujo);
    servidor.enviar(b"RFB 003.008\n").await.expect("saludo");
    assert_eq!(servidor.leer(12).await.expect("versión"), b"RFB 003.008\n");
    servidor.enviar(&[1, 1]).await.expect("seguridades");
    assert_eq!(servidor.leer(1).await.expect("elegida"), vec![1]);
    servidor
        .enviar(&0u32.to_be_bytes())
        .await
        .expect("resultado");
    assert_eq!(servidor.leer(1).await.expect("ClientInit"), vec![1]);
    let mut init = Vec::new();
    init.extend_from_slice(&9000u16.to_be_bytes());
    init.extend_from_slice(&100u16.to_be_bytes());
    init.extend_from_slice(&formato_natural());
    init.extend_from_slice(&0u32.to_be_bytes());
    servidor.enviar(&init).await.expect("ServerInit enorme");
    let error = cliente.fallo("escritorio de 9000×100").await;
    assert!(matches!(error, ErrorEscritorio::Datos(_)));
    drop(servidor);
}

#[tokio::test]
async fn rectangulo_fuera_del_escritorio_cae() {
    let escenario =
        nuevo_escenario(VersionPrueba::V38, SeguridadPrueba::Ninguna, 8, 4, &[], 1).await;
    let Escenario {
        cliente,
        mut servidor,
        ..
    } = escenario;
    peticion_inicial(&mut servidor, 8, 4).await;
    servidor
        .enviar_actualizacion(&[rectangulo(0, 6, 0, 4, 2, &[0u8; 32])])
        .await
        .expect("rectángulo fuera");
    let error = cliente.fallo("fuera del escritorio").await;
    assert!(matches!(error, ErrorEscritorio::Datos(_)));
    drop(servidor);
}

#[tokio::test]
async fn zrle_con_datos_de_mas_cae() {
    let escenario =
        nuevo_escenario(VersionPrueba::V38, SeguridadPrueba::Ninguna, 8, 4, &[], 1).await;
    let Escenario {
        cliente,
        mut servidor,
        ..
    } = escenario;
    peticion_inicial(&mut servidor, 8, 4).await;
    let mut compresor = CompresorZrle::nuevo();
    // Una tesela de 2×2 necesita 12 bytes; se mandan 24.
    let datos = [0u8; 24];
    let rectangulo = rectangulo_zrle(0, 0, 2, 2, &mut compresor, &datos);
    servidor
        .enviar_actualizacion(&[rectangulo])
        .await
        .expect("ZRLE con datos de más");
    let error = cliente.fallo("ZRLE").await;
    assert!(matches!(error, ErrorEscritorio::Datos(_)));
    drop(servidor);
}

#[tokio::test]
async fn cursor_enorme_cae() {
    let escenario =
        nuevo_escenario(VersionPrueba::V38, SeguridadPrueba::Ninguna, 8, 4, &[], 1).await;
    let Escenario {
        cliente,
        mut servidor,
        ..
    } = escenario;
    peticion_inicial(&mut servidor, 8, 4).await;
    servidor
        .enviar_actualizacion(&[rectangulo(-239, 0, 0, 300, 300, &[])])
        .await
        .expect("cursor enorme");
    let error = cliente.fallo("cursor").await;
    assert!(matches!(error, ErrorEscritorio::Datos(_)));
    drop(servidor);
}

#[tokio::test]
async fn texto_de_portapapeles_enorme_cae() {
    let escenario =
        nuevo_escenario(VersionPrueba::V38, SeguridadPrueba::Ninguna, 8, 4, &[], 1).await;
    let Escenario {
        cliente,
        mut servidor,
        ..
    } = escenario;
    peticion_inicial(&mut servidor, 8, 4).await;
    let mut mensaje = vec![3u8, 0, 0, 0];
    mensaje.extend_from_slice(&((1024 * 1024 + 1) as u32).to_be_bytes());
    servidor
        .enviar(&mensaje)
        .await
        .expect("cabecera del portapapeles enorme");
    let error = cliente.fallo("1 MB").await;
    assert!(matches!(error, ErrorEscritorio::Datos(_)));
    drop(servidor);
}

#[tokio::test]
async fn desktop_size_enorme_cae() {
    let escenario =
        nuevo_escenario(VersionPrueba::V38, SeguridadPrueba::Ninguna, 8, 4, &[], 1).await;
    let Escenario {
        cliente,
        mut servidor,
        ..
    } = escenario;
    peticion_inicial(&mut servidor, 8, 4).await;
    servidor
        .enviar_actualizacion(&[rectangulo(-223, 0, 0, 9000, 100, &[])])
        .await
        .expect("DesktopSize enorme");
    let error = cliente.fallo("DesktopSize").await;
    assert!(matches!(error, ErrorEscritorio::Datos(_)));
    drop(servidor);
}
