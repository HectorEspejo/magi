use anyhow::Context;
use russh::client::Handle;

use super::cliente::{autenticar, Cliente, Contexto, ErrorCliente, IdentidadUsada};
use crate::modelo::Host;

/// Cadena de conexión ordenada del salto más lejano al destino.
pub fn construir_cadena(
    host: &Host,
    hosts: &std::collections::HashMap<i64, Host>,
) -> Result<Vec<Host>, ErrorCliente> {
    let mut cadena = vec![host.clone()];
    let mut actual = host.clone();
    let mut niveles = 0u8;
    while let Some(salto_id) = actual.salto_host_id {
        niveles += 1;
        if niveles > 3 {
            return Err(ErrorCliente::Mensaje(
                "la cadena de saltos supera los 3 niveles".to_string(),
            ));
        }
        let salto = hosts.get(&salto_id).ok_or_else(|| {
            ErrorCliente::Mensaje(format!(
                "el host de salto de «{}» ya no existe",
                actual.nombre
            ))
        })?;
        cadena.push(salto.clone());
        actual = salto.clone();
    }
    cadena.reverse();
    Ok(cadena)
}

pub struct Transporte {
    pub handle: std::sync::Arc<Handle<Cliente>>,
    pub saltos: Vec<Handle<Cliente>>,
    pub identidad: IdentidadUsada,
}

/// Conecta y autentica toda la cadena, saltando con canales `direct-tcpip`
/// hasta el destino.
pub async fn conectar_cadena(
    cadena: &[Host],
    contexto: &Contexto,
) -> Result<Transporte, ErrorCliente> {
    let mut saltos: Vec<Handle<Cliente>> = Vec::new();
    let mut canal: Option<russh::Channel<russh::client::Msg>> = None;
    let mut handle_final: Option<Handle<Cliente>> = None;
    let mut identidad_usada = IdentidadUsada {
        descripcion: String::new(),
        huella: None,
    };

    for (indice, host) in cadena.iter().enumerate() {
        let config = std::sync::Arc::new(config_cliente(host));
        let manejador = Cliente::nuevo(
            contexto.tx.clone(),
            host.id,
            &host.direccion,
            host.puerto,
            contexto.known_hosts.clone(),
            contexto.interactivo,
        )
        .con_reenvios(contexto.reenvios.clone());
        let mut handle = match canal.take() {
            Some(canal) => {
                russh::client::connect_stream(config, canal.into_stream(), manejador).await?
            }
            None => {
                let socket = conectar_tcp(&host.direccion, host.puerto).await?;
                russh::client::connect_stream(config, socket, manejador).await?
            }
        };
        let identidad = autenticar(&mut handle, host, contexto).await?;
        if indice + 1 < cadena.len() {
            let siguiente = &cadena[indice + 1];
            let nuevo_canal = handle
                .channel_open_direct_tcpip(
                    siguiente.direccion.clone(),
                    siguiente.puerto as u32,
                    "127.0.0.1".to_string(),
                    0,
                )
                .await
                .with_context(|| {
                    format!(
                        "abriendo el canal hacia {} a través de {}",
                        siguiente.nombre, host.nombre
                    )
                })?;
            saltos.push(handle);
            canal = Some(nuevo_canal);
        } else {
            identidad_usada = identidad;
            handle_final = Some(handle);
        }
    }
    Ok(Transporte {
        handle: std::sync::Arc::new(handle_final.expect("la cadena siempre incluye el destino")),
        saltos,
        identidad: identidad_usada,
    })
}

fn config_cliente(host: &Host) -> russh::client::Config {
    russh::client::Config {
        keepalive_interval: host
            .keepalive_seg
            .map(|segundos| std::time::Duration::from_secs(u64::from(segundos))),
        ..Default::default()
    }
}

/// Resolución DNS + conexión TCP con timeout de 10 s.
async fn conectar_tcp(direccion: &str, puerto: u16) -> Result<tokio::net::TcpStream, ErrorCliente> {
    let intento = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio::net::TcpStream::connect((direccion, puerto)),
    )
    .await;
    match intento {
        Err(_) => Err(ErrorCliente::Mensaje(format!(
            "se agotó el tiempo de conexión con {direccion}:{puerto} (10 s)"
        ))),
        Ok(Err(error)) => Err(ErrorCliente::Mensaje(format!(
            "no se pudo conectar con {direccion}:{puerto}: {error}"
        ))),
        Ok(Ok(socket)) => Ok(socket),
    }
}
