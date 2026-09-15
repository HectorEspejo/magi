use std::path::Path;

use anyhow::Result;
use rusqlite::{params, Connection, Row};

use crate::modelo::{fecha_ahora, Identidad, OrigenIdentidad};

const SELECCION: &str = "
    SELECT id, alias, tipo, huella, origen, ruta, comentario, anadida_en,
           ultimo_uso_en, revocada_en
      FROM IDENTIDADES";

fn mapear(fila: &Row<'_>) -> rusqlite::Result<Identidad> {
    Ok(Identidad {
        id: fila.get(0)?,
        alias: fila.get(1)?,
        tipo: fila.get(2)?,
        huella: fila.get(3)?,
        origen: OrigenIdentidad::desde_texto(&fila.get::<_, String>(4)?),
        ruta: fila.get(5)?,
        comentario: fila.get(6)?,
        anadida_en: fila.get(7)?,
        ultimo_uso_en: fila.get(8)?,
        revocada_en: fila.get(9)?,
    })
}

/// Clave detectada en un escaneo, lista para el *upsert* por huella.
#[derive(Debug, Clone)]
pub struct ClaveSincronizada {
    pub tipo: String,
    pub huella: String,
    pub origen: OrigenIdentidad,
    pub ruta: Option<String>,
    pub comentario: Option<String>,
}

/// Alta o actualización por huella. Una identidad revocada se conserva
/// revocada; el alias y `anadida_en` nunca se pisan.
pub fn sincronizar(conexion: &Connection, claves: &[ClaveSincronizada]) -> Result<()> {
    for clave in claves {
        let existente: Option<(i64,)> = conexion
            .query_row(
                "SELECT id FROM IDENTIDADES WHERE huella = ?1",
                params![clave.huella],
                |fila| Ok((fila.get(0)?,)),
            )
            .ok();
        match existente {
            Some((id,)) => {
                conexion.execute(
                    "UPDATE IDENTIDADES
                        SET tipo = ?1, origen = ?2, ruta = ?3, comentario = ?4
                      WHERE id = ?5",
                    params![
                        clave.tipo,
                        clave.origen.como_texto(),
                        clave.ruta,
                        clave.comentario,
                        id
                    ],
                )?;
            }
            None => {
                let alias = alias_disponible(conexion, clave, None)?;
                conexion.execute(
                    "INSERT INTO IDENTIDADES (
                        alias, tipo, huella, origen, ruta, comentario, anadida_en
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        alias,
                        clave.tipo,
                        clave.huella,
                        clave.origen.como_texto(),
                        clave.ruta,
                        clave.comentario,
                        fecha_ahora(),
                    ],
                )?;
            }
        }
    }
    Ok(())
}

/// Crea una identidad con alias único (generadas e importadas).
pub fn crear(
    conexion: &Connection,
    tipo: &str,
    huella: &str,
    origen: OrigenIdentidad,
    ruta: Option<&str>,
    comentario: Option<&str>,
) -> Result<i64> {
    let clave = ClaveSincronizada {
        tipo: tipo.to_string(),
        huella: huella.to_string(),
        origen,
        ruta: ruta.map(str::to_string),
        comentario: comentario.map(str::to_string),
    };
    let alias = alias_disponible(conexion, &clave, None)?;
    conexion.execute(
        "INSERT INTO IDENTIDADES (
            alias, tipo, huella, origen, ruta, comentario, anadida_en
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            alias,
            tipo,
            huella,
            origen.como_texto(),
            ruta,
            comentario,
            fecha_ahora(),
        ],
    )?;
    Ok(conexion.last_insert_rowid())
}

/// Alias por defecto: comentario, nombre del fichero o tipo, con sufijo
/// numérico si ya existe.
fn alias_disponible(
    conexion: &Connection,
    clave: &ClaveSincronizada,
    excluyendo: Option<i64>,
) -> Result<String> {
    let base = clave
        .comentario
        .as_deref()
        .map(str::trim)
        .filter(|texto| !texto.is_empty())
        .map(str::to_string)
        .or_else(|| {
            clave.ruta.as_deref().and_then(|ruta| {
                Path::new(ruta)
                    .file_name()
                    .and_then(|nombre| nombre.to_str())
                    .map(|nombre| nombre.trim_end_matches(".pub").to_string())
            })
        })
        .unwrap_or_else(|| clave.tipo.clone());
    if !existe_alias(conexion, &base, excluyendo)? {
        return Ok(base);
    }
    for sufijo in 2..1000 {
        let candidato = format!("{base}-{sufijo}");
        if !existe_alias(conexion, &candidato, excluyendo)? {
            return Ok(candidato);
        }
    }
    anyhow::bail!("no se pudo encontrar un alias libre para «{base}»")
}

pub fn existe_alias(conexion: &Connection, alias: &str, excluyendo: Option<i64>) -> Result<bool> {
    let total: i64 = match excluyendo {
        Some(id) => conexion.query_row(
            "SELECT COUNT(*) FROM IDENTIDADES WHERE alias = ?1 AND id <> ?2",
            params![alias, id],
            |fila| fila.get(0),
        )?,
        None => conexion.query_row(
            "SELECT COUNT(*) FROM IDENTIDADES WHERE alias = ?1",
            params![alias],
            |fila| fila.get(0),
        )?,
    };
    Ok(total > 0)
}

pub fn listar(conexion: &Connection, incluir_revocadas: bool) -> Result<Vec<Identidad>> {
    let sql = if incluir_revocadas {
        format!("{SELECCION} ORDER BY alias")
    } else {
        format!("{SELECCION} WHERE revocada_en IS NULL ORDER BY alias")
    };
    let mut sentencia = conexion.prepare(&sql)?;
    let filas = sentencia.query_map([], mapear)?;
    Ok(filas.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn obtener(conexion: &Connection, id: i64) -> Result<Identidad> {
    Ok(conexion.query_row(&format!("{SELECCION} WHERE id = ?1"), params![id], mapear)?)
}

pub fn por_huella(conexion: &Connection, huella: &str) -> Result<Option<Identidad>> {
    let mut sentencia = conexion.prepare(&format!("{SELECCION} WHERE huella = ?1"))?;
    let mut filas = sentencia.query_map(params![huella], mapear)?;
    match filas.next() {
        Some(identidad) => Ok(Some(identidad?)),
        None => Ok(None),
    }
}

/// Renombra el alias; vacío o duplicado devuelve un error legible.
pub fn renombrar(conexion: &Connection, id: i64, alias: &str) -> Result<()> {
    let alias = alias.trim();
    if alias.is_empty() {
        anyhow::bail!("el alias no puede estar vacío");
    }
    if existe_alias(conexion, alias, Some(id))? {
        anyhow::bail!("ya existe una identidad con el alias «{alias}»");
    }
    conexion.execute(
        "UPDATE IDENTIDADES SET alias = ?1 WHERE id = ?2",
        params![alias, id],
    )?;
    Ok(())
}

/// Marca el último uso por huella (autenticación con éxito).
pub fn marcar_uso(conexion: &Connection, huella: &str, fecha: &str) -> Result<()> {
    conexion.execute(
        "UPDATE IDENTIDADES SET ultimo_uso_en = ?1 WHERE huella = ?2",
        params![fecha, huella],
    )?;
    Ok(())
}

/// Referencias de host que apuntan a esta identidad (`agente:` o `fichero:`),
/// con las variantes de ruta con y sin `~`.
pub fn variantes_ref(identidad: &Identidad, hogar: &Path) -> Vec<String> {
    match identidad.origen {
        OrigenIdentidad::Agente | OrigenIdentidad::Token => {
            vec![format!("agente:{}", identidad.huella)]
        }
        OrigenIdentidad::Fichero => {
            let mut variantes = Vec::new();
            let Some(ruta) = &identidad.ruta else {
                return variantes;
            };
            let completa = if let Some(resto) = ruta.strip_prefix("~/") {
                hogar.join(resto)
            } else {
                Path::new(ruta).to_path_buf()
            };
            variantes.push(format!("fichero:{ruta}"));
            if let Ok(resto) = completa.strip_prefix(hogar) {
                variantes.push(format!("fichero:~/{}", resto.display()));
            }
            if ruta.starts_with("~/") {
                variantes.push(format!("fichero:{}", completa.display()));
            }
            variantes.dedup();
            variantes
        }
    }
}

/// Nombres de los hosts cuyo `identidad_ref` casa con esta identidad.
pub fn hosts_que_usan(
    conexion: &Connection,
    identidad: &Identidad,
    hogar: &Path,
) -> Result<Vec<String>> {
    let mut nombres = Vec::new();
    for variante in variantes_ref(identidad, hogar) {
        let mut sentencia = conexion
            .prepare("SELECT nombre FROM HOSTS WHERE identidad_ref = ?1 ORDER BY nombre")?;
        let filas = sentencia.query_map(params![variante], |fila| fila.get::<_, String>(0))?;
        for nombre in filas {
            let nombre = nombre?;
            if !nombres.contains(&nombre) {
                nombres.push(nombre);
            }
        }
    }
    nombres.sort();
    Ok(nombres)
}

/// Baja lógica: marca `revocada_en` y pone a `auto` los hosts que la usan.
/// Nunca toca ficheros de `~/.ssh` ni el agente. Devuelve el número de hosts
/// que pasaron a `auto`.
pub fn revocar(conexion: &Connection, id: i64, hogar: &Path) -> Result<usize> {
    let identidad = obtener(conexion, id)?;
    let hosts = hosts_que_usan(conexion, &identidad, hogar)?;
    let ahora = fecha_ahora();
    let transaccion = conexion.unchecked_transaction()?;
    for variante in variantes_ref(&identidad, hogar) {
        transaccion.execute(
            "UPDATE HOSTS SET identidad_ref = NULL, actualizado_en = ?1
              WHERE identidad_ref = ?2",
            params![ahora, variante],
        )?;
    }
    transaccion.execute(
        "UPDATE IDENTIDADES SET revocada_en = ?1 WHERE id = ?2",
        params![ahora, id],
    )?;
    transaccion.commit()?;
    Ok(hosts.len())
}

pub fn reactivar(conexion: &Connection, id: i64) -> Result<()> {
    conexion.execute(
        "UPDATE IDENTIDADES SET revocada_en = NULL WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}
