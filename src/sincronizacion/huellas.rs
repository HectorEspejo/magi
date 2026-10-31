//! Huellas de known_hosts que viajan con el inventario (Fase 10, §4.4).
//!
//! Al exportar, las claves registradas de cada host del inventario. Al
//! importar, una huella se añade solo si este equipo no tiene ninguna de ese
//! tipo para ese host; si tiene otra, nunca se sustituye sola: queda un
//! conflicto de tipo huella. Se reutiliza la lectura de conexion/huellas.rs,
//! que entiende las entradas hasheadas.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use russh::keys::known_hosts::known_host_keys_path;
use russh::keys::PublicKey;

use crate::almacen::conflictos::{Conflicto, TIPO_HUELLA};
use crate::conexion::huellas::{
    aprender, comprobar, huella as huella_sha256, sustituir, tipo_clave, EstadoHuella,
};
use crate::ficheros::copia_con_fecha;
use crate::modelo::ResultadoRegistro;
use crate::registro;
use crate::sincronizacion::formato::HuellaDocumento;
use crate::sincronizacion::fusion::ConflictoFusion;

/// Clave pública en base64, sin el tipo ni el comentario.
pub fn clave_base64(clave: &russh::keys::PublicKey) -> Result<String> {
    let linea = clave
        .to_openssh()
        .context("serializando una clave pública")?;
    Ok(linea
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_string())
}

/// Fecha en que este equipo aceptó la última huella del host: la última
/// anotación de REGISTRO de tipo huella o, si no hay, la última conexión.
pub fn aceptada_en(conexion: &Connection, host_id: i64) -> Result<String> {
    let fecha: Option<String> = conexion
        .query_row(
            "SELECT fecha FROM REGISTRO
              WHERE host_id = ?1 AND tipo IN ('huella_aceptada', 'huella_sustituida')
              ORDER BY id DESC LIMIT 1",
            params![host_id],
            |fila| fila.get(0),
        )
        .ok();
    if let Some(fecha) = fecha {
        return Ok(fecha);
    }
    Ok(conexion
        .query_row(
            "SELECT COALESCE(ultima_conexion_en, '') FROM HOSTS WHERE id = ?1",
            params![host_id],
            |fila| fila.get(0),
        )
        .unwrap_or_default())
}

/// Exporta las huellas de todos los hosts del inventario.
pub fn exportar(conexion: &Connection, ruta_known_hosts: &Path) -> Result<Vec<HuellaDocumento>> {
    let mut sentencia = conexion.prepare(
        "SELECT id, uuid, direccion, puerto FROM HOSTS WHERE uuid IS NOT NULL ORDER BY id",
    )?;
    let filas = sentencia.query_map([], |fila| {
        Ok((
            fila.get::<_, i64>(0)?,
            fila.get::<_, String>(1)?,
            fila.get::<_, String>(2)?,
            fila.get::<_, i64>(3)? as u16,
        ))
    })?;
    let hosts: Vec<(i64, String, String, u16)> = filas.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut huellas = Vec::new();
    for (id, uuid, direccion, puerto) in hosts {
        let aceptada = aceptada_en(conexion, id)?;
        let entradas = known_host_keys_path(&direccion, puerto, ruta_known_hosts)
            .with_context(|| format!("leyendo {}", ruta_known_hosts.display()))?;
        for (_, clave) in entradas {
            huellas.push(HuellaDocumento {
                host: uuid.clone(),
                tipo: crate::conexion::huellas::tipo_clave(&clave),
                clave: clave_base64(&clave)?,
                aceptada_en: aceptada.clone(),
            });
        }
    }
    Ok(huellas)
}

/// Lo que hay que hacer al importar huellas.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResultadoHuellas {
    /// Hosts (por nombre) a los que se añadió una huella; una entrada por
    /// huella añadida.
    pub anadidas: Vec<String>,
    /// Conflictos de tipo huella.
    pub conflictos: Vec<ConflictoFusion>,
    /// Avisos legibles (hosts inexistentes, ficheros que no se pueden leer).
    pub avisos: Vec<String>,
}

/// Un host del inventario al que pertenece una huella.
struct HostHuella {
    id: i64,
    nombre: String,
    direccion: String,
    puerto: u16,
}

/// Busca en HOSTS el host con ese uuid. None si no está en el inventario.
fn host_por_uuid(conexion: &Connection, uuid: &str) -> Result<Option<HostHuella>> {
    let fila = conexion
        .query_row(
            "SELECT id, nombre, direccion, puerto FROM HOSTS WHERE uuid = ?1",
            params![uuid],
            |fila| {
                Ok((
                    fila.get::<_, i64>(0)?,
                    fila.get::<_, String>(1)?,
                    fila.get::<_, String>(2)?,
                    fila.get::<_, i64>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((id, nombre, direccion, puerto)) = fila else {
        return Ok(None);
    };
    let puerto = u16::try_from(puerto).map_err(|_| {
        anyhow::anyhow!("el host «{nombre}» tiene el puerto {puerto} fuera de rango")
    })?;
    Ok(Some(HostHuella {
        id,
        nombre,
        direccion,
        puerto,
    }))
}

/// Clave pública a partir del tipo y el base64 del documento.
fn clave_publica(tipo: &str, clave: &str) -> Result<PublicKey> {
    let tipo = tipo.trim();
    let clave = clave.trim();
    if tipo.is_empty() || clave.is_empty() {
        bail!("la huella no tiene tipo o no tiene clave");
    }
    PublicKey::from_openssh(&format!("{tipo} {clave}"))
        .with_context(|| format!("la clave pública «{tipo}» no se puede leer"))
}

/// Una entrada de known_hosts de este equipo, ya normalizada.
struct EntradaConocida {
    tipo: String,
    clave: String,
    huella: String,
}

/// Entradas de known_hosts para la dirección de un host (hasheadas incluidas).
fn entradas_de(host: &HostHuella, ruta_known_hosts: &Path) -> Result<Vec<EntradaConocida>> {
    let entradas = known_host_keys_path(&host.direccion, host.puerto, ruta_known_hosts)
        .with_context(|| format!("leyendo {}", ruta_known_hosts.display()))?;
    let mut salida = Vec::with_capacity(entradas.len());
    for (_, clave) in entradas {
        salida.push(EntradaConocida {
            tipo: tipo_clave(&clave),
            clave: clave_base64(&clave)?,
            huella: huella_sha256(&clave),
        });
    }
    Ok(salida)
}

/// Equipo al que se puede atribuir una huella ajena. La pasada entrega todas
/// las huellas en una lista plana, sin decir de qué fichero viene cada una: si
/// solo hay un equipo en juego se atribuye a él, y con varios se deja sin
/// atribuir (el formato del fichero no incluye el autor por huella).
fn equipo_unico(
    equipos: &BTreeMap<String, (String, String)>,
) -> Option<(&String, &(String, String))> {
    if equipos.len() == 1 {
        equipos.iter().next()
    } else {
        None
    }
}

/// Una huella ajena lista para el conflicto y para elegir la ganadora.
struct HuellaAjena {
    tipo: String,
    clave: String,
    huella: String,
    aceptada_en: String,
}

/// Orden canónico de las huellas ajenas: la más reciente al final.
fn orden_ajenas(a: &HuellaAjena, b: &HuellaAjena) -> std::cmp::Ordering {
    a.aceptada_en
        .cmp(&b.aceptada_en)
        .then_with(|| a.tipo.cmp(&b.tipo))
        .then_with(|| a.clave.cmp(&b.clave))
}

/// JSON de la versión perdedora de un conflicto de huella: las mías y las
/// ajenas para ese host.
fn version_perdedora_json(
    mias: &[EntradaConocida],
    aceptada_en_mias: &str,
    ajenas: &[HuellaAjena],
    equipos: &BTreeMap<String, (String, String)>,
) -> serde_json::Value {
    let (equipo, datos_equipo) = match equipo_unico(equipos) {
        Some((uuid, datos)) => (
            serde_json::Value::String(uuid.clone()),
            Some((datos.0.clone(), datos.1.clone())),
        ),
        None => (serde_json::Value::Null, None),
    };
    let (nombre, generado_en) = match datos_equipo {
        Some((nombre, generado_en)) => (
            serde_json::Value::String(nombre),
            serde_json::Value::String(generado_en),
        ),
        None => (serde_json::Value::Null, serde_json::Value::Null),
    };
    let mias_json: Vec<serde_json::Value> = mias
        .iter()
        .map(|entrada| {
            serde_json::json!({
                "tipo": entrada.tipo,
                "clave": entrada.clave,
                "huella": entrada.huella,
                "aceptada_en": aceptada_en_mias,
            })
        })
        .collect();
    let suyas_json: Vec<serde_json::Value> = ajenas
        .iter()
        .map(|ajena| {
            serde_json::json!({
                "equipo": equipo,
                "nombre": nombre,
                "generado_en": generado_en,
                "tipo": ajena.tipo,
                "clave": ajena.clave,
                "huella": ajena.huella,
                "aceptada_en": ajena.aceptada_en,
            })
        })
        .collect();
    serde_json::json!({ "mias": mias_json, "suyas": suyas_json })
}

/// Huella ajena que gana el conflicto: la aceptada más recientemente de ese
/// tipo, con la clave como desempate. Devuelve el índice en ajenas.
fn indice_ganadora(ajenas: &[HuellaAjena], tipo: &str) -> Option<usize> {
    let mut candidatas: Vec<usize> = ajenas
        .iter()
        .enumerate()
        .filter(|(_, ajena)| ajena.tipo == tipo)
        .map(|(indice, _)| indice)
        .collect();
    candidatas.sort_by(|a, b| orden_ajenas(&ajenas[*a], &ajenas[*b]));
    candidatas.last().copied()
}

/// Importa las huellas de un documento: añade las que este equipo no tiene y
/// deja conflicto si ya había otra del mismo tipo. No toca known_hosts salvo
/// para añadir. La lista equipos mapea uuid del equipo → (nombre, generado_en).
pub fn importar(
    conexion: &Connection,
    ruta_known_hosts: &Path,
    entrantes: &[HuellaDocumento],
    equipos: &BTreeMap<String, (String, String)>,
) -> Result<ResultadoHuellas> {
    let mut resultado = ResultadoHuellas::default();
    if entrantes.is_empty() {
        return Ok(resultado);
    }

    // Agrupa por host para leer known_hosts y HOSTS una vez por host.
    let mut por_host: BTreeMap<&str, Vec<&HuellaDocumento>> = BTreeMap::new();
    for huella in entrantes {
        por_host
            .entry(huella.host.as_str())
            .or_default()
            .push(huella);
    }

    for (host_uuid, del_host) in por_host {
        let Some(host) = host_por_uuid(conexion, host_uuid)
            .with_context(|| format!("buscando el host con uuid {host_uuid}"))?
        else {
            resultado.avisos.push(format!(
                "no se importan las huellas de un host que no está en este inventario (uuid {host_uuid})"
            ));
            continue;
        };

        // Huellas ajenas únicas (mismo tipo y clave puede venir de varios
        // equipos) y ordenadas para que la pasada sea determinista.
        let mut vistas: BTreeSet<(String, String)> = BTreeSet::new();
        let mut ajenas: Vec<HuellaAjena> = Vec::new();
        let mut ordenadas: Vec<&HuellaDocumento> = del_host.clone();
        ordenadas.sort_by(|a, b| {
            (&a.tipo, &a.clave, &a.aceptada_en, &a.host).cmp(&(
                &b.tipo,
                &b.clave,
                &b.aceptada_en,
                &b.host,
            ))
        });
        for huella in ordenadas {
            let tipo = huella.tipo.trim().to_string();
            let clave = huella.clave.trim().to_string();
            if !vistas.insert((tipo.clone(), clave.clone())) {
                continue;
            }
            match clave_publica(&tipo, &clave) {
                Ok(clave_parseada) => ajenas.push(HuellaAjena {
                    tipo: tipo_clave(&clave_parseada),
                    clave: clave_base64(&clave_parseada)?,
                    huella: huella_sha256(&clave_parseada),
                    aceptada_en: huella.aceptada_en.clone(),
                }),
                Err(error) => resultado.avisos.push(format!(
                    "huella no válida para «{}»: {error:#}",
                    host.nombre
                )),
            }
        }
        ajenas.sort_by(orden_ajenas);
        if ajenas.is_empty() {
            continue;
        }

        let mut tipos_con_conflicto: BTreeSet<String> = BTreeSet::new();
        for ajena in &ajenas {
            let Ok(clave_parseada) = clave_publica(&ajena.tipo, &ajena.clave) else {
                continue;
            };
            let estado = match comprobar(
                ruta_known_hosts,
                &host.direccion,
                host.puerto,
                &clave_parseada,
            ) {
                Ok(estado) => estado,
                Err(error) => {
                    resultado.avisos.push(format!(
                        "no se pudieron leer las huellas de «{}»: {error:#}",
                        host.nombre
                    ));
                    continue;
                }
            };
            match estado {
                // Este equipo ya tiene exactamente esa huella: no hay nada que hacer.
                EstadoHuella::Conocida => {}
                // No tiene ninguna de ese tipo: se añade a known_hosts con copia previa.
                EstadoHuella::Desconocida => {
                    if let Err(error) = copia_con_fecha(ruta_known_hosts) {
                        resultado.avisos.push(format!(
                            "no se pudo copiar known_hosts antes de añadir la huella de «{}»: {error:#}",
                            host.nombre
                        ));
                        continue;
                    }
                    if let Err(error) = aprender(
                        ruta_known_hosts,
                        &host.direccion,
                        host.puerto,
                        &clave_parseada,
                    ) {
                        resultado.avisos.push(format!(
                            "no se pudo añadir la huella de «{}» a known_hosts: {error:#}",
                            host.nombre
                        ));
                        continue;
                    }
                    let origen = match equipo_unico(equipos) {
                        Some((_, (nombre, _))) => format!(" (sincronizada desde «{nombre}»)"),
                        None => " (sincronizada)".to_string(),
                    };
                    registro::anotar(
                        conexion,
                        registro::HUELLA_SINCRONIZADA,
                        Some(host.id),
                        None,
                        &format!(
                            "«{}»: añadida la huella {} {}{}",
                            host.nombre, ajena.tipo, ajena.huella, origen
                        ),
                        ResultadoRegistro::Ok,
                    )?;
                    resultado.anadidas.push(host.nombre.clone());
                }
                // Tiene otra del mismo tipo: nunca se sustituye sola.
                EstadoHuella::Cambiada { .. } => {
                    if tipos_con_conflicto.contains(&ajena.tipo) {
                        continue;
                    }
                    let Some(indice) = indice_ganadora(&ajenas, &ajena.tipo) else {
                        continue;
                    };
                    let ganadora = &ajenas[indice];
                    let mias = match entradas_de(&host, ruta_known_hosts) {
                        Ok(mias) => mias,
                        Err(error) => {
                            resultado.avisos.push(format!(
                                "no se pudieron leer las huellas de «{}»: {error:#}",
                                host.nombre
                            ));
                            continue;
                        }
                    };
                    let aceptada_mias = aceptada_en(conexion, host.id).unwrap_or_default();
                    let json = version_perdedora_json(&mias, &aceptada_mias, &ajenas, equipos);
                    let conflicto = ConflictoFusion {
                        tipo: TIPO_HUELLA.to_string(),
                        elemento_tipo: "host".to_string(),
                        uuid: host_uuid.to_string(),
                        nombre: host.nombre.clone(),
                        version_perdedora_json: Some(json.to_string()),
                        version_ganadora: Some(ganadora.clave.clone()),
                        equipo_ganador: equipo_unico(equipos).map(|(uuid, _)| uuid.clone()),
                    };
                    tipos_con_conflicto.insert(ajena.tipo.clone());
                    if !resultado.conflictos.contains(&conflicto) {
                        resultado.conflictos.push(conflicto);
                    }
                }
            }
        }
    }
    Ok(resultado)
}

/// Acepta la huella de otro equipo para un host: sustituye la de este equipo,
/// con copia previa de known_hosts y anotación en el historial.
pub fn aceptar(
    conexion: &Connection,
    ruta_known_hosts: &Path,
    host_uuid: &str,
    tipo: &str,
    clave: &str,
) -> Result<()> {
    let host = host_por_uuid(conexion, host_uuid)?
        .with_context(|| format!("no existe ningún host con uuid {host_uuid}"))?;
    let clave = clave_publica(tipo, clave)?;
    // Si ya está esa misma huella no hay ningún efecto que anotar.
    if comprobar(ruta_known_hosts, &host.direccion, host.puerto, &clave)? == EstadoHuella::Conocida
    {
        return Ok(());
    }
    copia_con_fecha(ruta_known_hosts)
        .with_context(|| format!("copiando {}", ruta_known_hosts.display()))?;
    sustituir(ruta_known_hosts, &host.direccion, host.puerto, &clave)
        .with_context(|| format!("sustituyendo la huella de «{}»", host.nombre))?;
    registro::anotar(
        conexion,
        registro::HUELLA_SUSTITUIDA,
        Some(host.id),
        None,
        &format!(
            "«{}»: huella sustituida por la aceptada de otro equipo ({} {})",
            host.nombre,
            tipo_clave(&clave),
            huella_sha256(&clave)
        ),
        ResultadoRegistro::Ok,
    )?;
    Ok(())
}

/// La huella ajena que ganó un conflicto de tipo huella, lista para que la
/// vista CONFLICTOS muestre el detalle y aplique aceptar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatosHuellaGanadora {
    /// uuid del equipo autor; vacío si el conflicto no lo pudo atribuir.
    pub equipo: String,
    /// Nombre legible de ese equipo; vacío si no se conoce.
    pub nombre: String,
    /// Tipo de la clave ("ssh-ed25519", ...).
    pub tipo: String,
    /// Clave pública en base64, sin tipo ni comentario.
    pub clave: String,
    /// Cuándo la aceptó el equipo que la exportó.
    pub aceptada_en: String,
    /// Huella SHA-256 en el formato de OpenSSH ("SHA256:…"), para mostrar.
    pub huella: String,
}

/// Extrae del conflicto la huella ajena que ganó. Devuelve None si el
/// conflicto no es de tipo huella o si no se puede parsear.
pub fn datos_huella_ganadora(conflicto: &Conflicto) -> Option<DatosHuellaGanadora> {
    if conflicto.tipo != TIPO_HUELLA {
        return None;
    }
    let clave_ganadora = conflicto.version_ganadora.as_deref()?;
    let valor: serde_json::Value =
        serde_json::from_str(conflicto.version_perdedora_json.as_deref()?).ok()?;
    let suyas = valor.get("suyas")?.as_array()?;
    let elegida = suyas
        .iter()
        .find(|suya| suya.get("clave").and_then(serde_json::Value::as_str) == Some(clave_ganadora))
        .or_else(|| (suyas.len() == 1).then(|| &suyas[0]))?;
    let texto = |campo: &str| {
        elegida
            .get(campo)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let tipo = texto("tipo");
    let clave = texto("clave");
    if tipo.is_empty() || clave.is_empty() {
        return None;
    }
    Some(DatosHuellaGanadora {
        equipo: texto("equipo"),
        nombre: texto("nombre"),
        tipo,
        clave,
        aceptada_en: texto("aceptada_en"),
        huella: texto("huella"),
    })
}

/// Acepta la huella ganadora de un conflicto de tipo huella: la acción «a» de
/// la vista CONFLICTOS. Sustituye la de este equipo (con copia previa).
pub fn aceptar_conflicto(
    conexion: &Connection,
    ruta_known_hosts: &Path,
    conflicto: &Conflicto,
) -> Result<()> {
    if conflicto.tipo != TIPO_HUELLA {
        bail!(
            "el conflicto «{}» no es de huella; no se puede aceptar así",
            conflicto.tipo
        );
    }
    let datos = datos_huella_ganadora(conflicto).with_context(|| {
        format!(
            "el conflicto {} no tiene una huella ganadora que se pueda leer",
            conflicto.id
        )
    })?;
    aceptar(
        conexion,
        ruta_known_hosts,
        &conflicto.uuid,
        &datos.tipo,
        &datos.clave,
    )
}
