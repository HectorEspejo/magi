//! Listado de un directorio por SFTP para BALTHASAR-2 (F6) en el móvil.
//!
//! Es el mínimo que necesita la deliberación: abrir el subsistema (con plazo,
//! porque request_subsystem devuelve Ok aunque el host diga que no), listar
//! **un** directorio sin recursión y sin seguir enlaces, y cortarse a 10 000
//! entradas. El SFTP completo del escritorio (src/servidor/sftp.rs) no cambia
//! (D276).

use std::time::Duration;

use russh::client::Handle;
use russh_sftp::client::SftpSession;

use crate::conexion::cliente::Cliente;

/// Entradas que se leen como mucho de un directorio (§7.11).
pub const MAXIMO_ENTRADAS: usize = 10_000;

/// Plazo del saludo del subsistema y de la lectura.
pub const PLAZO_SUBSISTEMA: Duration = Duration::from_secs(10);

/// Tipo de una entrada del listado (sin seguir enlaces).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoEntradaListado {
    Fichero,
    Directorio,
    Enlace,
}

impl TipoEntradaListado {
    pub fn es_fichero(self) -> bool {
        matches!(self, TipoEntradaListado::Fichero)
    }

    pub fn texto(self) -> &'static str {
        match self {
            TipoEntradaListado::Fichero => "fichero",
            TipoEntradaListado::Directorio => "directorio",
            TipoEntradaListado::Enlace => "enlace",
        }
    }
}

/// Una entrada del directorio: nombre, tipo, tamaño y fecha de modificación.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntradaListado {
    pub nombre: String,
    pub tipo: TipoEntradaListado,
    pub tamano: u64,
    /// Época en segundos; cero si el otro extremo no la dio.
    pub mtime: i64,
}

/// Lo que devuelve un listado, con aviso si se cortó a MAXIMO_ENTRADAS.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listado {
    pub entradas: Vec<EntradaListado>,
    pub truncado: bool,
}

/// Lista un directorio remoto por SFTP sobre la conexión del host. No es
/// recursivo, no sigue enlaces y se corta a MAXIMO_ENTRADAS. Cualquier fallo
/// es un Err con su motivo (nunca un pánico).
pub async fn listar(
    handle: &Handle<Cliente>,
    ruta: &str,
    plazo: Duration,
) -> Result<Listado, String> {
    let canal = tokio::time::timeout(plazo, handle.channel_open_session())
        .await
        .map_err(|_| "no se pudo abrir el canal SFTP (plazo vencido)".to_string())?
        .map_err(|error| format!("no se pudo abrir el canal SFTP: {error}"))?;
    // request_subsystem devuelve Ok aunque el host diga que no: el plazo
    // cubre el saludo de abajo, pero pedirlo también puede colgarse.
    match tokio::time::timeout(plazo, canal.request_subsystem(true, "sftp")).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            let _ = canal.close().await;
            return Err(format!("el host no ofrece SFTP ({error})"));
        }
        Err(_) => {
            let _ = canal.close().await;
            return Err("el host no ofrece SFTP".to_string());
        }
    }
    let sesion = match tokio::time::timeout(plazo, SftpSession::new(canal.into_stream())).await {
        Ok(Ok(sesion)) => sesion,
        Ok(Err(error)) => return Err(format!("el host no ofrece SFTP ({error})")),
        Err(_) => return Err("el host no ofrece SFTP".to_string()),
    };
    let lectura = match tokio::time::timeout(plazo, sesion.read_dir(ruta)).await {
        Ok(Ok(lectura)) => lectura,
        Ok(Err(error)) => {
            let _ = tokio::time::timeout(plazo, sesion.close()).await;
            return Err(format!("no se pudo listar {ruta}: {error}"));
        }
        Err(_) => {
            let _ = tokio::time::timeout(plazo, sesion.close()).await;
            return Err(format!("no se pudo listar {ruta}: plazo vencido"));
        }
    };
    let mut listado = Listado::default();
    for elemento in lectura {
        if listado.entradas.len() >= MAXIMO_ENTRADAS {
            listado.truncado = true;
            break;
        }
        let metadata = elemento.metadata();
        let tipo = if metadata.is_symlink() {
            TipoEntradaListado::Enlace
        } else if metadata.is_dir() {
            TipoEntradaListado::Directorio
        } else {
            TipoEntradaListado::Fichero
        };
        listado.entradas.push(EntradaListado {
            nombre: elemento.file_name(),
            tipo,
            tamano: metadata.size.unwrap_or(0),
            mtime: metadata.mtime.map(i64::from).unwrap_or(0),
        });
    }
    let _ = tokio::time::timeout(plazo, sesion.close()).await;
    Ok(listado)
}
