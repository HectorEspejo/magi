//! Equipos de la carpeta: crear, unir, quitar, rotar la clave y desactivar
//! (Fase 10, §4.1, §4.3 y §7.4).
//!
//! La clave de sincronización vive solo en el llavero. Estas operaciones se
//! hacen desde la TUI o la CLI; el llavero y la carpeta son las fuentes de la
//! verdad.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::Connection;

use crate::config::{Config, Rutas};
use crate::llavero::{LlaveroSecreto, CUENTA_SINCRONIZACION, CUENTA_SINCRONIZACION_ANTERIOR};
use crate::registro;
use crate::sincronizacion::carpeta;
use crate::sincronizacion::cifrado::ClaveSincronizacion;
use crate::sincronizacion::formato::{self, Documento};

/// Días que se conserva la clave anterior tras una rotación.
pub const DIAS_CLAVE_ANTERIOR: i64 = 30;

/// Equipos quitados del universo: uuid → fecha del borrado (la más reciente).
pub fn equipos_quitados(documentos: &[Documento]) -> BTreeMap<String, String> {
    let mut quitados: BTreeMap<String, String> = BTreeMap::new();
    for documento in documentos {
        for quitado in &documento.equipos_quitados {
            let entrada = quitados
                .entry(quitado.uuid.clone())
                .or_insert_with(|| quitado.quitado_en.clone());
            if quitado.quitado_en > *entrada {
                *entrada = quitado.quitado_en.clone();
            }
        }
    }
    quitados
}

/// Guarda en el llavero la clave de sincronización.
pub fn guardar_en_llavero(
    clave: &ClaveSincronizacion,
    llavero: &dyn LlaveroSecreto,
) -> Result<(), String> {
    let texto = clave.texto();
    llavero.guardar(CUENTA_SINCRONIZACION, texto.as_str())
}

/// Lee la clave actual del llavero, si la hay.
pub fn clave_del_llavero(
    llavero: &dyn LlaveroSecreto,
) -> Result<Option<ClaveSincronizacion>, String> {
    let Some(texto) = llavero.recuperar(CUENTA_SINCRONIZACION)? else {
        return Ok(None);
    };
    ClaveSincronizacion::desde_texto(texto.as_str())
        .map(Some)
        .map_err(|error| error.to_string())
}

/// Lee la clave anterior (solo para leer tras una rotación), si la hay y sigue
/// dentro del plazo de 30 días.
pub fn clave_anterior_del_llavero(
    config: &Config,
    llavero: &dyn LlaveroSecreto,
) -> Result<Option<ClaveSincronizacion>, String> {
    if !clave_anterior_vigente(config) {
        return Ok(None);
    }
    let Some(texto) = llavero.recuperar(CUENTA_SINCRONIZACION_ANTERIOR)? else {
        return Ok(None);
    };
    ClaveSincronizacion::desde_texto(texto.as_str())
        .map(Some)
        .map_err(|error| error.to_string())
}

/// Claves con las que se puede descifrar: la actual y, durante los 30 días
/// siguientes a una rotación, la anterior.
pub fn claves_para_leer(
    config: &Config,
    llavero: &dyn LlaveroSecreto,
) -> Result<Vec<ClaveSincronizacion>, String> {
    let mut claves = Vec::new();
    if let Some(clave) = clave_del_llavero(llavero)? {
        claves.push(clave);
    }
    if let Some(anterior) = clave_anterior_del_llavero(config, llavero)? {
        claves.push(anterior);
    }
    Ok(claves)
}

/// Fecha límite para leer con la clave anterior: ahora + 30 días.
fn fecha_limite_anterior() -> String {
    let limite = chrono::Local::now() + chrono::Duration::days(DIAS_CLAVE_ANTERIOR);
    limite.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

/// ¿Sigue vigente la clave anterior según config.toml?
pub fn clave_anterior_vigente(config: &Config) -> bool {
    let Some(limite) = config.sincronizacion.clave_anterior_hasta.as_deref() else {
        return false;
    };
    match chrono::DateTime::parse_from_rfc3339(limite) {
        Ok(limite) => chrono::Local::now() < limite.with_timezone(&chrono::Local),
        Err(_) => false,
    }
}

/// Olvida la clave anterior si ha pasado el plazo. Devuelve si la olvidó.
pub fn olvidar_anterior_si_vencida(
    config: &mut Config,
    llavero: &dyn LlaveroSecreto,
) -> Result<bool> {
    if config.sincronizacion.clave_anterior_hasta.is_none() {
        return Ok(false);
    }
    if clave_anterior_vigente(config) {
        return Ok(false);
    }
    let _ = llavero.olvidar(CUENTA_SINCRONIZACION_ANTERIOR);
    config.sincronizacion.clave_anterior_hasta = None;
    Ok(true)
}

/// Comprueba que la carpeta existe y se puede escribir, sin dejar nada.
fn comprobar_carpeta(carpeta: &Path) -> Result<()> {
    let metadatos = std::fs::metadata(carpeta)
        .with_context(|| format!("la carpeta {} no existe", carpeta.display()))?;
    if !metadatos.is_dir() {
        anyhow::bail!("{} no es una carpeta", carpeta.display());
    }
    let prueba = carpeta.join(".magi-escritura");
    std::fs::write(&prueba, b"")
        .with_context(|| format!("no se puede escribir en {}", carpeta.display()))?;
    let _ = std::fs::remove_file(&prueba);
    Ok(())
}

/// Guarda la carpeta en config.toml.
fn guardar_carpeta(rutas: &Rutas, config: &mut Config, carpeta: &Path) -> Result<()> {
    config.sincronizacion.carpeta = Some(carpeta.to_string_lossy().to_string());
    config.sincronizacion.clave_anterior_hasta = None;
    config.guardar(&rutas.fichero_config())
}

/// Crea la sincronización en una carpeta sin ficheros de MAGI: valida la
/// carpeta, genera la identidad, la guarda en el llavero y guarda la carpeta en
/// config.toml. El primer fichero lo escribe la pasada que se lanza al
/// confirmar (la UI muestra antes la clave como texto y QR).
pub fn crear(
    conexion: &Connection,
    rutas: &Rutas,
    carpeta: &Path,
    config: &mut Config,
    llavero: &dyn LlaveroSecreto,
) -> Result<ClaveSincronizacion> {
    comprobar_carpeta(carpeta)?;
    if carpeta::hay_ficheros(carpeta)? {
        anyhow::bail!("la carpeta ya tiene ficheros de MAGI: une este equipo en vez de crearlo");
    }
    let clave = ClaveSincronizacion::generar();
    guardar_en_llavero(&clave, llavero).map_err(anyhow::Error::msg)?;
    guardar_carpeta(rutas, config, carpeta)?;
    registro::anotar(
        conexion,
        registro::INVENTARIO_SINC_ACTIVADA,
        None,
        None,
        &format!("sincronización creada en {}", carpeta.display()),
        crate::modelo::ResultadoRegistro::Ok,
    )?;
    Ok(clave)
}

/// Une este equipo a una carpeta con ficheros de otros equipos con la clave
/// pegada por el usuario. La valida descifrando y comprobando la firma de un
/// fichero; si no abre, no guarda nada. Después hace la unión por nombre con lo
/// que hay en la carpeta.
pub fn unir(
    conexion: &Connection,
    rutas: &Rutas,
    carpeta: &Path,
    texto_clave: &str,
    config: &mut Config,
    llavero: &dyn LlaveroSecreto,
) -> Result<ClaveSincronizacion> {
    comprobar_carpeta(carpeta)?;
    let clave = ClaveSincronizacion::desde_texto(texto_clave)?;
    let mut documentos = Vec::new();
    for fichero in carpeta::listar(carpeta)? {
        let Ok(bytes) = carpeta::leer(&fichero.ruta) else {
            continue;
        };
        let Ok(plano) = clave.descifrar(&bytes) else {
            continue;
        };
        let Ok(en_claro) = formato::separar(&plano) else {
            continue;
        };
        if !clave.verificar(en_claro.json.as_bytes(), &en_claro.firma) {
            continue;
        }
        if let Ok(documento) = formato::parsear(&en_claro.json) {
            documentos.push(documento);
        }
    }
    if documentos.is_empty() {
        anyhow::bail!("la clave no abre los ficheros de esta carpeta");
    }
    guardar_en_llavero(&clave, llavero).map_err(anyhow::Error::msg)?;
    guardar_carpeta(rutas, config, carpeta)?;
    let unidos =
        crate::sincronizacion::aplicacion::unir_por_nombre(conexion, &documentos, &rutas.hogar)?;
    registro::anotar(
        conexion,
        registro::INVENTARIO_SINC_ACTIVADA,
        None,
        None,
        &format!(
            "sincronización unida en {} ({} equipos, {} elementos con el mismo nombre)",
            carpeta.display(),
            documentos.len(),
            unidos
        ),
        crate::modelo::ResultadoRegistro::Ok,
    )?;
    Ok(clave)
}

/// Guarda la clave actual como anterior, con su fecha límite, y genera la
/// nueva.
fn rotar_en_llavero(
    config: &mut Config,
    llavero: &dyn LlaveroSecreto,
) -> Result<ClaveSincronizacion> {
    let actual = clave_del_llavero(llavero).map_err(anyhow::Error::msg)?;
    let nueva = ClaveSincronizacion::generar();
    if let Some(actual) = actual {
        let texto = actual.texto();
        llavero
            .guardar(CUENTA_SINCRONIZACION_ANTERIOR, texto.as_str())
            .map_err(anyhow::Error::msg)?;
    }
    guardar_en_llavero(&nueva, llavero).map_err(anyhow::Error::msg)?;
    config.sincronizacion.clave_anterior_hasta = Some(fecha_limite_anterior());
    Ok(nueva)
}

/// Quita un equipo: lo marca como quitado, borra su fichero de la carpeta y
/// rota la clave. Devuelve la clave nueva (la UI la muestra como texto y QR).
pub fn quitar(
    conexion: &Connection,
    rutas: &Rutas,
    config: &mut Config,
    uuid_equipo: &str,
    llavero: &dyn LlaveroSecreto,
) -> Result<ClaveSincronizacion> {
    let carpeta = carpeta_configurada(rutas, config)?;
    let equipo = crate::almacen::equipos::por_uuid(conexion, uuid_equipo)?
        .with_context(|| format!("no existe el equipo {uuid_equipo}"))?;
    if equipo.es_este {
        anyhow::bail!("no se puede quitar este mismo equipo");
    }
    crate::almacen::equipos::marcar_quitado(conexion, equipo.id, &crate::modelo::fecha_ahora())?;
    let _ = carpeta::borrar(&carpeta, uuid_equipo)?;
    let clave = rotar_en_llavero(config, llavero)?;
    config.guardar(&rutas.fichero_config())?;
    registro::anotar(
        conexion,
        registro::INVENTARIO_EQUIPO_QUITADO,
        None,
        None,
        &format!("equipo «{}» quitado; clave rotada", equipo.nombre),
        crate::modelo::ResultadoRegistro::Ok,
    )?;
    registro::anotar(
        conexion,
        registro::INVENTARIO_CLAVE_CAMBIADA,
        None,
        None,
        "clave rotada al quitar un equipo",
        crate::modelo::ResultadoRegistro::Ok,
    )?;
    Ok(clave)
}

/// Rota la clave sin quitar ningún equipo. Devuelve la clave nueva.
pub fn rotar(
    conexion: &Connection,
    rutas: &Rutas,
    config: &mut Config,
    llavero: &dyn LlaveroSecreto,
) -> Result<ClaveSincronizacion> {
    let _carpeta = carpeta_configurada(rutas, config)?;
    let clave = rotar_en_llavero(config, llavero)?;
    config.guardar(&rutas.fichero_config())?;
    registro::anotar(
        conexion,
        registro::INVENTARIO_CLAVE_CAMBIADA,
        None,
        None,
        "clave de sincronización rotada",
        crate::modelo::ResultadoRegistro::Ok,
    )?;
    Ok(clave)
}

/// Desactiva la sincronización en este equipo: borra su fichero de la carpeta,
/// olvida la clave del llavero y quita la carpeta de config.toml. magi.db queda
/// intacta.
pub fn desactivar(
    conexion: &Connection,
    rutas: &Rutas,
    config: &mut Config,
    llavero: &dyn LlaveroSecreto,
) -> Result<()> {
    let Some(carpeta) = config.sincronizacion.ruta_carpeta(&rutas.hogar) else {
        anyhow::bail!("la sincronización no está configurada");
    };
    let este = crate::almacen::equipos::este(conexion)?;
    let _ = carpeta::borrar(&carpeta, &este.uuid)?;
    let _ = llavero.olvidar(CUENTA_SINCRONIZACION);
    let _ = llavero.olvidar(CUENTA_SINCRONIZACION_ANTERIOR);
    config.sincronizacion.carpeta = None;
    config.sincronizacion.clave_anterior_hasta = None;
    config.guardar(&rutas.fichero_config())?;
    registro::anotar(
        conexion,
        registro::INVENTARIO_SINC_DESACTIVADA,
        None,
        None,
        &format!("sincronización desactivada (carpeta {})", carpeta.display()),
        crate::modelo::ResultadoRegistro::Ok,
    )?;
    Ok(())
}

/// Reincorpora este equipo como equipo nuevo tras haber sido quitado por otro:
/// borra el fichero de su uuid anterior, le asigna un uuid nuevo y lo deja
/// listo para unir de nuevo con la clave nueva. Devuelve el uuid nuevo.
pub fn reunir(conexion: &Connection, rutas: &Rutas, config: &mut Config) -> Result<String> {
    let Some(carpeta) = config.sincronizacion.ruta_carpeta(&rutas.hogar) else {
        anyhow::bail!("la sincronización no está configurada");
    };
    let este = crate::almacen::equipos::este(conexion)?;
    let _ = carpeta::borrar(&carpeta, &este.uuid)?;
    let nuevo = uuid::Uuid::new_v4().to_string();
    crate::almacen::equipos::reincorporar(conexion, este.id, &nuevo)?;
    if !clave_anterior_vigente(config) {
        config.sincronizacion.clave_anterior_hasta = None;
    }
    config.guardar(&rutas.fichero_config())?;
    registro::anotar(
        conexion,
        registro::INVENTARIO_SINC_ACTIVADA,
        None,
        None,
        &format!("equipo reincorporado como equipo nuevo ({nuevo}); une con la clave nueva"),
        crate::modelo::ResultadoRegistro::Ok,
    )?;
    Ok(nuevo)
}

/// Carpeta configurada y resuelta contra el home, con error legible.
pub fn carpeta_configurada(rutas: &Rutas, config: &Config) -> Result<PathBuf> {
    config
        .sincronizacion
        .ruta_carpeta(&rutas.hogar)
        .context("la sincronización no está configurada: falta la carpeta en config.toml")
}
