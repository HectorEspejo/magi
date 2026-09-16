//! SOCKS5 mínimo para los túneles dinámicos (`-D`): solo `CONNECT`, sin
//! autenticación, con destinos IPv4, IPv6 y dominio. `BIND` y `UDP ASSOCIATE`
//! no tienen sentido sobre un canal `direct-tcpip`, así que se contestan con
//! «orden no soportada» (0x07).
//!
//! El nombre de dominio se envía al host **sin resolver en local**: quien
//! resuelve es el host remoto, como hace `ssh -D`.

use std::net::{Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tracing::debug;

/// Plazo del saludo y de la petición de un cliente SOCKS.
pub const PLAZO_SALUDO: Duration = Duration::from_secs(10);

const VERSION: u8 = 0x05;
const METODO_SIN_AUTH: u8 = 0x00;
const METODO_NO_ACEPTABLE: u8 = 0xFF;

const COMANDO_CONNECT: u8 = 0x01;
const TIPO_IPV4: u8 = 0x01;
const TIPO_DOMINIO: u8 = 0x03;
const TIPO_IPV6: u8 = 0x04;

/// «Conexión establecida».
pub const OK: u8 = 0x00;
/// El host no quiso abrir el canal: «conexión rechazada» para el cliente.
const RECHAZADA: u8 = 0x05;
const ORDEN_NO_SOPORTADA: u8 = 0x07;

/// Qué ha pedido el cliente: a dónde hay que abrir el canal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Peticion {
    Conectar(String, u16),
    /// El cliente pidió algo que sobre `direct-tcpip` no se puede hacer.
    NoSoportada,
    /// El cliente habló mal o se fue a medias.
    Invalida,
}

/// Saludo y petición. Devuelve el destino tal cual lo pidió el cliente (el
/// nombre de dominio sin resolver).
pub async fn negociar(flujo: &mut TcpStream) -> Peticion {
    // Saludo: versión, número de métodos y los métodos.
    let mut cabecera = [0u8; 2];
    if timeout(flujo.read_exact(&mut cabecera)).await.is_err() {
        return Peticion::Invalida;
    }
    if cabecera[0] != VERSION {
        return Peticion::Invalida;
    }
    let mut metodos = vec![0u8; cabecera[1] as usize];
    if timeout(flujo.read_exact(&mut metodos)).await.is_err() {
        return Peticion::Invalida;
    }
    let metodo = if metodos.contains(&METODO_SIN_AUTH) {
        METODO_SIN_AUTH
    } else {
        METODO_NO_ACEPTABLE
    };
    // Solo se admite sin autenticación: por eso el túnel escucha en loopback
    // salvo que el usuario pida otra cosa a sabiendas.
    if timeout(flujo.write_all(&[VERSION, metodo])).await.is_err() || metodo == METODO_NO_ACEPTABLE
    {
        return Peticion::Invalida;
    }

    // Petición: versión, comando, reservado, tipo de dirección y dirección.
    let mut peticion = [0u8; 4];
    if timeout(flujo.read_exact(&mut peticion)).await.is_err() {
        return Peticion::Invalida;
    }
    if peticion[0] != VERSION {
        return Peticion::Invalida;
    }
    let destino = match leer_destino(flujo, peticion[3]).await {
        Some(destino) => destino,
        None => return Peticion::Invalida,
    };
    if peticion[1] != COMANDO_CONNECT {
        let _ = responder(flujo, ORDEN_NO_SOPORTADA).await;
        return Peticion::NoSoportada;
    }
    Peticion::Conectar(destino.0, destino.1)
}

/// Contesta al cliente. `codigo` 0x00 es «conexión establecida».
pub async fn responder(flujo: &mut TcpStream, codigo: u8) -> std::io::Result<()> {
    // El campo de dirección de la respuesta va vacío (0.0.0.0:0), que es lo que
    // hacen la mayoría de servidores y aceptan todos los clientes.
    let respuesta = [VERSION, codigo, 0x00, TIPO_IPV4, 0, 0, 0, 0, 0, 0];
    match timeout(flujo.write_all(&respuesta)).await {
        Ok(()) => Ok(()),
        Err(()) => Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "el cliente SOCKS no leyó la respuesta a tiempo",
        )),
    }
}

async fn leer_destino(flujo: &mut TcpStream, tipo: u8) -> Option<(String, u16)> {
    let direccion = match tipo {
        TIPO_IPV4 => {
            let mut octetos = [0u8; 4];
            timeout(flujo.read_exact(&mut octetos)).await.ok()?;
            Ipv4Addr::from(octetos).to_string()
        }
        TIPO_IPV6 => {
            let mut octetos = [0u8; 16];
            timeout(flujo.read_exact(&mut octetos)).await.ok()?;
            Ipv6Addr::from(octetos).to_string()
        }
        TIPO_DOMINIO => {
            let mut longitud = [0u8; 1];
            timeout(flujo.read_exact(&mut longitud)).await.ok()?;
            let mut nombre = vec![0u8; longitud[0] as usize];
            timeout(flujo.read_exact(&mut nombre)).await.ok()?;
            String::from_utf8(nombre).ok()?
        }
        _ => return None,
    };
    let mut puerto = [0u8; 2];
    timeout(flujo.read_exact(&mut puerto)).await.ok()?;
    let puerto = u16::from_be_bytes(puerto);
    debug!(direccion = %direccion, puerto, "petición SOCKS5 CONNECT");
    Some((direccion, puerto))
}

/// Código de respuesta para un canal que el host rechazó.
pub fn codigo_rechazo() -> u8 {
    RECHAZADA
}

async fn timeout<F, T>(futuro: F) -> Result<T, ()>
where
    F: std::future::Future<Output = std::io::Result<T>>,
{
    match tokio::time::timeout(PLAZO_SALUDO, futuro).await {
        Ok(Ok(valor)) => Ok(valor),
        _ => Err(()),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use tokio::net::TcpListener;

    /// Monta un cliente SOCKS contra la parte servidora y devuelve la petición.
    async fn conversar(saludo: &[u8], resto: &[u8]) -> Peticion {
        let escucha = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let puerto = escucha.local_addr().unwrap().port();
        let (saludo, resto) = (saludo.to_vec(), resto.to_vec());
        let cliente = tokio::spawn(async move {
            let mut flujo = TcpStream::connect(("127.0.0.1", puerto)).await.unwrap();
            flujo.write_all(&saludo).await.unwrap();
            let mut respuesta = [0u8; 2];
            flujo.read_exact(&mut respuesta).await.unwrap();
            flujo.write_all(&resto).await.unwrap();
            let mut eco = [0u8; 10];
            let _ = tokio::time::timeout(Duration::from_millis(200), flujo.read(&mut eco)).await;
            respuesta
        });
        let (mut servidor, _) = escucha.accept().await.unwrap();
        let peticion = negociar(&mut servidor).await;
        let respuesta = cliente.await.unwrap();
        assert_eq!(respuesta, [VERSION, METODO_SIN_AUTH]);
        peticion
    }

    #[tokio::test]
    async fn un_connect_a_dominio_se_deja_sin_resolver() {
        let mut resto = vec![VERSION, COMANDO_CONNECT, 0x00, TIPO_DOMINIO, 11];
        resto.extend_from_slice(b"ejemplo.com");
        resto.extend_from_slice(&5432u16.to_be_bytes());
        assert_eq!(
            conversar(&[VERSION, 1, METODO_SIN_AUTH], &resto).await,
            Peticion::Conectar("ejemplo.com".to_string(), 5432)
        );
    }

    #[tokio::test]
    async fn un_connect_a_ipv4_e_ipv6_se_entiende() {
        let mut resto = vec![VERSION, COMANDO_CONNECT, 0x00, TIPO_IPV4, 10, 0, 0, 5];
        resto.extend_from_slice(&80u16.to_be_bytes());
        assert_eq!(
            conversar(&[VERSION, 1, METODO_SIN_AUTH], &resto).await,
            Peticion::Conectar("10.0.0.5".to_string(), 80)
        );

        let mut resto = vec![VERSION, COMANDO_CONNECT, 0x00, TIPO_IPV6];
        resto.extend_from_slice(&Ipv6Addr::LOCALHOST.octets());
        resto.extend_from_slice(&443u16.to_be_bytes());
        assert_eq!(
            conversar(&[VERSION, 1, METODO_SIN_AUTH], &resto).await,
            Peticion::Conectar("::1".to_string(), 443)
        );
    }

    #[tokio::test]
    async fn bind_y_udp_se_rechazan_con_orden_no_soportada() {
        let mut resto = vec![VERSION, 0x02, 0x00, TIPO_IPV4, 0, 0, 0, 0];
        resto.extend_from_slice(&0u16.to_be_bytes());
        assert_eq!(
            conversar(&[VERSION, 1, METODO_SIN_AUTH], &resto).await,
            Peticion::NoSoportada
        );
    }

    #[tokio::test]
    async fn sin_metodo_sin_autenticacion_no_se_sigue() {
        let escucha = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let puerto = escucha.local_addr().unwrap().port();
        let cliente = tokio::spawn(async move {
            let mut flujo = TcpStream::connect(("127.0.0.1", puerto)).await.unwrap();
            flujo.write_all(&[VERSION, 1, 0x02]).await.unwrap();
            let mut respuesta = [0u8; 2];
            flujo.read_exact(&mut respuesta).await.unwrap();
            respuesta
        });
        let (mut servidor, _) = escucha.accept().await.unwrap();
        assert_eq!(negociar(&mut servidor).await, Peticion::Invalida);
        assert_eq!(cliente.await.unwrap(), [VERSION, METODO_NO_ACEPTABLE]);
    }
}
