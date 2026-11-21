//! Sincronización del inventario en el escritorio (Fase 10): sobre el núcleo
//! compartido añade la pasada en modo escritor (copia previa de magi.db,
//! confirmación de borrados masivos y exportación del fichero propio), el QR
//! de la clave y las operaciones de equipos que tocan config.toml.

pub use magi_nucleo::sincronizacion::*;

pub mod almacenamiento;
pub mod cli;
pub mod equipos;
pub mod qr;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use rusqlite::Connection;

use crate::config::{Config, Rutas};

/// Lo que se deja en el log (una sola vez por sesión) cuando la carpeta no
/// admite permisos. Sin rutas ni nada del inventario.
pub const AVISO_PERMISOS_IGNORADOS: &str =
    "la carpeta de sincronización no admite permisos 600 (almacenamiento compartido): \
     el fichero de este equipo se escribe sin ellos";

/// Un aviso que se da una sola vez por sesión de MAGI (un proceso).
#[derive(Debug, Default)]
pub struct AvisoUnico {
    dicho: AtomicBool,
}

impl AvisoUnico {
    pub const fn nuevo() -> Self {
        Self {
            dicho: AtomicBool::new(false),
        }
    }

    /// `true` solo la primera vez que se pregunta: quien lo recibe debe decirlo.
    pub fn primera_vez(&self) -> bool {
        !self.dicho.swap(true, Ordering::SeqCst)
    }
}

/// El aviso de los permisos de esta sesión de MAGI.
static PERMISOS_DICHOS: AvisoUnico = AvisoUnico::nuevo();

/// Cuántos ficheros rechazados se recuerdan para no repetirlos en el log: la
/// carpeta es entrada no fiable y no puede hacer crecer la memoria sin tope.
const MAX_RECHAZADOS_DICHOS: usize = 64;

/// Los ficheros rechazados con la clave anterior (R97) que ya constan en el log
/// de este proceso: una línea por fichero y proceso, no una en cada pasada.
static RECHAZADOS_DICHOS: std::sync::Mutex<std::collections::BTreeSet<String>> =
    std::sync::Mutex::new(std::collections::BTreeSet::new());

/// Deja en el log una línea por cada fichero rechazado que aún no conste: solo
/// su nombre y el motivo, nunca su contenido (R97). Pasado el tope, ya no
/// anota más: el aviso de la pasada lo sigue diciendo.
pub fn registrar_rechazados(rechazados: &[String]) {
    let Ok(mut dichos) = RECHAZADOS_DICHOS.lock() else {
        return;
    };
    for nombre in rechazados {
        if dichos.len() >= MAX_RECHAZADOS_DICHOS || !dichos.insert(nombre.clone()) {
            continue;
        }
        tracing::warn!(
            fichero = %nombre,
            "fichero de sincronización rechazado: {MOTIVO_RECHAZADO}"
        );
    }
}

/// Cómo escribe esta plataforma en la carpeta compartida: en Termux es el
/// almacenamiento compartido de Android, que no admite `chmod`; en el resto,
/// un 600 que no se puede aplicar es un error, como siempre. Es la única
/// pregunta por la plataforma de la pasada: el núcleo recibe la respuesta.
pub fn permisos_de_esta_plataforma() -> PermisosCarpeta {
    if crate::termux::es_termux() {
        PermisosCarpeta::Tolerantes
    } else {
        PermisosCarpeta::Estrictos
    }
}

/// Lo que pide la UI del escritorio para hacer una pasada.
pub struct OpcionesPasada<'a> {
    pub politica_borrados: PoliticaBorrados,
    pub trabajo_vivo: &'a dyn Fn(&str, &str) -> bool,
}

impl<'a> OpcionesPasada<'a> {
    pub fn sin_trabajo_vivo(politica: PoliticaBorrados) -> Self {
        fn ninguno(_tipo: &str, _uuid: &str) -> bool {
            false
        }
        Self {
            politica_borrados: politica,
            trabajo_vivo: &ninguno,
        }
    }
}

/// Una pasada completa en modo escritor. En Termux el fichero propio se
/// escribe tolerando que la carpeta no admita permisos (§3.2); la primera vez
/// que pasa en esta sesión de MAGI, queda una línea en el log.
pub fn pasada(
    conexion: &Connection,
    rutas: &Rutas,
    config: &Config,
    claves: &[cifrado::ClaveSincronizacion],
    opciones: &OpcionesPasada<'_>,
) -> Result<ResultadoPasada> {
    pasada_con_aviso(
        conexion,
        rutas,
        config,
        claves,
        opciones,
        permisos_de_esta_plataforma(),
        &PERMISOS_DICHOS,
    )
}

/// Como [`pasada`], con los permisos y el aviso de la sesión explícitos: cada
/// `AvisoUnico` es una sesión de MAGI (así se prueba «una línea por sesión»
/// sin depender del proceso).
pub fn pasada_con_aviso(
    conexion: &Connection,
    rutas: &Rutas,
    config: &Config,
    claves: &[cifrado::ClaveSincronizacion],
    opciones: &OpcionesPasada<'_>,
    permisos: PermisosCarpeta,
    aviso: &AvisoUnico,
) -> Result<ResultadoPasada> {
    pasada_con_aviso_y_ajuste(
        conexion,
        rutas,
        config,
        claves,
        opciones,
        permisos,
        aviso,
        &crate::ficheros::ajustar_permisos,
    )
}

/// Cuerpo de las pasadas del escritorio, con el `chmod` del fichero propio
/// inyectable (superficie de pruebas, ver `ficheros::escribir_atomico_con`).
#[doc(hidden)]
#[allow(clippy::too_many_arguments)] // lo que pide una pasada más el aviso y el chmod
pub fn pasada_con_aviso_y_ajuste(
    conexion: &Connection,
    rutas: &Rutas,
    config: &Config,
    claves: &[cifrado::ClaveSincronizacion],
    opciones: &OpcionesPasada<'_>,
    permisos: PermisosCarpeta,
    aviso: &AvisoUnico,
    ajustar: &dyn Fn(&Path, u32) -> std::io::Result<()>,
) -> Result<ResultadoPasada> {
    let exportar = |conn: &Connection| {
        let _ = crate::sshconfig::exportar::exportar(conn, &rutas.dir_ssh());
    };
    let copia_previa = |conn: &Connection| -> Result<PathBuf> {
        crate::copias::copiar(conn, rutas, config.sincronizacion.copias)
    };
    let nucleo = magi_nucleo::sincronizacion::OpcionesPasada {
        politica_borrados: opciones.politica_borrados,
        trabajo_vivo: opciones.trabajo_vivo,
        al_aplicar: if config.exportar_al_guardar {
            Some(&exportar as &dyn Fn(&Connection))
        } else {
            None
        },
        copia_previa: Some(&copia_previa as &dyn Fn(&Connection) -> Result<PathBuf>),
    };
    let salida = magi_nucleo::sincronizacion::pasada_con_ajuste(
        conexion,
        rutas,
        &config.sincronizacion,
        crate::VERSION_APP,
        claves,
        &nucleo,
        permisos,
        ajustar,
    )?;
    // Una línea por sesión, no por pasada, y sin rutas ni datos del inventario.
    if salida.permisos_ignorados && aviso.primera_vez() {
        tracing::info!("{AVISO_PERMISOS_IGNORADOS}");
    }
    registrar_rechazados(&salida.resultado.rechazados);
    Ok(salida.resultado)
}

/// Texto de estado de la sincronización para la CLI y la vista, con el motivo de
/// cada borrado aplazado que da `explicar_aplazo`.
pub fn estado_texto(
    conexion: &Connection,
    config: &Config,
    hogar: &Path,
    explicar_aplazo: &dyn Fn(&magi_nucleo::almacen::conflictos::Conflicto) -> String,
) -> Result<String> {
    magi_nucleo::sincronizacion::estado_texto(
        conexion,
        &config.sincronizacion,
        hogar,
        explicar_aplazo,
    )
}
