//! Saludo, negociación de versión y seguridad del cliente RFB (RFC 6143).
//!
//! Versiones 3.3, 3.7 y 3.8; seguridad None (1) y VNC Authentication (2). El
//! fallo de autenticación cierra la conexión (RFB no permite reintentar sobre
//! el mismo flujo): el cliente devuelve ErrorEscritorio::Autenticacion y la
//! capa de sesión reabre el camino y vuelve a llamar a abrir con el siguiente
//! intento.

use std::time::Duration;

use crate::escritorio::{Dialogo, ErrorEscritorio, PeticionCredenciales, TipoHost};

use super::protocolo::{datos_invalidos, Version, LADO_MAX, NOMBRE_MAX};

/// Plazo de cada paso del saludo y de la negociación (no cubre los diálogos).
const PLAZO: Duration = Duration::from_secs(10);

/// Lector y escritor separados del flujo del escritorio.
pub type Lector = tokio::io::ReadHalf<crate::escritorio::Flujo>;
/// Escritor separado del flujo del escritorio.
pub type Escritor = tokio::io::WriteHalf<crate::escritorio::Flujo>;

// ------------------------------------------------------------- entrada/salida

async fn leer_exacto(lector: &mut Lector, destino: &mut [u8]) -> Result<(), ErrorEscritorio> {
    match tokio::time::timeout(PLAZO, tokio::io::AsyncReadExt::read_exact(lector, destino)).await {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(error)) => Err(ErrorEscritorio::Conexion(format!(
            "no se pudo leer del servidor VNC: {error}"
        ))),
        Err(_) => Err(ErrorEscritorio::Conexion(
            "el servidor VNC no respondió en 10 s".to_string(),
        )),
    }
}

async fn escribir(escritor: &mut Escritor, datos: &[u8]) -> Result<(), ErrorEscritorio> {
    match tokio::time::timeout(PLAZO, tokio::io::AsyncWriteExt::write_all(escritor, datos)).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(ErrorEscritorio::Conexion(format!(
            "no se pudo escribir al servidor VNC: {error}"
        ))),
        Err(_) => Err(ErrorEscritorio::Conexion(
            "el servidor VNC no respondió en 10 s".to_string(),
        )),
    }
}

async fn leer_u8(lector: &mut Lector) -> Result<u8, ErrorEscritorio> {
    let mut byte = [0u8; 1];
    leer_exacto(lector, &mut byte).await?;
    Ok(byte[0])
}

async fn leer_u16(lector: &mut Lector) -> Result<u16, ErrorEscritorio> {
    let mut bytes = [0u8; 2];
    leer_exacto(lector, &mut bytes).await?;
    Ok(u16::from_be_bytes(bytes))
}

async fn leer_u32(lector: &mut Lector) -> Result<u32, ErrorEscritorio> {
    let mut bytes = [0u8; 4];
    leer_exacto(lector, &mut bytes).await?;
    Ok(u32::from_be_bytes(bytes))
}

/// Lee una cadena del protocolo (longitud de 32 bits y bytes) con tope. El
/// texto del servidor es entrada no fiable: se devuelve con UTF-8 tolerante y
/// sin caracteres de control.
async fn leer_cadena(lector: &mut Lector, tope: usize) -> Result<String, ErrorEscritorio> {
    let longitud = leer_u32(lector).await? as usize;
    if longitud > tope {
        return Err(datos_invalidos(format!(
            "el servidor manda una cadena de {longitud} bytes"
        )));
    }
    let mut datos = vec![0u8; longitud];
    leer_exacto(lector, &mut datos).await?;
    Ok(limpiar_texto(&String::from_utf8_lossy(&datos)))
}

/// Quita caracteres de control y recorta: el texto del servidor se enseña en
/// la interfaz y no debe romperla.
fn limpiar_texto(texto: &str) -> String {
    texto
        .chars()
        .filter(|caracter| !caracter.is_control())
        .take(200)
        .collect()
}

// ------------------------------------------------------------------ versión

/// Interpreta el saludo de 12 bytes del servidor.
fn interpretar_saludo(saludo: &[u8; 12]) -> Result<(u32, u32), ErrorEscritorio> {
    let digitos = |bytes: &[u8]| bytes.iter().all(u8::is_ascii_digit);
    let bien_formado = saludo.starts_with(b"RFB ")
        && saludo.get(7) == Some(&b'.')
        && digitos(&saludo[4..7])
        && digitos(&saludo[8..11])
        && saludo[11] == b'\n';
    if !bien_formado {
        return Err(ErrorEscritorio::Protocolo(format!(
            "el servidor no habla RFB (saludo: {:?})",
            String::from_utf8_lossy(saludo)
        )));
    }
    let mayor = std::str::from_utf8(&saludo[4..7])
        .ok()
        .and_then(|texto| texto.parse::<u32>().ok())
        .ok_or_else(|| ErrorEscritorio::Protocolo("saludo RFB inválido".to_string()))?;
    let menor = std::str::from_utf8(&saludo[8..11])
        .ok()
        .and_then(|texto| texto.parse::<u32>().ok())
        .ok_or_else(|| ErrorEscritorio::Protocolo("saludo RFB inválido".to_string()))?;
    Ok((mayor, menor))
}

/// Elige la versión que se va a usar: la más alta que el servidor ofrezca y
/// que este cliente admita (3.3, 3.7 o 3.8).
fn elegir_version(mayor: u32, menor: u32) -> Result<Version, ErrorEscritorio> {
    if mayor != 3 {
        return Err(ErrorEscritorio::Protocolo(format!(
            "versión RFB {mayor}.{menor} no admitida"
        )));
    }
    let version = match menor {
        7 => Version::V37,
        valor if valor >= 8 => Version::V38,
        _ => Version::V33,
    };
    Ok(version)
}

/// Negocia la versión del protocolo.
async fn negociar_version(
    lector: &mut Lector,
    escritor: &mut Escritor,
) -> Result<Version, ErrorEscritorio> {
    let mut saludo = [0u8; 12];
    leer_exacto(lector, &mut saludo).await?;
    let (mayor, menor) = interpretar_saludo(&saludo)?;
    let version = elegir_version(mayor, menor)?;
    escribir(escritor, version.saludo()).await?;
    Ok(version)
}

// ------------------------------------------------------------------ seguridad

/// Seguridad acordada con el servidor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seguridad {
    Ninguna,
    Vnc,
}

fn seguridad_no_admitida(numeros: &[u8]) -> ErrorEscritorio {
    let lista = numeros
        .iter()
        .map(|numero| numero.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    ErrorEscritorio::Protocolo(format!(
        "el servidor pide la seguridad {lista}: no admitida"
    ))
}

async fn negociar_seguridad(
    version: Version,
    lector: &mut Lector,
    escritor: &mut Escritor,
) -> Result<Seguridad, ErrorEscritorio> {
    match version {
        Version::V33 => {
            let tipo = leer_u32(lector).await?;
            match tipo {
                0 => {
                    let motivo = leer_cadena(lector, NOMBRE_MAX).await?;
                    Err(ErrorEscritorio::Protocolo(format!(
                        "el servidor rechazó la conexión VNC: {motivo}"
                    )))
                }
                1 => Ok(Seguridad::Ninguna),
                2 => Ok(Seguridad::Vnc),
                _ => Err(seguridad_no_admitida(&[tipo as u8])),
            }
        }
        Version::V37 | Version::V38 => {
            let numero = leer_u8(lector).await?;
            if numero == 0 {
                let motivo = leer_cadena(lector, NOMBRE_MAX).await?;
                return Err(ErrorEscritorio::Protocolo(format!(
                    "el servidor rechazó la conexión VNC: {motivo}"
                )));
            }
            let mut lista = vec![0u8; numero as usize];
            leer_exacto(lector, &mut lista).await?;
            // Se prefiere VNC Authentication (contraseña) y, si no, None.
            let elegida = if lista.contains(&2) {
                Some((2u8, Seguridad::Vnc))
            } else if lista.contains(&1) {
                Some((1u8, Seguridad::Ninguna))
            } else {
                None
            };
            let Some((elegida, seguridad)) = elegida else {
                return Err(seguridad_no_admitida(&lista));
            };
            escribir(escritor, &[elegida]).await?;
            Ok(seguridad)
        }
    }
}

/// Invierte los bits de un byte: VNC usa el orden inverso al de DES habitual
/// (el bit menos significativo es el primero y el de paridad se descarta).
pub fn invertir_bits(byte: u8) -> u8 {
    byte.reverse_bits()
}

/// Respuesta a un reto de 16 bytes de VNC Authentication: clave de hasta 8
/// bytes con los bits invertidos y DES en modo ECB (dos bloques).
pub fn respuesta_vnc(reto: &[u8; 16], contrasena: &str) -> [u8; 16] {
    use des::cipher::{Block, BlockCipherEncrypt, KeyInit};

    let mut clave = [0u8; 8];
    for (posicion, byte) in contrasena.as_bytes().iter().take(8).enumerate() {
        clave[posicion] = invertir_bits(*byte);
    }
    let Ok(cifrador) = des::Des::new_from_slice(&clave) else {
        // Una clave de 8 bytes siempre vale para DES; si no, se devuelve una
        // respuesta vacía en vez de entrar en pánico (no se puede provocar
        // desde el servidor).
        return [0u8; 16];
    };
    let mut salida = [0u8; 16];
    for bloque in 0..2 {
        let mut datos: [u8; 8] = [0u8; 8];
        datos.copy_from_slice(&reto[bloque * 8..bloque * 8 + 8]);
        let mut cifrado = Block::<des::Des>::from(datos);
        cifrador.encrypt_block(&mut cifrado);
        salida[bloque * 8..bloque * 8 + 8].copy_from_slice(&cifrado[..]);
    }
    salida
}

/// Autenticación VNC: reto, respuesta con la contraseña pedida al cliente y
/// resultado de seguridad.
async fn autenticar_vnc(
    version: Version,
    lector: &mut Lector,
    escritor: &mut Escritor,
    dialogo: &Dialogo,
    intento: u8,
) -> Result<(), ErrorEscritorio> {
    let mut reto = [0u8; 16];
    leer_exacto(lector, &mut reto).await?;
    let intento = intento.clamp(1, 3);
    let credenciales = dialogo
        .credenciales(PeticionCredenciales {
            tipo: TipoHost::Vnc,
            usuario: None,
            dominio: None,
            intento,
        })
        .await?;
    let Some(credenciales) = credenciales else {
        return Err(ErrorEscritorio::Cancelado(
            "cancelado: no se pidió la contraseña de VNC".to_string(),
        ));
    };
    let respuesta = respuesta_vnc(&reto, &credenciales.contrasena);
    escribir(escritor, &respuesta).await?;
    let resultado = leer_u32(lector).await?;
    if resultado == 0 {
        return Ok(());
    }
    let motivo = if version.motivo_en_fallo() {
        leer_cadena(lector, NOMBRE_MAX).await.unwrap_or_default()
    } else {
        String::new()
    };
    let detalle = if motivo.is_empty() {
        format!("contraseña incorrecta (intento {intento} de 3)")
    } else {
        format!("contraseña incorrecta (intento {intento} de 3): {motivo}")
    };
    if resultado == 1 || resultado == 2 {
        Err(ErrorEscritorio::Autenticacion(detalle))
    } else {
        Err(ErrorEscritorio::Protocolo(format!(
            "resultado de seguridad desconocido: {resultado}"
        )))
    }
}

/// Elige seguridad, autentica y espera al resultado. Devuelve false si no hizo
/// falta autenticarse.
async fn negociar_y_autenticar(
    version: Version,
    lector: &mut Lector,
    escritor: &mut Escritor,
    dialogo: &Dialogo,
    intento: u8,
) -> Result<(), ErrorEscritorio> {
    let seguridad = negociar_seguridad(version, lector, escritor).await?;
    match seguridad {
        Seguridad::Ninguna => {
            if version.resultado_sin_seguridad() {
                let resultado = leer_u32(lector).await?;
                if resultado != 0 {
                    let motivo = leer_cadena(lector, NOMBRE_MAX).await.unwrap_or_default();
                    return Err(ErrorEscritorio::Protocolo(format!(
                        "el servidor rechazó la conexión VNC: {motivo}"
                    )));
                }
            }
            Ok(())
        }
        Seguridad::Vnc => autenticar_vnc(version, lector, escritor, dialogo, intento).await,
    }
}

// ------------------------------------------------------------ inicialización

/// Resultado del saludo: tamaño del escritorio que anuncia el servidor.
#[derive(Debug, Clone, Copy)]
pub struct SesionIniciada {
    pub ancho: u16,
    pub alto: u16,
}

/// Hace el saludo completo (versión, seguridad, ClientInit y ServerInit).
/// Devuelve el tamaño inicial validado. No manda SetPixelFormat ni
/// SetEncodings: de eso se encarga el bucle de la sesión.
pub async fn iniciar(
    lector: &mut Lector,
    escritor: &mut Escritor,
    dialogo: &Dialogo,
    intento: u8,
) -> Result<SesionIniciada, ErrorEscritorio> {
    let version = negociar_version(lector, escritor).await?;
    negociar_y_autenticar(version, lector, escritor, dialogo, intento).await?;
    // ClientInit con shared-flag=1: abrir MAGI no echa a otros visores.
    escribir(escritor, &[1]).await?;
    // ServerInit: ancho, alto, formato natural del servidor (se ignora: se
    // pide el formato fijo con SetPixelFormat) y nombre.
    let ancho = leer_u16(lector).await?;
    let alto = leer_u16(lector).await?;
    if ancho == 0 || alto == 0 || ancho > LADO_MAX || alto > LADO_MAX {
        return Err(datos_invalidos(format!(
            "escritorio de {ancho}×{alto} (tamaño no válido)"
        )));
    }
    let mut formato = [0u8; 16];
    leer_exacto(lector, &mut formato).await?;
    let longitud_nombre = leer_u32(lector).await? as usize;
    if longitud_nombre > NOMBRE_MAX {
        return Err(datos_invalidos(format!(
            "nombre del escritorio de {longitud_nombre} bytes"
        )));
    }
    let mut nombre = vec![0u8; longitud_nombre];
    leer_exacto(lector, &mut nombre).await?;
    Ok(SesionIniciada { ancho, alto })
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_vector_conocido_de_vnc_coincide() {
        // Vector calculado con una implementación independiente (OpenSSL
        // 3.x, cifrado DES-ECB): contraseña "password", clave con los bits de
        // cada byte invertidos y reto 00 01 … 0f.
        let reto: [u8; 16] = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f,
        ];
        assert_eq!(
            &texto_hex(&respuesta_vnc(&reto, "password")),
            "b866924125c8eebb9debc1db61c538e2"
        );
        assert_eq!(invertir_bits(0x70), 0x0e);
        assert_eq!(invertir_bits(0x61), 0x86);
    }

    fn texto_hex(bytes: &[u8]) -> String {
        let mut salida = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            salida.push_str(&format!("{byte:02x}"));
        }
        salida
    }

    #[test]
    fn las_versiones_se_eligen_bien() {
        assert_eq!(elegir_version(3, 3).unwrap(), Version::V33);
        assert_eq!(elegir_version(3, 5).unwrap(), Version::V33);
        assert_eq!(elegir_version(3, 7).unwrap(), Version::V37);
        assert_eq!(elegir_version(3, 8).unwrap(), Version::V38);
        assert_eq!(
            elegir_version(4, 0).unwrap_err().motivo(),
            "versión RFB 4.0 no admitida"
        );
    }

    #[test]
    fn el_texto_del_servidor_se_limpia() {
        assert_eq!(limpiar_texto("hola\u{0}mundo\n"), "holamundo");
        assert!(limpiar_texto(&"a".repeat(500)).len() <= 200);
    }
}
