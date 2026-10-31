//! Tabla ↔ elemento: qué campos viajan de cada tipo y cómo se exportan y se
//! aplican (Fase 10, §3 y §7.3).
//!
//! Los tipos `Datos*` son el contrato del formato (docs/formato-sincronizacion.md
//! y la app Android). Lo que no aparece aquí es propio del equipo y no viaja.
//!
//! ## Hogar y rutas
//!
//! Las rutas bajo el hogar (`identidad_ref fichero:` y `ruta_local`) se
//! exportan como `~/…` (checklist de la Fase 10). Para que el JSON canónico
//! sea el mismo tanto si se compone con el hogar del entorno (lo que hace
//! `datos_de_fila`, que llama `versiones::datos_de` para comparar una fila
//! antes y después de una escritura) como con el que pasa la exportación
//! (`estado_local`), todo se arma con la misma función interna y se serializa
//! a JSON sobre un `serde_json::Value`. Así el contenido de un `Candidato` es
//! byte a byte el mismo que compara la fusión.
//!
//! `datos_de_fila` resuelve el hogar real con `directories` porque su firma
//! es la que consume el almacén. Si no se puede determinar, deja las rutas tal
//! cual: la comparación antes/después sigue valiendo, porque el hogar no
//! cambia entre las dos llamadas.

use std::path::Path;

use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::almacen::versiones::{vector_desde_json, Tabla};
use crate::sincronizacion::fusion::{nombre_de_contenido, Candidato};

/// Datos de un grupo que viajan (sin `plegado`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatosGrupo {
    pub nombre: String,
    pub orden: i64,
}

/// Datos de un host que viajan. Las referencias son uuids; las etiquetas, por
/// nombre; las verificaciones van dentro del host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatosHost {
    pub nombre: String,
    pub direccion: String,
    pub puerto: u16,
    pub usuario: Option<String>,
    /// Referencia de identidad tal cual (`auto`, `contrasena`, `agente:<huella>`,
    /// `fichero:<ruta>`). Las rutas bajo el home se exportan como ~/.
    pub identidad_ref: Option<String>,
    pub salto: Option<String>,
    pub grupo: Option<String>,
    pub multiplexar: bool,
    pub keepalive_seg: Option<u32>,
    pub opciones_extra: String,
    pub servicios: String,
    pub origen: String,
    pub etiquetas: Vec<String>,
    pub verificaciones: Option<DatosVerificaciones>,
    pub snippet_al_conectar: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatosVerificaciones {
    pub salud: bool,
    pub backup: bool,
    pub backup_ruta: Option<String>,
    pub backup_patron: Option<String>,
    pub tests: bool,
    pub tests_comando: Option<String>,
}

/// Datos de una identidad que viajan (sin origen, ruta, fechas ni último uso).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatosIdentidad {
    pub huella: String,
    pub alias: String,
    pub tipo: String,
    pub comentario: Option<String>,
    pub revocada_en: Option<String>,
}

/// Datos de un túnel que viajan (sin fechas de creación).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatosTunel {
    pub host: String,
    pub nombre: String,
    pub tipo: String,
    pub escucha: String,
    pub destino: Option<String>,
    pub automatico: bool,
}

/// Un destino de snippet: exactamente uno de `etiqueta` o `host`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DestinoSnippet {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etiqueta: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
}

/// Datos de un snippet que viajan (sin usos ni fechas).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatosSnippet {
    pub nombre: String,
    pub comando: String,
    pub descripcion: String,
    pub etiquetas: String,
    pub critico: bool,
    pub timeout_seg: u64,
    pub parar_al_fallo: bool,
    pub destinos: Vec<DestinoSnippet>,
}

/// Datos de una sincronización de directorio que viajan (sin último
/// resultado ni fecha de ejecución).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatosSincronizacion {
    pub host: String,
    pub nombre: String,
    pub ruta_local: String,
    pub ruta_remota: String,
    pub direccion: String,
    pub borrar: bool,
    pub exclusiones: String,
}

/// JSON canónico de los datos que viajan de una fila, para comparar si una
/// escritura local debe subir la versión. `None` si la fila no existe.
///
/// Normaliza las rutas del hogar con el hogar real del usuario, que es lo que
/// hace falta cuando lo llama `versiones::datos_de` sin más contexto.
pub fn datos_de_fila(conexion: &Connection, tabla: Tabla, id: i64) -> Result<Option<String>> {
    let hogar = directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf());
    serializar(valor_de_fila(conexion, tabla, id, hogar.as_deref())?)
}

/// Igual que `datos_de_fila`, con el hogar explícito. La exportación
/// (`estado_local`) y las pruebas usan este camino para no depender del hogar
/// del proceso; el JSON que sale es idéntico al de `datos_de_fila` para el
/// mismo estado.
pub fn datos_de_fila_con_hogar(
    conexion: &Connection,
    tabla: Tabla,
    id: i64,
    hogar: &Path,
) -> Result<Option<String>> {
    serializar(valor_de_fila(conexion, tabla, id, Some(hogar))?)
}

fn serializar(valor: Option<Value>) -> Result<Option<String>> {
    valor
        .map(|valor| serde_json::to_string(&valor).context("serializando los datos que viajan"))
        .transpose()
}

/// Valores que viajan de una fila, con el hogar para normalizar rutas (`None`:
/// sin normalizar). Es el único sitio que conoce el mapeo de campos.
fn valor_de_fila(
    conexion: &Connection,
    tabla: Tabla,
    id: i64,
    hogar: Option<&Path>,
) -> Result<Option<Value>> {
    match tabla {
        Tabla::Grupos => a_valor(datos_grupo(conexion, id)?),
        Tabla::Hosts => a_valor(datos_host(conexion, id, hogar)?),
        Tabla::Identidades => a_valor(datos_identidad(conexion, id)?),
        Tabla::Tuneles => a_valor(datos_tunel(conexion, id)?),
        Tabla::Snippets => a_valor(datos_snippet(conexion, id)?),
        Tabla::SincronizacionesDir => a_valor(datos_sincronizacion(conexion, id, hogar)?),
    }
}

/// Convierte los datos de una fila (si existe) al `Value` que se serializa.
/// Se pasa por `Value` a propósito: el mismo `Value` se serializa igual en
/// `datos_de_fila` y en `estado_local`, y la fusión compara esos bytes.
fn a_valor<T: Serialize>(datos: Option<T>) -> Result<Option<Value>> {
    datos
        .map(|datos| serde_json::to_value(datos).context("serializando los datos que viajan"))
        .transpose()
}

fn datos_grupo(conexion: &Connection, id: i64) -> Result<Option<DatosGrupo>> {
    Ok(conexion
        .query_row(
            "SELECT nombre, orden FROM GRUPOS WHERE id = ?1",
            params![id],
            |fila| {
                Ok(DatosGrupo {
                    nombre: fila.get(0)?,
                    orden: fila.get(1)?,
                })
            },
        )
        .optional()?)
}

/// Fila de `HOSTS` tal cual está en la base de datos (sin resolver referencias).
struct FilaHost {
    nombre: String,
    grupo_id: Option<i64>,
    direccion: String,
    puerto: i64,
    usuario: Option<String>,
    identidad_ref: Option<String>,
    salto_host_id: Option<i64>,
    multiplexar: bool,
    keepalive_seg: Option<i64>,
    opciones_extra: String,
    servicios: String,
    origen: String,
    snippet_al_conectar_id: Option<i64>,
}

fn datos_host(conexion: &Connection, id: i64, hogar: Option<&Path>) -> Result<Option<DatosHost>> {
    let fila = conexion
        .query_row(
            "SELECT nombre, grupo_id, direccion, puerto, usuario, identidad_ref,
                    salto_host_id, multiplexar, keepalive_seg, opciones_extra,
                    servicios, origen, snippet_al_conectar_id
               FROM HOSTS WHERE id = ?1",
            params![id],
            |fila| {
                Ok(FilaHost {
                    nombre: fila.get(0)?,
                    grupo_id: fila.get(1)?,
                    direccion: fila.get(2)?,
                    puerto: fila.get(3)?,
                    usuario: fila.get(4)?,
                    identidad_ref: fila.get(5)?,
                    salto_host_id: fila.get(6)?,
                    multiplexar: fila.get::<_, i64>(7)? != 0,
                    keepalive_seg: fila.get(8)?,
                    opciones_extra: fila.get(9)?,
                    servicios: fila.get(10)?,
                    origen: fila.get(11)?,
                    snippet_al_conectar_id: fila.get(12)?,
                })
            },
        )
        .optional()?;
    let Some(fila) = fila else {
        return Ok(None);
    };
    let puerto = u16::try_from(fila.puerto)
        .with_context(|| format!("puerto fuera de rango en el host {id}: {}", fila.puerto))?;
    let keepalive_seg = match fila.keepalive_seg {
        Some(segundos) => Some(
            u32::try_from(segundos)
                .with_context(|| format!("keepalive fuera de rango en el host {id}: {segundos}"))?,
        ),
        None => None,
    };
    Ok(Some(DatosHost {
        nombre: fila.nombre,
        direccion: fila.direccion,
        puerto,
        usuario: fila.usuario.filter(|usuario| !usuario.is_empty()),
        identidad_ref: identidad_ref_exportada(fila.identidad_ref, hogar),
        salto: uuid_de(conexion, Tabla::Hosts, fila.salto_host_id)?,
        grupo: uuid_de(conexion, Tabla::Grupos, fila.grupo_id)?,
        multiplexar: fila.multiplexar,
        keepalive_seg,
        opciones_extra: fila.opciones_extra,
        servicios: fila.servicios,
        origen: fila.origen,
        etiquetas: crate::almacen::etiquetas::de_host(conexion, id)?,
        verificaciones: verificaciones_de_host(conexion, id)?,
        snippet_al_conectar: uuid_de(conexion, Tabla::Snippets, fila.snippet_al_conectar_id)?,
    }))
}

/// La referencia de identidad tal cual, salvo una ruta `fichero:` bajo el
/// hogar, que se exporta como `~/…`.
fn identidad_ref_exportada(texto: Option<String>, hogar: Option<&Path>) -> Option<String> {
    let texto = texto.filter(|texto| !texto.is_empty())?;
    let Some(ruta) = texto.strip_prefix("fichero:") else {
        return Some(texto);
    };
    let ruta = match hogar {
        Some(hogar) => ruta_a_home(ruta, hogar),
        None => ruta.to_string(),
    };
    Some(format!("fichero:{ruta}"))
}

fn verificaciones_de_host(
    conexion: &Connection,
    host_id: i64,
) -> Result<Option<DatosVerificaciones>> {
    Ok(conexion
        .query_row(
            "SELECT salud, backup, backup_ruta, backup_patron, tests, tests_comando
               FROM VERIFICACIONES_HOST WHERE host_id = ?1",
            params![host_id],
            |fila| {
                Ok(DatosVerificaciones {
                    salud: fila.get::<_, i64>(0)? != 0,
                    backup: fila.get::<_, i64>(1)? != 0,
                    backup_ruta: fila.get(2)?,
                    backup_patron: fila.get(3)?,
                    tests: fila.get::<_, i64>(4)? != 0,
                    tests_comando: fila.get(5)?,
                })
            },
        )
        .optional()?)
}

fn datos_identidad(conexion: &Connection, id: i64) -> Result<Option<DatosIdentidad>> {
    Ok(conexion
        .query_row(
            "SELECT huella, alias, tipo, comentario, revocada_en
               FROM IDENTIDADES WHERE id = ?1",
            params![id],
            |fila| {
                Ok(DatosIdentidad {
                    huella: fila.get(0)?,
                    alias: fila.get(1)?,
                    tipo: fila.get(2)?,
                    comentario: fila.get(3)?,
                    revocada_en: fila.get(4)?,
                })
            },
        )
        .optional()?)
}

fn datos_tunel(conexion: &Connection, id: i64) -> Result<Option<DatosTunel>> {
    let fila = conexion
        .query_row(
            "SELECT host_id, nombre, tipo, escucha, destino, automatico
               FROM TUNELES WHERE id = ?1",
            params![id],
            |fila| {
                Ok((
                    fila.get::<_, i64>(0)?,
                    fila.get::<_, String>(1)?,
                    fila.get::<_, String>(2)?,
                    fila.get::<_, String>(3)?,
                    fila.get::<_, Option<String>>(4)?,
                    fila.get::<_, i64>(5)? != 0,
                ))
            },
        )
        .optional()?;
    let Some((host_id, nombre, tipo, escucha, destino, automatico)) = fila else {
        return Ok(None);
    };
    let host = uuid_de(conexion, Tabla::Hosts, Some(host_id))?
        .ok_or_else(|| anyhow!("el túnel {id} apunta a un host sin uuid"))?;
    Ok(Some(DatosTunel {
        host,
        nombre,
        tipo,
        escucha,
        destino,
        automatico,
    }))
}

fn datos_snippet(conexion: &Connection, id: i64) -> Result<Option<DatosSnippet>> {
    let fila = conexion
        .query_row(
            "SELECT nombre, comando, descripcion, etiquetas, critico, timeout_seg,
                    parar_al_fallo
               FROM SNIPPETS WHERE id = ?1",
            params![id],
            |fila| {
                Ok((
                    fila.get::<_, String>(0)?,
                    fila.get::<_, String>(1)?,
                    fila.get::<_, String>(2)?,
                    fila.get::<_, String>(3)?,
                    fila.get::<_, i64>(4)? != 0,
                    fila.get::<_, i64>(5)?,
                    fila.get::<_, i64>(6)? != 0,
                ))
            },
        )
        .optional()?;
    let Some((nombre, comando, descripcion, etiquetas, critico, timeout, parar_al_fallo)) = fila
    else {
        return Ok(None);
    };
    let timeout_seg = u64::try_from(timeout)
        .with_context(|| format!("timeout fuera de rango en el snippet {id}: {timeout}"))?;
    Ok(Some(DatosSnippet {
        nombre,
        comando,
        descripcion,
        etiquetas,
        critico,
        timeout_seg,
        parar_al_fallo,
        destinos: destinos_de_snippet(conexion, id)?,
    }))
}

/// Destinos de un snippet en el orden en que se guardaron (el id de la fila),
/// que es estable entre exportaciones mientras no se reordenen.
fn destinos_de_snippet(conexion: &Connection, snippet_id: i64) -> Result<Vec<DestinoSnippet>> {
    let mut sentencia = conexion.prepare(
        "SELECT d.etiqueta, d.host_id, h.uuid
           FROM SNIPPET_DESTINOS d
           LEFT JOIN HOSTS h ON h.id = d.host_id
          WHERE d.snippet_id = ?1
          ORDER BY d.id",
    )?;
    let filas = sentencia.query_map(params![snippet_id], |fila| {
        Ok((
            fila.get::<_, Option<String>>(0)?,
            fila.get::<_, Option<i64>>(1)?,
            fila.get::<_, Option<String>>(2)?,
        ))
    })?;
    let mut destinos = Vec::new();
    for fila in filas {
        let (etiqueta, host_id, host) = fila?;
        match (etiqueta, host_id, host) {
            (Some(etiqueta), None, _) if !etiqueta.is_empty() => destinos.push(DestinoSnippet {
                etiqueta: Some(etiqueta),
                host: None,
            }),
            (None, Some(_), Some(host)) if !host.is_empty() => destinos.push(DestinoSnippet {
                etiqueta: None,
                host: Some(host),
            }),
            _ => anyhow::bail!("destino ilegible en el snippet {snippet_id}"),
        }
    }
    Ok(destinos)
}

fn datos_sincronizacion(
    conexion: &Connection,
    id: i64,
    hogar: Option<&Path>,
) -> Result<Option<DatosSincronizacion>> {
    let fila = conexion
        .query_row(
            "SELECT host_id, nombre, ruta_local, ruta_remota, direccion, borrar,
                    exclusiones
               FROM SINCRONIZACIONES_DIR WHERE id = ?1",
            params![id],
            |fila| {
                Ok((
                    fila.get::<_, i64>(0)?,
                    fila.get::<_, String>(1)?,
                    fila.get::<_, String>(2)?,
                    fila.get::<_, String>(3)?,
                    fila.get::<_, String>(4)?,
                    fila.get::<_, i64>(5)? != 0,
                    fila.get::<_, String>(6)?,
                ))
            },
        )
        .optional()?;
    let Some((host_id, nombre, ruta_local, ruta_remota, direccion, borrar, exclusiones)) = fila
    else {
        return Ok(None);
    };
    let host = uuid_de(conexion, Tabla::Hosts, Some(host_id))?
        .ok_or_else(|| anyhow!("la sincronización {id} apunta a un host sin uuid"))?;
    let ruta_local = match hogar {
        Some(hogar) => ruta_a_home(&ruta_local, hogar),
        None => ruta_local,
    };
    Ok(Some(DatosSincronizacion {
        host,
        nombre,
        ruta_local,
        ruta_remota,
        direccion,
        borrar,
        exclusiones,
    }))
}

/// `uuid` de una fila referenciada por id. `None` si la referencia es nula,
/// si la fila no existe o si todavía no tiene uuid: la fusión resuelve las
/// referencias vacías.
fn uuid_de(conexion: &Connection, tabla: Tabla, id: Option<i64>) -> Result<Option<String>> {
    let Some(id) = id else {
        return Ok(None);
    };
    let uuid: Option<String> = conexion
        .query_row(
            &format!("SELECT uuid FROM {} WHERE id = ?1", tabla.nombre()),
            params![id],
            |fila| fila.get(0),
        )
        .optional()?
        .flatten();
    Ok(uuid.filter(|uuid| !uuid.is_empty()))
}

/// Lee el inventario completo como candidatos: todos los elementos vivos de
/// las seis tablas que viajan y todas las lápidas.
///
/// El contenido de cada vivo es el mismo JSON canónico que devuelve
/// `datos_de_fila_con_hogar` (misma función interna), que es lo que compara la
/// fusión. Las filas sin uuid, versión o vector (que no deberían existir) se
/// omiten con un aviso: no pueden participar en la fusión.
pub fn estado_local(conexion: &Connection, hogar: &Path) -> Result<Vec<Candidato>> {
    let mut candidatos = Vec::new();
    for tabla in Tabla::todas() {
        candidatos.extend(candidatos_vivos(conexion, tabla, hogar)?);
    }
    candidatos.extend(candidatos_lapidas(conexion)?);
    Ok(candidatos)
}

fn candidatos_vivos(conexion: &Connection, tabla: Tabla, hogar: &Path) -> Result<Vec<Candidato>> {
    let mut sentencia = conexion.prepare(&format!(
        "SELECT id, uuid, version, vector FROM {} ORDER BY uuid",
        tabla.nombre()
    ))?;
    let filas = sentencia.query_map([], |fila| {
        Ok((
            fila.get::<_, i64>(0)?,
            fila.get::<_, Option<String>>(1)?,
            fila.get::<_, Option<String>>(2)?,
            fila.get::<_, Option<String>>(3)?,
        ))
    })?;
    let mut candidatos = Vec::new();
    for fila in filas {
        let (id, uuid, version, vector_texto) = fila?;
        let (Some(uuid), Some(version), Some(vector_texto)) = (uuid, version, vector_texto) else {
            tracing::warn!(
                tabla = tabla.nombre(),
                id,
                "fila sin uuid, versión o vector: no se exporta"
            );
            continue;
        };
        if uuid.is_empty() || version.is_empty() {
            tracing::warn!(
                tabla = tabla.nombre(),
                id,
                "fila con uuid o versión vacíos: no se exporta"
            );
            continue;
        }
        let vector = vector_desde_json(&vector_texto);
        if vector.is_empty() {
            tracing::warn!(
                tabla = tabla.nombre(),
                id,
                "fila con vector vacío: no se exporta"
            );
            continue;
        }
        let contenido = match valor_de_fila(conexion, tabla, id, Some(hogar)) {
            Ok(Some(contenido)) => contenido,
            Ok(None) => continue,
            Err(error) => {
                tracing::warn!(
                    tabla = tabla.nombre(),
                    id,
                    "fila ilegible, no se exporta: {error}"
                );
                continue;
            }
        };
        candidatos.push(Candidato {
            tipo: tabla.tipo_elemento().to_string(),
            uuid,
            nombre: nombre_de_contenido(tabla.tipo_elemento(), &contenido),
            version,
            vector,
            contenido: Some(contenido),
            borrado_en: None,
        });
    }
    Ok(candidatos)
}

fn candidatos_lapidas(conexion: &Connection) -> Result<Vec<Candidato>> {
    let mut candidatos = Vec::new();
    for lapida in crate::almacen::lapidas::listar(conexion)? {
        if lapida.uuid.is_empty() || lapida.version.is_empty() {
            tracing::warn!(
                tipo = %lapida.tipo,
                "lápida con uuid o versión vacíos: no se exporta"
            );
            continue;
        }
        let vector = vector_desde_json(&lapida.vector);
        if vector.is_empty() {
            tracing::warn!(tipo = %lapida.tipo, uuid = %lapida.uuid, "lápida con vector vacío: no se exporta");
            continue;
        }
        candidatos.push(Candidato {
            tipo: lapida.tipo,
            uuid: lapida.uuid,
            nombre: lapida.nombre,
            version: lapida.version,
            vector,
            contenido: None,
            borrado_en: Some(lapida.borrado_en),
        });
    }
    Ok(candidatos)
}

/// Normaliza una ruta para exportarla: si cuelga del hogar, como `~/…`.
///
/// Se compara por componentes, no por prefijo de texto: `/home/ana2` no cuelga
/// de `/home/ana` aunque empiece igual.
pub fn ruta_a_home(ruta: &str, hogar: &Path) -> String {
    let home = hogar.to_str().unwrap_or_default().trim_end_matches('/');
    if home.is_empty() {
        return ruta.to_string();
    }
    if ruta == home {
        return "~".to_string();
    }
    let Some(resto) = ruta
        .strip_prefix(home)
        .and_then(|resto| resto.strip_prefix('/'))
    else {
        return ruta.to_string();
    };
    if resto.is_empty() {
        return "~".to_string();
    }
    format!("~/{resto}")
}

/// Expande ~/... con el home de este equipo.
pub fn ruta_desde_home(ruta: &str, hogar: &Path) -> String {
    if ruta == "~" {
        return hogar.to_string_lossy().to_string();
    }
    if let Some(resto) = ruta.strip_prefix("~/") {
        return hogar.join(resto).to_string_lossy().to_string();
    }
    ruta.to_string()
}
