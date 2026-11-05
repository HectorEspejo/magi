//! Cliente RFB propio (pista VNC, RFC 6143): versiones 3.3, 3.7 y 3.8,
//! seguridad None y VNC Authentication, codificaciones Raw, CopyRect, Hextile y
//! ZRLE, seudocodificaciones DesktopSize, ExtendedDesktopSize y Cursor, entrada
//! por keysyms (y teclas de QEMU) y portapapeles de texto.
//!
//! Toda entrada del servidor es no fiable: escritorio de hasta 8192×8192,
//! rectángulos dentro del escritorio, ZRLE acotado, cursor de hasta 256×256 y
//! texto de hasta 1 MB; nunca un pánico.
//!
//! Toda la red y el análisis del protocolo viven en la misma tarea async. El
//! bucle no se bloquea esperando una actualización: alterna la lectura del
//! socket con las órdenes del visor, y el analizador es incremental (guarda el
//! estado a medias en vez de releer mensajes completos), así que una tecla no
//! espera al siguiente fotograma.

mod hextile;
mod protocolo;
mod seguridad;
mod zrle;

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

use crate::escritorio::{
    teclas, Conexion, ErrorEscritorio, EventoEscritorio, ImagenEscritorio, OrdenEscritorio,
};

use protocolo::{
    datos_invalidos, descomprimir_acotado, mensaje_cambiar_tamano, mensaje_corte_caps,
    mensaje_corte_latin1, mensaje_corte_peticion, mensaje_corte_provision_texto,
    mensaje_fijar_codificaciones, mensaje_fijar_pixel, mensaje_pedir_actualizacion, mensaje_raton,
    mensaje_tecla, mensaje_tecla_qemu, pixel_a_rgba, Entrada, BYTES_POR_PIXEL, COD_COPYRECT,
    COD_CURSOR, COD_DESKTOP_SIZE, COD_EXTENDED_DESKTOP_SIZE, COD_HEXTILE,
    COD_PORTAPAPELES_EXTENDIDO, COD_QEMU_EXTENDED_KEY, COD_RAW, COD_ZRLE, CORTE_AVISO, CORTE_CAPS,
    CORTE_MAX, CORTE_PETICION, CORTE_PROVISION, CORTE_TEXTO, CURSOR_MAX, LADO_MAX,
    MSG_ACTUALIZACION, MSG_CAMPANA, MSG_CORTE_SERVIDOR, MSG_FIN_CONTINUAS, MSG_TABLA_COLORES,
    MSG_VALLA_SERVIDOR, TEXTO_MAX,
};

use seguridad::{Escritor, Lector};

/// Bytes que se leen de una vez del socket.
const TAMANO_TEMPORAL: usize = 64 * 1024;
/// Peticiones de actualización por segundo como máximo.
const FPS_MAX: u64 = 60;
/// Intervalo mínimo entre peticiones de actualización (1000/60 ms).
const INTERVALO_PETICION: Duration = Duration::from_millis(1000 / FPS_MAX);
/// Tamaño mínimo que el visor puede pedir (coherente con el formulario).
const LADO_MIN_VISOR: u16 = 200;
/// Plazo de escritura al servidor.
const PLAZO_ESCRITURA: Duration = Duration::from_secs(10);
/// Tope de píxeles (bytes RGBA) decodificados y pendientes de entregar al
/// puente. Al alcanzarlo se entrega la tanda y se sigue analizando: un
/// servidor hostil no puede acumular gigabytes de rectángulos (C3).
const LIMITE_PIXELES_PENDIENTES: usize = 16 * 1024 * 1024;
/// Presupuesto de píxeles decodificados por FramebufferUpdate. Una
/// actualización completa legítima no pasa de 1,5 veces el escritorio; muy por
/// encima es un servidor hostil que quiere agotar la memoria (C3).
const PRESUPUESTO_PIXELES_MIN: usize = 64 * 1024 * 1024;

/// Abre una sesión VNC y corre hasta que termina. Emite siempre un único
/// evento de fin de sesión (Caido o Cerrado) seguido de Fin.
pub async fn abrir(conexion: Conexion) -> Result<(), ErrorEscritorio> {
    let Conexion {
        flujo,
        config,
        dialogo,
        eventos,
        ordenes,
        intento,
    } = conexion;
    let (mut lector, mut escritor) = tokio::io::split(flujo);
    let inicio = match seguridad::iniciar(&mut lector, &mut escritor, &dialogo, intento).await {
        Ok(inicio) => inicio,
        Err(error) => {
            terminar(&eventos, Some(error.motivo())).await;
            return Err(error);
        }
    };
    if let Err(error) = preparar(&mut escritor, &eventos, &inicio).await {
        terminar(&eventos, Some(error.motivo())).await;
        return Err(error);
    }
    let mut sesion = Sesion::nueva(
        lector,
        escritor,
        eventos,
        ordenes,
        &inicio,
        config.portapapeles,
    );
    match sesion.correr().await {
        Ok(Cierre::Remoto(motivo)) | Ok(Cierre::Visor(motivo)) => {
            // El motivo se manda como Cerrado: la conexión no falló.
            let _ = sesion.eventos.send(EventoEscritorio::Cerrado(motivo)).await;
            let _ = sesion.eventos.send(EventoEscritorio::Fin).await;
            Ok(())
        }
        Err(error) => {
            let motivo = error.motivo().to_string();
            terminar(&sesion.eventos, Some(&motivo)).await;
            Err(error)
        }
    }
}

/// Manda SetPixelFormat y SetEncodings y anuncia la sesión al puente (Abierto,
/// Imagen y el tamaño inicial).
async fn preparar(
    escritor: &mut Escritor,
    eventos: &mpsc::Sender<EventoEscritorio>,
    inicio: &seguridad::SesionIniciada,
) -> Result<(), ErrorEscritorio> {
    // Los dos mensajes iniciales van en una sola escritura: son pequeños y
    // separarlos puede caer en el retardo de ACK de TCP (Nagle).
    let mut inicial = mensaje_fijar_pixel().to_vec();
    inicial.extend_from_slice(&mensaje_fijar_codificaciones());
    escribir(escritor, &inicial).await?;
    eventos
        .send(EventoEscritorio::Abierto)
        .await
        .map_err(|_| ErrorEscritorio::Remoto("el visor cerró la sesión".to_string()))?;
    eventos
        .send(EventoEscritorio::Imagen(ImagenEscritorio::Rectangulos))
        .await
        .map_err(|_| ErrorEscritorio::Remoto("el visor cerró la sesión".to_string()))?;
    eventos
        .send(EventoEscritorio::Tamano {
            ancho: inicio.ancho,
            alto: inicio.alto,
        })
        .await
        .map_err(|_| ErrorEscritorio::Remoto("el visor cerró la sesión".to_string()))?;
    Ok(())
}

/// Emite el evento final (Caido si hay motivo) seguido de Fin. Si el puente ya
/// no escucha, no hay nada que hacer.
async fn terminar(eventos: &mpsc::Sender<EventoEscritorio>, motivo: Option<&str>) {
    let evento = match motivo {
        Some(motivo) => EventoEscritorio::Caido(motivo.to_string()),
        None => return,
    };
    let _ = eventos.send(evento).await;
    let _ = eventos.send(EventoEscritorio::Fin).await;
}

/// Forma limpia de terminar el bucle.
enum Cierre {
    /// El servidor VNC cerró la conexión.
    Remoto(String),
    /// El visor o la capa de sesión cerraron el canal de órdenes.
    Visor(String),
}

/// Escribe un mensaje al servidor con plazo.
async fn escribir(escritor: &mut Escritor, datos: &[u8]) -> Result<(), ErrorEscritorio> {
    match tokio::time::timeout(PLAZO_ESCRITURA, escritor.write_all(datos)).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(ErrorEscritorio::Conexion(format!(
            "no se pudo escribir al servidor VNC: {error}"
        ))),
        Err(_) => Err(ErrorEscritorio::Conexion(
            "el servidor VNC no acepta datos (10 s)".to_string(),
        )),
    }
}

// ---------------------------------------------------------------- cabeceras

/// Cabecera de un rectángulo de FramebufferUpdate.
#[derive(Debug, Clone, Copy)]
struct CabeceraRect {
    x: u16,
    y: u16,
    ancho: u16,
    alto: u16,
    codificacion: i32,
}

impl CabeceraRect {
    fn tomar(entrada: &mut Entrada) -> Option<Self> {
        let bytes = entrada.mirar(12)?;
        let cabecera = Self {
            x: u16::from_be_bytes([bytes[0], bytes[1]]),
            y: u16::from_be_bytes([bytes[2], bytes[3]]),
            ancho: u16::from_be_bytes([bytes[4], bytes[5]]),
            alto: u16::from_be_bytes([bytes[6], bytes[7]]),
            codificacion: i32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
        };
        entrada.tomar(12);
        Some(cabecera)
    }

    /// Comprueba que un rectángulo de píxeles cabe en el escritorio.
    fn validar_en_escritorio(&self, ancho: u16, alto: u16) -> Result<(), ErrorEscritorio> {
        if self.ancho == 0 || self.alto == 0 {
            return Err(datos_invalidos("rectángulo de tamaño cero"));
        }
        let x = u32::from(self.x);
        let y = u32::from(self.y);
        if x + u32::from(self.ancho) > u32::from(ancho)
            || y + u32::from(self.alto) > u32::from(alto)
        {
            return Err(datos_invalidos(format!(
                "rectángulo en ({}, {}) de {}×{} fuera del escritorio {}×{}",
                self.x, self.y, self.ancho, self.alto, ancho, alto
            )));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- resultados

/// Lo que produce un rectángulo cuando termina de decodificarse.
enum ResultadoRect {
    Rect {
        x: u16,
        y: u16,
        ancho: u16,
        alto: u16,
        pixeles: Vec<u8>,
    },
    Copia {
        origen: (u16, u16),
        destino: (u16, u16),
        ancho: u16,
        alto: u16,
    },
    Cursor {
        visible: bool,
        x: u16,
        y: u16,
        ancho: u16,
        alto: u16,
        pixeles: Vec<u8>,
    },
    Tamano {
        ancho: u16,
        alto: u16,
        extendido: bool,
        pantalla_id: Option<u32>,
        cambiar: bool,
    },
}

/// Cuerpo de un rectángulo que se está decodificando.
enum Cuerpo {
    /// Sin cuerpo (estado transitorio al sacar el cuerpo del enum).
    Vacio,
    Raw {
        x: u16,
        y: u16,
        ancho: u16,
        alto: u16,
        pixeles: Vec<u8>,
        restantes: usize,
    },
    CopyRect {
        x: u16,
        y: u16,
        ancho: u16,
        alto: u16,
    },
    Hextile(hextile::Hextile),
    Zrle(zrle::Zrle),
    Cursor {
        x: u16,
        y: u16,
        ancho: u16,
        alto: u16,
        datos: Vec<u8>,
        necesarios: usize,
    },
    ExtendedDesktopSize {
        x: u16,
        y: u16,
        ancho: u16,
        alto: u16,
        pantallas: Option<u8>,
        datos: Vec<u8>,
        necesarios: usize,
    },
}

impl Cuerpo {
    /// Consume lo que pueda. Devuelve None si todavía faltan bytes y el
    /// resultado cuando el rectángulo está completo.
    fn avanzar(
        &mut self,
        entrada: &mut Entrada,
        zrle: &mut zrle::DescompresorZrle,
        ancho_escritorio: u16,
        alto_escritorio: u16,
    ) -> Result<Option<ResultadoRect>, ErrorEscritorio> {
        let cuerpo = std::mem::replace(self, Cuerpo::Vacio);
        match cuerpo {
            Cuerpo::Vacio => Ok(None),
            Cuerpo::Raw {
                x,
                y,
                ancho,
                alto,
                mut pixeles,
                mut restantes,
            } => {
                if restantes > 0 {
                    let listos = entrada.disponible() / BYTES_POR_PIXEL;
                    if listos == 0 {
                        *self = Cuerpo::Raw {
                            x,
                            y,
                            ancho,
                            alto,
                            pixeles,
                            restantes,
                        };
                        return Ok(None);
                    }
                    let tomar = listos.min(restantes);
                    let Some(datos) = entrada.tomar(tomar * BYTES_POR_PIXEL) else {
                        return Err(datos_invalidos("rectángulo Raw incompleto"));
                    };
                    for crudo in datos.chunks_exact(BYTES_POR_PIXEL) {
                        pixeles.extend_from_slice(&pixel_a_rgba(crudo));
                    }
                    restantes -= tomar;
                }
                if restantes == 0 {
                    Ok(Some(ResultadoRect::Rect {
                        x,
                        y,
                        ancho,
                        alto,
                        pixeles,
                    }))
                } else {
                    *self = Cuerpo::Raw {
                        x,
                        y,
                        ancho,
                        alto,
                        pixeles,
                        restantes,
                    };
                    Ok(None)
                }
            }
            Cuerpo::CopyRect { x, y, ancho, alto } => {
                if entrada.disponible() < 4 {
                    *self = Cuerpo::CopyRect { x, y, ancho, alto };
                    return Ok(None);
                }
                let Some(origen_x) = entrada.u16() else {
                    return Ok(None);
                };
                let Some(origen_y) = entrada.u16() else {
                    return Ok(None);
                };
                if u32::from(origen_x) + u32::from(ancho) > u32::from(ancho_escritorio)
                    || u32::from(origen_y) + u32::from(alto) > u32::from(alto_escritorio)
                {
                    return Err(datos_invalidos(format!(
                        "CopyRect desde ({origen_x}, {origen_y}) fuera del escritorio"
                    )));
                }
                Ok(Some(ResultadoRect::Copia {
                    origen: (origen_x, origen_y),
                    destino: (x, y),
                    ancho,
                    alto,
                }))
            }
            Cuerpo::Hextile(mut decodificador) => {
                if !decodificador.avanzar(entrada)? {
                    *self = Cuerpo::Hextile(decodificador);
                    return Ok(None);
                }
                let rectangulo = decodificador.terminar();
                Ok(Some(ResultadoRect::Rect {
                    x: rectangulo.x,
                    y: rectangulo.y,
                    ancho: rectangulo.ancho,
                    alto: rectangulo.alto,
                    pixeles: rectangulo.pixeles,
                }))
            }
            Cuerpo::Zrle(mut decodificador) => {
                if !decodificador.avanzar(entrada, zrle)? {
                    *self = Cuerpo::Zrle(decodificador);
                    return Ok(None);
                }
                let rectangulo = decodificador.terminar();
                Ok(Some(ResultadoRect::Rect {
                    x: rectangulo.x,
                    y: rectangulo.y,
                    ancho: rectangulo.ancho,
                    alto: rectangulo.alto,
                    pixeles: rectangulo.pixeles,
                }))
            }
            Cuerpo::Cursor {
                x,
                y,
                ancho,
                alto,
                mut datos,
                necesarios,
            } => {
                let faltan = necesarios.saturating_sub(datos.len());
                let tomar = faltan.min(entrada.disponible());
                if tomar > 0 {
                    let Some(trozo) = entrada.tomar(tomar) else {
                        return Err(datos_invalidos("cursor incompleto"));
                    };
                    datos.extend_from_slice(trozo);
                }
                if datos.len() < necesarios {
                    *self = Cuerpo::Cursor {
                        x,
                        y,
                        ancho,
                        alto,
                        datos,
                        necesarios,
                    };
                    return Ok(None);
                }
                let mut pixeles = Vec::with_capacity(usize::from(ancho) * usize::from(alto) * 4);
                let fila_mascara = usize::from(ancho).div_ceil(8);
                let total_pixeles = usize::from(ancho) * usize::from(alto);
                for indice in 0..total_pixeles {
                    let base = indice * BYTES_POR_PIXEL;
                    let Some(crudo) = datos.get(base..base + BYTES_POR_PIXEL) else {
                        return Err(datos_invalidos("cursor con píxeles de menos"));
                    };
                    let mut rgba = pixel_a_rgba(crudo);
                    let fila = indice / usize::from(ancho);
                    let columna = indice % usize::from(ancho);
                    let mascara = datos
                        .get(total_pixeles * BYTES_POR_PIXEL + fila * fila_mascara + columna / 8)
                        .copied()
                        .unwrap_or(0);
                    let bit = 7 - (columna % 8);
                    rgba[3] = if (mascara >> bit) & 1 == 1 { 255 } else { 0 };
                    pixeles.extend_from_slice(&rgba);
                }
                Ok(Some(ResultadoRect::Cursor {
                    visible: true,
                    x,
                    y,
                    ancho,
                    alto,
                    pixeles,
                }))
            }
            Cuerpo::ExtendedDesktopSize {
                x,
                y,
                ancho,
                alto,
                pantallas,
                mut datos,
                mut necesarios,
            } => {
                let mut pantallas = pantallas;
                if pantallas.is_none() {
                    if entrada.disponible() < 4 {
                        *self = Cuerpo::ExtendedDesktopSize {
                            x,
                            y,
                            ancho,
                            alto,
                            pantallas,
                            datos,
                            necesarios,
                        };
                        return Ok(None);
                    }
                    let Some(cabecera) = entrada.tomar(4) else {
                        return Ok(None);
                    };
                    let numero = cabecera.first().copied().unwrap_or(0);
                    pantallas = Some(numero);
                    necesarios = usize::from(numero) * 16;
                }
                if datos.len() < necesarios {
                    let faltan = necesarios - datos.len();
                    let tomar = faltan.min(entrada.disponible());
                    if tomar > 0 {
                        let Some(trozo) = entrada.tomar(tomar) else {
                            return Err(datos_invalidos("ExtendedDesktopSize incompleto"));
                        };
                        datos.extend_from_slice(trozo);
                    }
                }
                if datos.len() < necesarios {
                    *self = Cuerpo::ExtendedDesktopSize {
                        x,
                        y,
                        ancho,
                        alto,
                        pantallas,
                        datos,
                        necesarios,
                    };
                    return Ok(None);
                }
                // El estado (y) solo vale cuando el cambio lo pidió el cliente.
                let denegado = u32::from(x) == 1 && u32::from(y) != 0;
                let mut pantalla_id = None;
                if !denegado {
                    for pantalla in datos.chunks_exact(16) {
                        let id = u32::from_be_bytes([
                            pantalla[0],
                            pantalla[1],
                            pantalla[2],
                            pantalla[3],
                        ]);
                        let px = u16::from_be_bytes([pantalla[4], pantalla[5]]);
                        let py = u16::from_be_bytes([pantalla[6], pantalla[7]]);
                        let pw = u16::from_be_bytes([pantalla[8], pantalla[9]]);
                        let ph = u16::from_be_bytes([pantalla[10], pantalla[11]]);
                        if pw == 0
                            || ph == 0
                            || u32::from(px) + u32::from(pw) > u32::from(ancho)
                            || u32::from(py) + u32::from(ph) > u32::from(alto)
                        {
                            return Err(datos_invalidos(
                                "pantalla de ExtendedDesktopSize fuera del escritorio",
                            ));
                        }
                        if pantalla_id.is_none() {
                            pantalla_id = Some(id);
                        }
                    }
                }
                Ok(Some(ResultadoRect::Tamano {
                    ancho,
                    alto,
                    extendido: true,
                    pantalla_id,
                    cambiar: !denegado,
                }))
            }
        }
    }
}

// ---------------------------------------------------------------- estado

/// Estado del analizador incremental.
enum Estado {
    /// Esperando el tipo del siguiente mensaje.
    Mensaje,
    /// Dentro de una FramebufferUpdate: rectángulos pendientes y, si toca, el
    /// que se está decodificando.
    Actualizacion { rects: u16, rect: Option<Cuerpo> },
}

/// Resultado de un paso del analizador.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Avance {
    /// Faltan bytes: hay que leer del socket (o esperar órdenes).
    Necesita,
    /// Se ha acumulado el tope de píxeles pendientes: toca entregarlos antes
    /// de seguir analizando.
    Tope,
}

/// Sesión RFB ya saludada.
struct Sesion {
    lector: Lector,
    escritor: Escritor,
    entrada: Entrada,
    temporal: Vec<u8>,
    estado: Estado,
    ancho: u16,
    alto: u16,
    /// El servidor anunció la extensión de teclas de QEMU.
    qemu: bool,
    /// El servidor anunció ExtendedDesktopSize (y con él SetDesktopSize).
    escritorio_extendido: bool,
    /// Identificador de la primera pantalla, para SetDesktopSize.
    pantalla_id: u32,
    /// El host tiene el portapapeles activado (opciones del escritorio).
    portapapeles: bool,
    /// El servidor soporta el portapapeles extendido (avisó con caps).
    portapapeles_extendido: bool,
    /// Ya se mandaron nuestras capacidades de portapapeles extendido.
    caps_enviadas: bool,
    /// Último texto enviado al remoto (para responder a una petición suya).
    ultimo_texto: Option<String>,
    /// La pestaña del visor está visible: se piden actualizaciones.
    visible: bool,
    /// Hay una petición de actualización sin respuesta.
    peticion_en_vuelo: bool,
    /// La próxima petición debe ser completa (incremental=0).
    peticion_completa: bool,
    ultima_peticion: Option<tokio::time::Instant>,
    botones: u8,
    raton: (u16, u16),
    rueda_x: f32,
    rueda_y: f32,
    teclas_pulsadas: Vec<(u32, String)>,
    eventos: mpsc::Sender<EventoEscritorio>,
    ordenes: mpsc::Receiver<OrdenEscritorio>,
    eventos_pendientes: Vec<EventoEscritorio>,
    /// Bytes de píxeles (y texto) que suman los eventos pendientes.
    bytes_eventos_pendientes: usize,
    /// Píxeles decodificados en la actualización en curso (presupuesto C3).
    pixeles_actualizacion: usize,
    salida_pendiente: Vec<Vec<u8>>,
    zrle: zrle::DescompresorZrle,
}

impl Sesion {
    fn nueva(
        lector: Lector,
        escritor: Escritor,
        eventos: mpsc::Sender<EventoEscritorio>,
        ordenes: mpsc::Receiver<OrdenEscritorio>,
        inicio: &seguridad::SesionIniciada,
        portapapeles: bool,
    ) -> Self {
        Self {
            lector,
            escritor,
            entrada: Entrada::nueva(),
            temporal: vec![0u8; TAMANO_TEMPORAL],
            estado: Estado::Mensaje,
            ancho: inicio.ancho,
            alto: inicio.alto,
            qemu: false,
            escritorio_extendido: false,
            pantalla_id: 0,
            portapapeles,
            portapapeles_extendido: false,
            caps_enviadas: false,
            ultimo_texto: None,
            visible: true,
            peticion_en_vuelo: false,
            peticion_completa: true,
            ultima_peticion: None,
            botones: 0,
            raton: (0, 0),
            rueda_x: 0.0,
            rueda_y: 0.0,
            teclas_pulsadas: Vec::new(),
            eventos,
            ordenes,
            eventos_pendientes: Vec::new(),
            bytes_eventos_pendientes: 0,
            pixeles_actualizacion: 0,
            salida_pendiente: Vec::new(),
            zrle: zrle::DescompresorZrle::nuevo(),
        }
    }

    /// Bucle principal: analiza lo que llegue, manda lo pendiente y espera por
    /// el socket o por una orden del visor.
    async fn correr(&mut self) -> Result<Cierre, ErrorEscritorio> {
        loop {
            let avance = self.avanzar()?;
            self.volcar_pendientes().await?;
            self.entrada.compactar();
            if avance == Avance::Tope {
                // Quedan rectángulos por analizar de la misma actualización:
                // se sigue sin esperar al socket ni pedir otra actualización.
                continue;
            }
            let ahora = tokio::time::Instant::now();
            let mut dormir = None;
            if self.visible && !self.peticion_en_vuelo {
                let instante = self
                    .ultima_peticion
                    .map(|ultima| ultima + INTERVALO_PETICION)
                    .unwrap_or(ahora);
                if instante <= ahora {
                    self.enviar_peticion().await?;
                    continue;
                }
                dormir = Some(instante);
            }
            tokio::select! {
                biased;
                orden = self.ordenes.recv() => {
                    match orden {
                        Some(orden) => self.procesar_orden(orden).await?,
                        None => return Ok(Cierre::Visor("se cerró la sesión de escritorio".to_string())),
                    }
                }
                resultado = self.lector.read(&mut self.temporal) => {
                    match resultado {
                        Ok(0) => return Ok(Cierre::Remoto("el servidor cerró la conexión".to_string())),
                        Ok(leidos) => self.entrada.anadir(&self.temporal[..leidos]),
                        Err(error) => return Err(ErrorEscritorio::Conexion(format!(
                            "se perdió la conexión con el servidor VNC: {error}"
                        ))),
                    }
                }
                _ = tokio::time::sleep_until(dormir.unwrap_or_else(tokio::time::Instant::now)), if dormir.is_some() => {}
            }
        }
    }

    /// Manda la petición de actualización del escritorio entero.
    async fn enviar_peticion(&mut self) -> Result<(), ErrorEscritorio> {
        let incremental = !self.peticion_completa;
        let mensaje = mensaje_pedir_actualizacion(incremental, self.ancho, self.alto);
        escribir(&mut self.escritor, &mensaje).await?;
        self.peticion_en_vuelo = true;
        self.peticion_completa = false;
        self.ultima_peticion = Some(tokio::time::Instant::now());
        Ok(())
    }

    /// Manda los eventos y los mensajes que ha dejado el analizador.
    async fn volcar_pendientes(&mut self) -> Result<(), ErrorEscritorio> {
        self.bytes_eventos_pendientes = 0;
        for evento in std::mem::take(&mut self.eventos_pendientes) {
            self.eventos
                .send(evento)
                .await
                .map_err(|_| ErrorEscritorio::Remoto("el visor cerró la sesión".to_string()))?;
        }
        for datos in std::mem::take(&mut self.salida_pendiente) {
            escribir(&mut self.escritor, &datos).await?;
        }
        Ok(())
    }

    /// Presupuesto de píxeles de una FramebufferUpdate: 1,5 veces el
    /// escritorio, con un mínimo de 64 MB para no rechazar actualizaciones
    /// legítimas de escritorios pequeños.
    fn presupuesto_pixeles(&self) -> usize {
        let area = usize::from(self.ancho) * usize::from(self.alto) * 4;
        area.saturating_add(area / 2).max(PRESUPUESTO_PIXELES_MIN)
    }

    /// Suma los píxeles de un rectángulo al presupuesto de la actualización;
    /// si se pasa, la actualización es hostil y la sesión cae con datos
    /// inválidos (nunca se reservan los píxeles).
    fn contar_pixeles_rect(&mut self, cabecera: &CabeceraRect) -> Result<(), ErrorEscritorio> {
        let pixeles = usize::from(cabecera.ancho) * usize::from(cabecera.alto) * 4;
        let total = self.pixeles_actualizacion.saturating_add(pixeles);
        let presupuesto = self.presupuesto_pixeles();
        if total > presupuesto {
            return Err(datos_invalidos(format!(
                "la actualización lleva más de {} MB de píxeles",
                presupuesto / (1024 * 1024)
            )));
        }
        self.pixeles_actualizacion = total;
        Ok(())
    }

    /// Encola un evento para el puente contando sus píxeles: la tanda se
    /// entrega al llegar al tope.
    fn push_evento(&mut self, evento: EventoEscritorio) {
        anotar_evento(
            &mut self.eventos_pendientes,
            &mut self.bytes_eventos_pendientes,
            evento,
        );
    }

    /// Estado tras terminar un rectángulo (o un mensaje suelto). Al terminar
    /// la actualización se da por recogida la petición pendiente.
    fn tras_rect(&mut self, rects: u16) -> Estado {
        if rects <= 1 {
            self.peticion_en_vuelo = false;
            Estado::Mensaje
        } else {
            Estado::Actualizacion {
                rects: rects - 1,
                rect: None,
            }
        }
    }

    /// Analiza todo lo que quepa del búfer. Nunca consume un mensaje a medias.
    fn avanzar(&mut self) -> Result<Avance, ErrorEscritorio> {
        loop {
            if self.bytes_eventos_pendientes >= LIMITE_PIXELES_PENDIENTES {
                return Ok(Avance::Tope);
            }
            let estado = std::mem::replace(&mut self.estado, Estado::Mensaje);
            match estado {
                Estado::Mensaje => {
                    if self.entrada.disponible() == 0 {
                        self.estado = Estado::Mensaje;
                        return Ok(Avance::Necesita);
                    }
                    let tipo = self
                        .entrada
                        .mirar(1)
                        .and_then(|b| b.first().copied())
                        .unwrap_or(0);
                    match tipo {
                        MSG_ACTUALIZACION => {
                            if self.entrada.disponible() < 4 {
                                self.estado = Estado::Mensaje;
                                return Ok(Avance::Necesita);
                            }
                            let cabecera = self.entrada.tomar(4).unwrap_or(&[]);
                            let rects = u16::from_be_bytes([
                                cabecera.get(2).copied().unwrap_or(0),
                                cabecera.get(3).copied().unwrap_or(0),
                            ]);
                            self.pixeles_actualizacion = 0;
                            if rects == 0 {
                                self.peticion_en_vuelo = false;
                                self.estado = Estado::Mensaje;
                            } else {
                                self.estado = Estado::Actualizacion { rects, rect: None };
                            }
                        }
                        MSG_CAMPANA => {
                            self.entrada.tomar(1);
                            self.estado = Estado::Mensaje;
                        }
                        MSG_TABLA_COLORES => {
                            // El formato es true color: el servidor no debería
                            // mandar tabla de colores, pero se consume sin
                            // romper el protocolo.
                            if self.entrada.disponible() < 6 {
                                self.estado = Estado::Mensaje;
                                return Ok(Avance::Necesita);
                            }
                            let cabecera = self.entrada.mirar(6).unwrap_or(&[]);
                            let colores = u16::from_be_bytes([
                                cabecera.get(4).copied().unwrap_or(0),
                                cabecera.get(5).copied().unwrap_or(0),
                            ]);
                            let total = 6 + usize::from(colores) * 6;
                            if self.entrada.disponible() < total {
                                self.estado = Estado::Mensaje;
                                return Ok(Avance::Necesita);
                            }
                            self.entrada.tomar(total);
                            self.estado = Estado::Mensaje;
                        }
                        MSG_CORTE_SERVIDOR => {
                            if self.entrada.disponible() < 8 {
                                self.estado = Estado::Mensaje;
                                return Ok(Avance::Necesita);
                            }
                            let cabecera = self.entrada.mirar(8).unwrap_or(&[]);
                            let largo = i32::from_be_bytes([
                                cabecera.get(4).copied().unwrap_or(0),
                                cabecera.get(5).copied().unwrap_or(0),
                                cabecera.get(6).copied().unwrap_or(0),
                                cabecera.get(7).copied().unwrap_or(0),
                            ]);
                            if largo >= 0 {
                                let cuantos = largo as usize;
                                if cuantos > TEXTO_MAX {
                                    return Err(datos_invalidos(
                                        "el texto del portapapeles pasa de 1 MB",
                                    ));
                                }
                                let total = 8 + cuantos;
                                if self.entrada.disponible() < total {
                                    self.estado = Estado::Mensaje;
                                    return Ok(Avance::Necesita);
                                }
                                let Some(datos) = self.entrada.tomar(total) else {
                                    self.estado = Estado::Mensaje;
                                    return Ok(Avance::Necesita);
                                };
                                if self.portapapeles {
                                    let texto: String =
                                        datos[8..].iter().map(|&byte| byte as char).collect();
                                    if !texto.is_empty() {
                                        self.eventos_pendientes
                                            .push(EventoEscritorio::Portapapeles(texto));
                                    }
                                }
                                self.estado = Estado::Mensaje;
                            } else {
                                let cuantos = largo.unsigned_abs() as usize;
                                if cuantos > CORTE_MAX {
                                    return Err(datos_invalidos(
                                        "el portapapeles del servidor pasa de 1 MB",
                                    ));
                                }
                                let total = 8 + cuantos;
                                if self.entrada.disponible() < total {
                                    self.estado = Estado::Mensaje;
                                    return Ok(Avance::Necesita);
                                }
                                let Some(datos) = self.entrada.tomar(total) else {
                                    self.estado = Estado::Mensaje;
                                    return Ok(Avance::Necesita);
                                };
                                if self.portapapeles {
                                    procesar_corte_extendido(
                                        &datos[8..],
                                        &mut self.portapapeles_extendido,
                                        &mut self.caps_enviadas,
                                        &mut self.eventos_pendientes,
                                        &mut self.bytes_eventos_pendientes,
                                        &mut self.salida_pendiente,
                                        &self.ultimo_texto,
                                    )?;
                                }
                                self.estado = Estado::Mensaje;
                            }
                        }
                        MSG_FIN_CONTINUAS => {
                            self.entrada.tomar(1);
                            self.estado = Estado::Mensaje;
                        }
                        MSG_VALLA_SERVIDOR => {
                            if self.entrada.disponible() < 9 {
                                self.estado = Estado::Mensaje;
                                return Ok(Avance::Necesita);
                            }
                            let cabecera = self.entrada.mirar(9).unwrap_or(&[]);
                            let total = 9 + usize::from(cabecera.get(8).copied().unwrap_or(0));
                            if self.entrada.disponible() < total {
                                self.estado = Estado::Mensaje;
                                return Ok(Avance::Necesita);
                            }
                            self.entrada.tomar(total);
                            self.estado = Estado::Mensaje;
                        }
                        otro => {
                            return Err(ErrorEscritorio::Protocolo(format!(
                                "mensaje del servidor desconocido: {otro}"
                            )));
                        }
                    }
                }
                Estado::Actualizacion { rects, rect: None } => {
                    if self.entrada.disponible() < 12 {
                        self.estado = Estado::Actualizacion { rects, rect: None };
                        return Ok(Avance::Necesita);
                    }
                    let Some(cabecera) = CabeceraRect::tomar(&mut self.entrada) else {
                        self.estado = Estado::Actualizacion { rects, rect: None };
                        return Ok(Avance::Necesita);
                    };
                    match self.iniciar_rect(cabecera)? {
                        Some(cuerpo) => {
                            self.estado = Estado::Actualizacion {
                                rects,
                                rect: Some(cuerpo),
                            };
                        }
                        None => {
                            self.estado = self.tras_rect(rects);
                        }
                    }
                }
                Estado::Actualizacion {
                    rects,
                    rect: Some(mut cuerpo),
                } => {
                    let resultado =
                        cuerpo.avanzar(&mut self.entrada, &mut self.zrle, self.ancho, self.alto)?;
                    match resultado {
                        Some(resultado) => {
                            self.procesar_resultado(resultado);
                            self.estado = self.tras_rect(rects);
                        }
                        None => {
                            self.estado = Estado::Actualizacion {
                                rects,
                                rect: Some(cuerpo),
                            };
                            return Ok(Avance::Necesita);
                        }
                    }
                }
            }
        }
    }

    /// Decide qué hacer con la cabecera de un rectángulo. Devuelve el cuerpo a
    /// decodificar o None si el rectángulo ya está resuelto.
    fn iniciar_rect(&mut self, cabecera: CabeceraRect) -> Result<Option<Cuerpo>, ErrorEscritorio> {
        match cabecera.codificacion {
            COD_RAW => {
                cabecera.validar_en_escritorio(self.ancho, self.alto)?;
                self.contar_pixeles_rect(&cabecera)?;
                let total = usize::from(cabecera.ancho) * usize::from(cabecera.alto);
                Ok(Some(Cuerpo::Raw {
                    x: cabecera.x,
                    y: cabecera.y,
                    ancho: cabecera.ancho,
                    alto: cabecera.alto,
                    pixeles: Vec::with_capacity(total * BYTES_POR_PIXEL),
                    restantes: total,
                }))
            }
            COD_COPYRECT => {
                cabecera.validar_en_escritorio(self.ancho, self.alto)?;
                Ok(Some(Cuerpo::CopyRect {
                    x: cabecera.x,
                    y: cabecera.y,
                    ancho: cabecera.ancho,
                    alto: cabecera.alto,
                }))
            }
            COD_HEXTILE => {
                cabecera.validar_en_escritorio(self.ancho, self.alto)?;
                self.contar_pixeles_rect(&cabecera)?;
                Ok(Some(Cuerpo::Hextile(hextile::Hextile::nuevo(
                    cabecera.x,
                    cabecera.y,
                    cabecera.ancho,
                    cabecera.alto,
                ))))
            }
            COD_ZRLE => {
                cabecera.validar_en_escritorio(self.ancho, self.alto)?;
                self.contar_pixeles_rect(&cabecera)?;
                Ok(Some(Cuerpo::Zrle(zrle::Zrle::nuevo(
                    cabecera.x,
                    cabecera.y,
                    cabecera.ancho,
                    cabecera.alto,
                ))))
            }
            COD_DESKTOP_SIZE => {
                self.procesar_desktop_size(cabecera)?;
                Ok(None)
            }
            COD_EXTENDED_DESKTOP_SIZE => {
                if cabecera.ancho == 0
                    || cabecera.alto == 0
                    || cabecera.ancho > LADO_MAX
                    || cabecera.alto > LADO_MAX
                {
                    return Err(datos_invalidos(format!(
                        "ExtendedDesktopSize de {}×{}",
                        cabecera.ancho, cabecera.alto
                    )));
                }
                Ok(Some(Cuerpo::ExtendedDesktopSize {
                    x: cabecera.x,
                    y: cabecera.y,
                    ancho: cabecera.ancho,
                    alto: cabecera.alto,
                    pantallas: None,
                    datos: Vec::new(),
                    necesarios: 0,
                }))
            }
            COD_CURSOR => self.iniciar_cursor(cabecera),
            COD_QEMU_EXTENDED_KEY => {
                // Rectángulo vacío: el servidor anuncia que admite la
                // extensión de teclas de QEMU.
                self.qemu = true;
                Ok(None)
            }
            COD_PORTAPAPELES_EXTENDIDO => Ok(None),
            otra => Err(ErrorEscritorio::Protocolo(format!(
                "codificación {otra} no admitida"
            ))),
        }
    }

    /// Prepara la decodificación de un cursor (seudocodificación -239). Un
    /// cursor de tamaño cero oculta el puntero.
    fn iniciar_cursor(
        &mut self,
        cabecera: CabeceraRect,
    ) -> Result<Option<Cuerpo>, ErrorEscritorio> {
        if cabecera.ancho == 0 || cabecera.alto == 0 {
            self.push_evento(EventoEscritorio::Cursor {
                visible: false,
                x: 0,
                y: 0,
                punto: (0, 0),
                ancho: 0,
                alto: 0,
                pixeles: Vec::new(),
            });
            return Ok(None);
        }
        if cabecera.ancho > CURSOR_MAX || cabecera.alto > CURSOR_MAX {
            return Err(datos_invalidos(format!(
                "cursor de {}×{} (máximo {CURSOR_MAX}×{CURSOR_MAX})",
                cabecera.ancho, cabecera.alto
            )));
        }
        let fila_mascara = usize::from(cabecera.ancho).div_ceil(8);
        let necesarios = usize::from(cabecera.ancho) * usize::from(cabecera.alto) * BYTES_POR_PIXEL
            + fila_mascara * usize::from(cabecera.alto);
        Ok(Some(Cuerpo::Cursor {
            x: cabecera.x,
            y: cabecera.y,
            ancho: cabecera.ancho,
            alto: cabecera.alto,
            datos: Vec::with_capacity(necesarios),
            necesarios,
        }))
    }

    /// Aplica un rectángulo de DesktopSize (-223).
    fn procesar_desktop_size(&mut self, cabecera: CabeceraRect) -> Result<(), ErrorEscritorio> {
        if cabecera.ancho == 0
            || cabecera.alto == 0
            || cabecera.ancho > LADO_MAX
            || cabecera.alto > LADO_MAX
        {
            return Err(datos_invalidos(format!(
                "DesktopSize de {}×{}",
                cabecera.ancho, cabecera.alto
            )));
        }
        if (cabecera.ancho, cabecera.alto) != (self.ancho, self.alto) {
            self.ancho = cabecera.ancho;
            self.alto = cabecera.alto;
            self.push_evento(EventoEscritorio::Tamano {
                ancho: self.ancho,
                alto: self.alto,
            });
            // Con DesktopSize el contenido del escritorio queda indefinido: se
            // pide una actualización completa.
            self.peticion_completa = true;
        }
        Ok(())
    }

    /// Guarda o emite el resultado de un rectángulo terminado.
    fn procesar_resultado(&mut self, resultado: ResultadoRect) {
        match resultado {
            ResultadoRect::Rect {
                x,
                y,
                ancho,
                alto,
                pixeles,
            } => self.push_evento(EventoEscritorio::Rect {
                x,
                y,
                ancho,
                alto,
                pixeles,
            }),
            ResultadoRect::Copia {
                origen,
                destino,
                ancho,
                alto,
            } => self.push_evento(EventoEscritorio::Copia {
                origen,
                destino,
                ancho,
                alto,
            }),
            ResultadoRect::Cursor {
                visible,
                x,
                y,
                ancho,
                alto,
                pixeles,
            } => self.push_evento(EventoEscritorio::Cursor {
                visible,
                x,
                y,
                punto: (x, y),
                ancho,
                alto,
                pixeles,
            }),
            ResultadoRect::Tamano {
                ancho,
                alto,
                extendido,
                pantalla_id,
                cambiar,
            } => {
                if extendido {
                    self.escritorio_extendido = true;
                    if let Some(id) = pantalla_id {
                        self.pantalla_id = id;
                    }
                }
                if cambiar && (ancho, alto) != (self.ancho, self.alto) {
                    self.ancho = ancho;
                    self.alto = alto;
                    self.eventos_pendientes
                        .push(EventoEscritorio::Tamano { ancho, alto });
                }
            }
        }
    }

    // -------------------------------------------------------------- órdenes

    async fn procesar_orden(&mut self, orden: OrdenEscritorio) -> Result<(), ErrorEscritorio> {
        match orden {
            OrdenEscritorio::Tecla {
                codigo,
                keysym,
                abajo,
            } => self.tecla(&codigo, keysym, abajo).await,
            OrdenEscritorio::Raton { x, y } => {
                self.raton = (
                    x.min(self.ancho.saturating_sub(1)),
                    y.min(self.alto.saturating_sub(1)),
                );
                let mensaje = mensaje_raton(self.botones, self.raton.0, self.raton.1);
                escribir(&mut self.escritor, &mensaje).await
            }
            OrdenEscritorio::Boton { boton, abajo } => {
                if !(1..=8).contains(&boton) {
                    return Ok(());
                }
                let bit = 1u8 << (boton - 1);
                if abajo {
                    self.botones |= bit;
                } else {
                    self.botones &= !bit;
                }
                let mensaje = mensaje_raton(self.botones, self.raton.0, self.raton.1);
                escribir(&mut self.escritor, &mensaje).await
            }
            OrdenEscritorio::Rueda { dx, dy } => self.rueda(dx, dy).await,
            OrdenEscritorio::SoltarTodo => {
                let pulsadas = std::mem::take(&mut self.teclas_pulsadas);
                for (keysym, codigo) in pulsadas.into_iter().rev() {
                    self.tecla(&codigo, keysym, false).await?;
                }
                Ok(())
            }
            OrdenEscritorio::Tamano { ancho, alto } => {
                if !self.escritorio_extendido {
                    // Sin ExtendedDesktopSize el visor escala: se ignora.
                    return Ok(());
                }
                if ancho < LADO_MIN_VISOR
                    || alto < LADO_MIN_VISOR
                    || ancho > LADO_MAX
                    || alto > LADO_MAX
                    || (ancho, alto) == (self.ancho, self.alto)
                {
                    return Ok(());
                }
                let mensaje = mensaje_cambiar_tamano(ancho, alto, self.pantalla_id);
                escribir(&mut self.escritor, &mensaje).await
            }
            OrdenEscritorio::Portapapeles(texto) => {
                if !self.portapapeles || texto.len() > TEXTO_MAX {
                    return Ok(());
                }
                if self.portapapeles_extendido {
                    let mensaje = mensaje_corte_provision_texto(&texto);
                    self.ultimo_texto = Some(texto);
                    escribir(&mut self.escritor, &mensaje).await
                } else {
                    let mensaje = mensaje_corte_latin1(&texto);
                    self.ultimo_texto = Some(texto);
                    escribir(&mut self.escritor, &mensaje).await
                }
            }
            OrdenEscritorio::PedirPortapapeles => {
                if self.portapapeles && self.portapapeles_extendido {
                    let mensaje = mensaje_corte_peticion();
                    escribir(&mut self.escritor, &mensaje).await
                } else {
                    Ok(())
                }
            }
            OrdenEscritorio::Visible(visible) => {
                self.visible = visible;
                if visible {
                    // Al volver a la pestaña se pide el escritorio entero.
                    self.peticion_completa = true;
                }
                Ok(())
            }
            OrdenEscritorio::CtrlAltSupr => {
                let secuencia = [
                    ("ControlLeft", 0xFFE3),
                    ("AltLeft", 0xFFE9),
                    ("Delete", 0xFFFF),
                ];
                for (codigo, keysym) in secuencia {
                    self.tecla(codigo, keysym, true).await?;
                }
                for (codigo, keysym) in secuencia.into_iter().rev() {
                    self.tecla(codigo, keysym, false).await?;
                }
                Ok(())
            }
            OrdenEscritorio::TeclaWindows => {
                self.tecla("MetaLeft", 0xFFEB, true).await?;
                self.tecla("MetaLeft", 0xFFEB, false).await
            }
            OrdenEscritorio::Reconectar => Err(ErrorEscritorio::Remoto(
                "el visor pidió reconectar la sesión".to_string(),
            )),
        }
    }

    async fn tecla(
        &mut self,
        codigo: &str,
        keysym: u32,
        abajo: bool,
    ) -> Result<(), ErrorEscritorio> {
        if abajo {
            if !self.teclas_pulsadas.iter().any(|(otro, _)| *otro == keysym) {
                self.teclas_pulsadas.push((keysym, codigo.to_string()));
            }
        } else {
            self.teclas_pulsadas.retain(|(otro, _)| *otro != keysym);
        }
        if self.qemu {
            if let Some((scancode, extendida)) = teclas::scancode(codigo) {
                let keycode = if extendida {
                    0x80 | u32::from(scancode)
                } else {
                    u32::from(scancode)
                };
                return escribir(
                    &mut self.escritor,
                    &mensaje_tecla_qemu(abajo, keysym, keycode),
                )
                .await;
            }
        }
        escribir(&mut self.escritor, &mensaje_tecla(abajo, keysym)).await
    }

    /// Rueda del ratón: botones 4 (arriba), 5 (abajo), 6 (izquierda) y 7
    /// (derecha), un clic por unidad de desplazamiento.
    async fn rueda(&mut self, dx: f32, dy: f32) -> Result<(), ErrorEscritorio> {
        if dx.is_finite() {
            self.rueda_x = (self.rueda_x + dx).clamp(-64.0, 64.0);
        }
        if dy.is_finite() {
            self.rueda_y = (self.rueda_y + dy).clamp(-64.0, 64.0);
        }
        let mut clics: Vec<u8> = Vec::new();
        if self.rueda_y >= 1.0 {
            let cantidad = self.rueda_y.floor().min(32.0) as u32;
            self.rueda_y -= cantidad as f32;
            clics.extend(std::iter::repeat_n(5u8, cantidad as usize));
        } else if self.rueda_y <= -1.0 {
            let cantidad = (-self.rueda_y).floor().min(32.0) as u32;
            self.rueda_y += cantidad as f32;
            clics.extend(std::iter::repeat_n(4u8, cantidad as usize));
        }
        if self.rueda_x >= 1.0 {
            let cantidad = self.rueda_x.floor().min(32.0) as u32;
            self.rueda_x -= cantidad as f32;
            clics.extend(std::iter::repeat_n(7u8, cantidad as usize));
        } else if self.rueda_x <= -1.0 {
            let cantidad = (-self.rueda_x).floor().min(32.0) as u32;
            self.rueda_x += cantidad as f32;
            clics.extend(std::iter::repeat_n(6u8, cantidad as usize));
        }
        for boton in clics {
            let bit = 1u8 << (boton - 1);
            let pulsado = mensaje_raton(self.botones | bit, self.raton.0, self.raton.1);
            escribir(&mut self.escritor, &pulsado).await?;
            let soltado = mensaje_raton(self.botones, self.raton.0, self.raton.1);
            escribir(&mut self.escritor, &soltado).await?;
        }
        Ok(())
    }
}

// ------------------------------------------------------------ portapapeles

/// Encola un evento y suma los bytes que ocupa (píxeles RGBA o texto).
fn anotar_evento(eventos: &mut Vec<EventoEscritorio>, bytes: &mut usize, evento: EventoEscritorio) {
    let tamano = match &evento {
        EventoEscritorio::Rect { pixeles, .. } | EventoEscritorio::Cursor { pixeles, .. } => {
            pixeles.len()
        }
        EventoEscritorio::Video { regiones, h264, .. } => h264.len() + regiones.len() * 8,
        EventoEscritorio::Portapapeles(texto) => texto.len(),
        _ => 0,
    };
    *bytes = bytes.saturating_add(tamano);
    eventos.push(evento);
}

/// Procesa un mensaje de portapapeles extendido (longitud negativa). El
/// servidor es entrada no fiable: los tamaños se comprueban antes de indexar.
fn procesar_corte_extendido(
    cuerpo: &[u8],
    soportado: &mut bool,
    caps_enviadas: &mut bool,
    eventos: &mut Vec<EventoEscritorio>,
    bytes_eventos: &mut usize,
    escrituras: &mut Vec<Vec<u8>>,
    ultimo_texto: &Option<String>,
) -> Result<(), ErrorEscritorio> {
    let Some(banderas) = cuerpo
        .get(..4)
        .map(|bytes| u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    else {
        return Err(datos_invalidos(
            "mensaje de portapapeles extendido sin banderas",
        ));
    };
    let resto = cuerpo.get(4..).unwrap_or(&[]);
    let formatos = banderas & 0xFFFF;
    if banderas & CORTE_CAPS != 0 {
        *soportado = true;
        let cuantos = formatos.count_ones() as usize;
        if resto.len() < cuantos * 4 {
            return Err(datos_invalidos(
                "capacidades de portapapeles extendido truncadas",
            ));
        }
        if !*caps_enviadas {
            *caps_enviadas = true;
            escrituras.push(mensaje_corte_caps(TEXTO_MAX as u32));
        }
        return Ok(());
    }
    if banderas & CORTE_AVISO != 0 {
        if banderas & CORTE_TEXTO != 0 {
            escrituras.push(mensaje_corte_peticion());
        }
        return Ok(());
    }
    if banderas & CORTE_PETICION != 0 {
        anotar_evento(eventos, bytes_eventos, EventoEscritorio::PedirPortapapeles);
        let texto = ultimo_texto.as_deref().unwrap_or("");
        if texto.len() <= TEXTO_MAX {
            escrituras.push(mensaje_corte_provision_texto(texto));
        }
        return Ok(());
    }
    if banderas & CORTE_PROVISION == 0 {
        return Ok(());
    }
    let limite = TEXTO_MAX + 64 * 1024;
    let descomprimido = descomprimir_acotado(resto, limite)?;
    let mut posicion = 0usize;
    for formato in 0..16u32 {
        if formatos & (1u32 << formato) == 0 {
            continue;
        }
        let Some(cabecera) = descomprimido.get(posicion..posicion + 4) else {
            return Err(datos_invalidos("provisión del portapapeles truncada"));
        };
        let tamano =
            u32::from_be_bytes([cabecera[0], cabecera[1], cabecera[2], cabecera[3]]) as usize;
        posicion += 4;
        let Some(datos) = descomprimido.get(posicion..posicion + tamano) else {
            return Err(datos_invalidos("provisión del portapapeles truncada"));
        };
        posicion += tamano;
        if formato == 0 {
            if tamano > TEXTO_MAX {
                return Err(datos_invalidos("el texto del portapapeles pasa de 1 MB"));
            }
            let texto = String::from_utf8_lossy(datos);
            let texto = texto.trim_end_matches('\0');
            if !texto.is_empty() {
                anotar_evento(
                    eventos,
                    bytes_eventos,
                    EventoEscritorio::Portapapeles(texto.to_string()),
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn una_cabecera_de_rectangulo_se_lee_entera() {
        let mut entrada = Entrada::nueva();
        entrada.anadir(&[0, 10, 0, 20, 0, 3, 0, 4, 0, 0, 0, 16, 0xFF]);
        let cabecera = CabeceraRect::tomar(&mut entrada).unwrap();
        assert_eq!(cabecera.x, 10);
        assert_eq!(cabecera.y, 20);
        assert_eq!(cabecera.ancho, 3);
        assert_eq!(cabecera.alto, 4);
        assert_eq!(cabecera.codificacion, 16);
        assert_eq!(entrada.disponible(), 1);
    }

    #[test]
    fn un_rectangulo_fuera_del_escritorio_es_invalido() {
        let cabecera = CabeceraRect {
            x: 90,
            y: 0,
            ancho: 20,
            alto: 10,
            codificacion: COD_RAW,
        };
        assert!(cabecera.validar_en_escritorio(100, 100).is_err());
        assert_eq!(
            cabecera.validar_en_escritorio(100, 100).unwrap_err().motivo(),
            "datos inválidos del servidor: rectángulo en (90, 0) de 20×10 fuera del escritorio 100×100"
        );
    }

    #[test]
    fn el_portapapeles_extendido_de_caps_responde_con_las_nuestras() {
        let mut soportado = false;
        let mut caps_enviadas = false;
        let mut eventos = Vec::new();
        let mut bytes_eventos = 0usize;
        let mut escrituras = Vec::new();
        let mut cuerpo = (CORTE_CAPS | CORTE_TEXTO).to_be_bytes().to_vec();
        cuerpo.extend_from_slice(&[0u8; 4]);
        procesar_corte_extendido(
            &cuerpo,
            &mut soportado,
            &mut caps_enviadas,
            &mut eventos,
            &mut bytes_eventos,
            &mut escrituras,
            &None,
        )
        .unwrap();
        assert!(soportado);
        assert!(caps_enviadas);
        assert_eq!(escrituras.len(), 1);
        assert!(eventos.is_empty());
    }

    /// Sesión sin red para probar el analizador: el otro extremo del dúplex no
    /// se usa.
    fn sesion_sin_red(ancho: u16, alto: u16) -> Sesion {
        let (servidor, _cliente) = tokio::io::duplex(64 * 1024);
        let flujo: crate::escritorio::Flujo = Box::new(servidor);
        let (lector, escritor) = tokio::io::split(flujo);
        let (eventos_tx, _eventos_rx) = mpsc::channel(8);
        let (_ordenes_tx, ordenes_rx) = mpsc::channel(8);
        let inicio = seguridad::SesionIniciada { ancho, alto };
        Sesion::nueva(lector, escritor, eventos_tx, ordenes_rx, &inicio, true)
    }

    /// Construye un FramebufferUpdate con muchos rectángulos ZRLE de un solo
    /// color, todos del mismo tamaño, con el zlib de la sesión (una sola
    /// secuencia comprimida).
    fn marco_zrle_solidos(cuantos: usize, lado: u16) -> Vec<u8> {
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write as _;

        let tiles = usize::from(lado.div_ceil(64));
        let mut codificador = ZlibEncoder::new(Vec::new(), Compression::default());
        let mut anterior = 0usize;
        let mut rectangulos = Vec::new();
        for _ in 0..cuantos {
            let mut crudo = Vec::new();
            for _ in 0..(tiles * tiles) {
                // Subcodificación 1 (color sólido) y un CPIXEL.
                crudo.extend_from_slice(&[1u8, 10, 20, 30]);
            }
            codificador.write_all(&crudo).expect("comprimir ZRLE");
            codificador.flush().expect("vaciar zlib");
            let todos = codificador.get_mut();
            let comprimido = todos[anterior..].to_vec();
            anterior = todos.len();
            let mut rectangulo = Vec::new();
            rectangulo.extend_from_slice(&0u16.to_be_bytes());
            rectangulo.extend_from_slice(&0u16.to_be_bytes());
            rectangulo.extend_from_slice(&lado.to_be_bytes());
            rectangulo.extend_from_slice(&lado.to_be_bytes());
            rectangulo.extend_from_slice(&COD_ZRLE.to_be_bytes());
            rectangulo.extend_from_slice(&(comprimido.len() as u32).to_be_bytes());
            rectangulo.extend_from_slice(&comprimido);
            rectangulos.push(rectangulo);
        }
        let mut marco = vec![0u8, 0];
        marco.extend_from_slice(&(cuantos as u16).to_be_bytes());
        for rectangulo in &rectangulos {
            marco.extend_from_slice(rectangulo);
        }
        marco
    }

    fn drenar_rects(sesion: &mut Sesion) -> usize {
        let eventos = std::mem::take(&mut sesion.eventos_pendientes);
        let mut rects = 0usize;
        for evento in eventos {
            if matches!(evento, EventoEscritorio::Rect { .. }) {
                rects += 1;
            }
        }
        sesion.bytes_eventos_pendientes = 0;
        rects
    }

    /// C3: un FramebufferUpdate con muchos rectángulos grandes se analiza y
    /// entrega por tandas; la memoria pendiente nunca pasa del tope.
    #[test]
    fn un_fotograma_con_muchos_rectangulos_se_entrega_por_tandas() {
        // Escritorio 4096×4096 (presupuesto de 100 MB): 20 rectángulos de
        // 1024×1024 suman 80 MB, por debajo del presupuesto pero muy por encima
        // del tope de tanda de 16 MB, así que se entregan por tandas.
        let mut sesion = sesion_sin_red(4096, 4096);
        let cuantos = 20usize;
        sesion.entrada.anadir(&marco_zrle_solidos(cuantos, 1024));

        let mut entregados = 0usize;
        let mut tandas = 0usize;
        let mut maximo = 0usize;
        loop {
            maximo = maximo.max(sesion.bytes_eventos_pendientes);
            match sesion.avanzar().expect("analizar el fotograma") {
                Avance::Tope => {
                    tandas += 1;
                    assert!(
                        sesion.bytes_eventos_pendientes >= LIMITE_PIXELES_PENDIENTES,
                        "se entrega antes de llegar al tope"
                    );
                    // Como mucho un rectángulo por encima del tope.
                    assert!(
                        sesion.bytes_eventos_pendientes
                            <= LIMITE_PIXELES_PENDIENTES + 4 * 1024 * 1024
                    );
                    entregados += drenar_rects(&mut sesion);
                }
                Avance::Necesita => break,
            }
        }
        entregados += drenar_rects(&mut sesion);
        assert_eq!(entregados, cuantos, "se entregan todos los rectángulos");
        assert!(
            tandas >= 2,
            "el fotograma debe entregarse por tandas: {tandas}"
        );
        assert!(maximo <= LIMITE_PIXELES_PENDIENTES + 4 * 1024 * 1024);
    }

    /// C3: una actualización con más píxeles que el presupuesto (1,5 veces el
    /// escritorio) se rechaza como datos inválidos antes de reservarlos.
    #[test]
    fn rechaza_una_actualizacion_que_pasa_del_presupuesto() {
        // Escritorio 1024×1024: presupuesto de 64 MB (el mínimo). 40
        // rectángulos de 4 MB suman 160 MB.
        let mut sesion = sesion_sin_red(1024, 1024);
        sesion.entrada.anadir(&marco_zrle_solidos(40, 1024));
        let mut error = None;
        loop {
            match sesion.avanzar() {
                Ok(Avance::Tope) => {
                    let _ = drenar_rects(&mut sesion);
                }
                Ok(Avance::Necesita) => break,
                Err(devuelto) => {
                    error = Some(devuelto);
                    break;
                }
            }
        }
        let error = error.expect("la actualización debe rechazarse como inválida");
        assert!(matches!(error, ErrorEscritorio::Datos(_)));
        assert!(error.motivo().contains("datos inválidos del servidor"));
        assert!(error.motivo().contains("MB de píxeles"));
    }
}
