//! Listado de un directorio por SFTP para BALTHASAR-2 (F6) en el móvil.
//!
//! Es el mínimo que necesita la deliberación: abrir el subsistema (con plazo,
//! porque request_subsystem devuelve Ok aunque el host diga que no), listar
//! **un** directorio sin recursión y sin seguir enlaces, y cortarse a 10 000
//! entradas. El SFTP completo del escritorio (src/servidor/sftp.rs) no cambia
//! (D276).
//!
//! Lo que llega del host es entrada no fiable: la ruta se valida antes de
//! abrir nada y los nombres de las entradas se sanean y se acotan antes de
//! salir de aquí. El canal se cierra en todas las ramas (en las de error, por
//! el cierre en segundo plano de `russh` al soltar el flujo).
//!
//! Limitación conocida de `russh-sftp`: su `read_dir` acumula el directorio
//! entero antes de devolverlo, así que el tope de 10 000 se aplica al
//! resultado y el plazo de 10 s es lo que acota el tiempo y la memoria de una
//! respuesta abusiva.

use std::time::Duration;

use russh::client::Handle;
use russh_sftp::client::SftpSession;

use crate::conexion::cliente::Cliente;

/// Entradas que se leen como mucho de un directorio (§7.11).
pub const MAXIMO_ENTRADAS: usize = 10_000;

/// Plazo del saludo del subsistema y de la lectura.
pub const PLAZO_SUBSISTEMA: Duration = Duration::from_secs(10);

/// Largo máximo de la ruta que se le pide al host (entrada no fiable).
pub const MAXIMO_RUTA: usize = 4096;

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

/// Valida la ruta que se le pide al host: no vacía, sin caracteres de control
/// (ni el NUL) y acotada. Es entrada del inventario, que puede venir de la
/// sincronización.
pub fn validar_ruta(ruta: &str) -> Result<(), String> {
    if ruta.trim().is_empty() {
        return Err("la ruta del listado está vacía".to_string());
    }
    if ruta.chars().any(char::is_control) {
        return Err("la ruta del listado no puede llevar caracteres de control".to_string());
    }
    if ruta.chars().count() > MAXIMO_RUTA {
        return Err(format!(
            "la ruta del listado no puede pasar de {MAXIMO_RUTA} caracteres"
        ));
    }
    Ok(())
}

/// Nombre de una entrada saneado: sin caracteres de control ni de dirección
/// (los pinta la vista) y acotado al límite de la Fase 13.
fn nombre_de_entrada(nombre: &str) -> Option<String> {
    let nombre: String = nombre
        .chars()
        .filter(|caracter| !es_invisible(*caracter))
        .take(crate::servicios::limites::MAX_NOMBRE)
        .collect();
    if nombre.is_empty() {
        None
    } else {
        Some(nombre)
    }
}

/// Caracteres de control (C0, C1 y DEL) y de control bidireccional.
fn es_invisible(caracter: char) -> bool {
    caracter.is_control()
        || matches!(
            caracter,
            '\u{200e}' | '\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
                | '\u{200b}'..='\u{200d}'
        )
}

/// Lista un directorio remoto por SFTP sobre la conexión del host. No es
/// recursivo, no sigue enlaces y se corta a MAXIMO_ENTRADAS. Cualquier fallo
/// es un Err con su motivo (nunca un pánico).
pub async fn listar(
    handle: &Handle<Cliente>,
    ruta: &str,
    plazo: Duration,
) -> Result<Listado, String> {
    validar_ruta(ruta)?;
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
        // Al soltarse el futuro se suelta el flujo del canal y `russh` manda
        // su cierre en segundo plano: no queda ningún canal huérfano.
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
        // Un nombre sin nada visible no sirve para nada (y no debe llegar a la
        // vista): se descarta.
        let Some(nombre) = nombre_de_entrada(&elemento.file_name()) else {
            continue;
        };
        listado.entradas.push(EntradaListado {
            nombre,
            tipo,
            tamano: metadata.size.unwrap_or(0),
            mtime: metadata.mtime.map(i64::from).unwrap_or(0),
        });
    }
    let _ = tokio::time::timeout(plazo, sesion.close()).await;
    Ok(listado)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn la_ruta_del_listado_se_valida() {
        assert!(validar_ruta("/var/backups").is_ok());
        assert!(validar_ruta("backups").is_ok());
        assert!(validar_ruta("").is_err());
        assert!(validar_ruta("   ").is_err());
        assert!(validar_ruta("/var/back\u{0}ups").is_err());
        assert!(validar_ruta("/var/back\u{000a}ups").is_err());
        assert!(validar_ruta(&format!("/{}", "a".repeat(MAXIMO_RUTA))).is_err());
    }

    #[test]
    fn los_nombres_del_listado_se_sanean_y_se_acotan() {
        assert_eq!(nombre_de_entrada("db.sql.gz").as_deref(), Some("db.sql.gz"));
        assert_eq!(
            nombre_de_entrada("\u{1b}[31mdb.sql.gz\u{1b}[0m").as_deref(),
            Some("[31mdb.sql.gz[0m")
        );
        assert_eq!(nombre_de_entrada("db\u{202e}sql").as_deref(), Some("dbsql"));
        assert!(nombre_de_entrada("\u{1b}\u{0}").is_none());
        let largo = "a".repeat(MAXIMO_RUTA);
        assert_eq!(
            nombre_de_entrada(&largo).map(|nombre| nombre.chars().count()),
            Some(crate::servicios::limites::MAX_NOMBRE)
        );
    }
}
