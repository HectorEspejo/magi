use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use anyhow::{Context, Result};
use russh::keys::known_hosts::known_host_keys_path;
use russh::keys::{HashAlg, PublicKey};

use crate::ficheros::escribir_atomico;

/// Resultado de comprobar una clave de servidor contra known_hosts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EstadoHuella {
    Conocida,
    Desconocida,
    Cambiada { anterior: PublicKey },
}

/// Formato de host de OpenSSH: `host` o `[host]:puerto`.
pub fn formato_host(host: &str, puerto: u16) -> String {
    if puerto == 22 {
        host.to_string()
    } else {
        format!("[{host}]:{puerto}")
    }
}

pub fn tipo_clave(clave: &PublicKey) -> String {
    clave.algorithm().as_str().to_string()
}

/// Huella SHA256 en el formato de OpenSSH (`SHA256:…`).
pub fn huella(clave: &PublicKey) -> String {
    clave.fingerprint(HashAlg::Sha256).to_string()
}

/// Comprueba la clave del servidor contra `known_hosts`, entradas hasheadas
/// incluidas.
pub fn comprobar(ruta: &Path, host: &str, puerto: u16, clave: &PublicKey) -> Result<EstadoHuella> {
    let entradas = known_host_keys_path(host, puerto, ruta)
        .with_context(|| format!("leyendo {}", ruta.display()))?;
    let mut cambiada = None;
    for (_, registrada) in entradas {
        if registrada.algorithm() != clave.algorithm() {
            continue;
        }
        if &registrada == clave {
            return Ok(EstadoHuella::Conocida);
        }
        cambiada = Some(registrada);
    }
    Ok(match cambiada {
        Some(anterior) => EstadoHuella::Cambiada { anterior },
        None => EstadoHuella::Desconocida,
    })
}

fn linea_nueva(host: &str, puerto: u16, clave: &PublicKey) -> Result<String> {
    let clave = clave.to_openssh().context("serializando la clave")?;
    Ok(format!("{} {clave}", formato_host(host, puerto)))
}

/// Aprende una huella desconocida añadiendo una línea a `known_hosts`.
pub fn aprender(ruta: &Path, host: &str, puerto: u16, clave: &PublicKey) -> Result<()> {
    let linea = linea_nueva(host, puerto, clave)?;
    if let Some(directorio) = ruta.parent() {
        fs::create_dir_all(directorio)?;
    }
    let necesita_salto = match fs::metadata(ruta) {
        Ok(metadatos) if metadatos.len() > 0 => {
            let contenido = fs::read(ruta)?;
            !contenido.ends_with(b"\n")
        }
        _ => false,
    };
    let mut fichero = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(ruta)
        .with_context(|| format!("abriendo {}", ruta.display()))?;
    if necesita_salto {
        fichero.write_all(b"\n")?;
    }
    fichero.write_all(linea.as_bytes())?;
    fichero.write_all(b"\n")?;
    fichero.sync_all()?;
    crate::ficheros::asegurar_permisos(ruta, 0o600)?;
    Ok(())
}

/// Sustituye una huella cambiada: copia `known_hosts.old`, elimina las líneas
/// previas de esa dirección y añade la nueva.
pub fn sustituir(ruta: &Path, host: &str, puerto: u16, clave: &PublicKey) -> Result<()> {
    if !ruta.exists() {
        return aprender(ruta, host, puerto, clave);
    }
    let anteriores = known_host_keys_path(host, puerto, ruta)
        .with_context(|| format!("leyendo {}", ruta.display()))?;
    if anteriores.is_empty() {
        return aprender(ruta, host, puerto, clave);
    }
    let copia = ruta.with_file_name("known_hosts.old");
    fs::copy(ruta, &copia)
        .with_context(|| format!("copiando {} → {}", ruta.display(), copia.display()))?;
    crate::ficheros::asegurar_permisos(&copia, 0o600)?;
    let lineas_previas: Vec<usize> = anteriores.iter().map(|(linea, _)| *linea).collect();
    let contenido =
        fs::read_to_string(ruta).with_context(|| format!("leyendo {}", ruta.display()))?;
    let mut nuevas: Vec<&str> = contenido
        .lines()
        .enumerate()
        .filter(|(indice, _)| !lineas_previas.contains(&(indice + 1)))
        .map(|(_, linea)| linea)
        .collect();
    let linea = linea_nueva(host, puerto, clave)?;
    nuevas.push(&linea);
    let mut texto = nuevas.join("\n");
    texto.push('\n');
    escribir_atomico(ruta, texto.as_bytes(), 0o600)
}
