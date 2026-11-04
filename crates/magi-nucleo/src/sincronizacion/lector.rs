//! Pasada de sincronización en modo lector (Fase 11, §7.3 y §7.4).
//!
//! El móvil es lector: recibe la carpeta por `CarpetaSincronizacion`, lee los
//! ficheros de los demás equipos con el mismo código que el escritorio
//! (descifrar, firma, formato y equipo), fusiona con la misma fusión y aplica
//! el resultado en una transacción.
//!
//! Lo que NO hace, a propósito:
//!
//! - no exporta ni escribe en la carpeta (el trait ni siquiera tiene métodos
//!   de escritura),
//! - no hace copia de `magi.db` (el inventario del móvil es un espejo que se
//!   puede rehacer uniéndose otra vez),
//! - no pregunta por los borrados masivos (los confirma el escritorio que los
//!   recibe; §7.3 del informe de la Fase 11),
//! - no guarda conflictos de elemento, nombre ni huérfano: solo `huella` y
//!   `borrado`, que son los que se resuelven en el móvil.
//!
//! Los borrados de hosts y túneles con trabajo vivo en el móvil se aplazan
//! como conflicto `borrado` y sin lápida local; en la primera pasada sin
//! trabajo vivo se releen todos los ficheros y se aplican solos (§7.4).

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use rusqlite::Connection;

use crate::almacen::{conflictos, equipos as equipos_bd, versiones};
use crate::modelo::ResultadoRegistro;
use crate::registro;
use crate::rutas::Rutas;
use crate::trabajo_vivo::TrabajoVivo;

use super::aplicacion::{self, OpcionesAplicacion};
use super::carpeta::{self, uuid_de_nombre, CarpetaSincronizacion, EntradaCarpeta};
use super::cifrado::{self, ClaveSincronizacion};
use super::elementos;
use super::formato::Documento;
use super::fusion::{self, ConflictoFusion, PlanFusion};
use super::huellas;
use super::{
    aplicar_equipos_quitados, interpretar_fichero, plan_cambia, FalloLectura, FicheroLeido,
    ResumenImportacion,
};

/// Estado final de una pasada en modo lector, en los términos de la pantalla
/// Sincronización de la app (§6.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EstadoLectura {
    /// No hay ninguna clave de sincronización disponible.
    SinClave,
    /// Inventario al día. Puede haber conflictos de huella o borrados
    /// aplazados pendientes, que no impiden este estado.
    AlDia,
    /// El fichero de un equipo conocido no se abre con la clave actual: hay
    /// que volver a escanearla. Es un aviso no bloqueante (§7.13 R85): se
    /// sigue con el inventario actual y se reintenta en la siguiente pasada.
    EsperandoClave(String),
    /// Este equipo aparece quitado en el fichero de otro equipo.
    Quitado,
    /// No se puede leer la carpeta (permiso retirado, carpeta borrada…).
    ErrorCarpeta(String),
}

/// Resultado completo de una pasada en modo lector, para la app y la pantalla
/// Sincronización.
#[derive(Debug, Clone)]
pub struct ResultadoLectura {
    pub estado: EstadoLectura,
    /// Presente si esta pasada cambió el inventario (o importó huellas).
    pub resumen: Option<ResumenImportacion>,
    /// Avisos legibles y no bloqueantes (ficheros dañados, de formato nuevo,
    /// que no abren…).
    pub avisos: Vec<String>,
    /// Conflictos pendientes después de la pasada (huella y borrado).
    pub conflictos_pendientes: i64,
}

impl ResultadoLectura {
    fn nuevo(estado: EstadoLectura) -> Self {
        Self {
            estado,
            resumen: None,
            avisos: Vec::new(),
            conflictos_pendientes: 0,
        }
    }
}

/// ¿Hay algún borrado aplazado pendiente? Si lo hay, la pasada relee todos los
/// ficheros aunque su hash no cambie, porque la fusión necesita las lápidas
/// otra vez para decidir el borrado (§7.3 y §7.4).
pub fn hay_borrados_aplazados(conexion: &Connection) -> Result<bool> {
    Ok(conflictos::listar_pendientes(conexion)?
        .iter()
        .any(|conflicto| conflicto.tipo == conflictos::TIPO_BORRADO))
}

/// Tipos de conflicto que se guardan en el móvil (§7.3).
fn tipos_de_conflicto_que_se_guardan() -> BTreeSet<String> {
    [conflictos::TIPO_HUELLA, conflictos::TIPO_BORRADO]
        .into_iter()
        .map(String::from)
        .collect()
}

/// Una pasada completa en modo lector. No toma cerrojo (el móvil es el único
/// proceso que abre su `magi.db`), no escribe en la carpeta, no exporta y no
/// hace copia de la base de datos.
pub fn pasada_lector(
    conexion: &Connection,
    rutas: &Rutas,
    carpeta: &dyn CarpetaSincronizacion,
    claves: &[ClaveSincronizacion],
    trabajo_vivo: &dyn TrabajoVivo,
) -> Result<ResultadoLectura> {
    if claves.is_empty() {
        return Ok(ResultadoLectura::nuevo(EstadoLectura::SinClave));
    }
    let este = equipos_bd::este(conexion)?;
    if este.estado == equipos_bd::ESTADO_QUITADO {
        return Ok(ResultadoLectura::nuevo(EstadoLectura::Quitado));
    }
    let mut resultado = ResultadoLectura::nuevo(EstadoLectura::AlDia);

    // 1. Listar la carpeta. Es entrada no fiable (T81): además de lo que
    //    devuelva el servicio, se revalidan el nombre y el tamaño.
    let releer_todo = hay_borrados_aplazados(conexion)?;
    let entradas = match carpeta.listar() {
        Ok(entradas) => entradas,
        Err(error) => return Ok(ResultadoLectura::nuevo(EstadoLectura::ErrorCarpeta(error))),
    };
    let mut ficheros: Vec<(String, EntradaCarpeta)> = Vec::new();
    for entrada in entradas {
        let Some(uuid) = uuid_de_nombre(&entrada.nombre) else {
            // Copias en conflicto de la herramienta de sincronización,
            // temporales, subcarpetas y nombres no canónicos: no son ficheros
            // de equipo.
            continue;
        };
        if uuid == este.uuid {
            continue;
        }
        if entrada.tamano > carpeta::TAMANO_MAXIMO {
            resultado.avisos.push(format!(
                "se ignora «{}»: ocupa {} bytes y el máximo es {} (T81)",
                entrada.nombre,
                entrada.tamano,
                carpeta::TAMANO_MAXIMO
            ));
            continue;
        }
        ficheros.push((uuid, entrada));
    }
    ficheros.sort_by(|a, b| a.0.cmp(&b.0));

    // 2. Leer y validar los ficheros que hayan cambiado (todos si hay
    //    borrados aplazados pendientes).
    let mut leidos: Vec<FicheroLeido> = Vec::new();
    let mut fallos_de_lectura = 0usize;
    for (uuid, entrada) in &ficheros {
        let equipo = equipos_bd::por_uuid(conexion, uuid)?;
        if let Some(equipo) = &equipo {
            if equipo.estado == equipos_bd::ESTADO_QUITADO {
                continue;
            }
        }
        let bytes = match carpeta.leer(&entrada.nombre, carpeta::TAMANO_MAXIMO) {
            Ok(bytes) => bytes,
            Err(error) => {
                fallos_de_lectura += 1;
                resultado
                    .avisos
                    .push(format!("no se pudo leer «{}»: {error}", entrada.nombre));
                continue;
            }
        };
        // El servicio debe respetar el tope; se vuelve a comprobar porque la
        // carpeta es entrada no fiable (T81).
        if bytes.len() as u64 > carpeta::TAMANO_MAXIMO {
            resultado.avisos.push(format!(
                "se ignora «{}»: supera el tamaño máximo",
                entrada.nombre
            ));
            continue;
        }
        let hash = cifrado::sha256_hex(&bytes);
        if !releer_todo {
            if let Some(equipo) = &equipo {
                if equipo.hash_ultimo_fichero.as_deref() == Some(hash.as_str()) {
                    continue;
                }
            }
        }
        match interpretar_fichero(&bytes, claves, uuid) {
            Ok(leido) => {
                if equipo.is_none() {
                    equipos_bd::crear(
                        conexion,
                        uuid,
                        &leido.documento.equipo.nombre,
                        &leido.documento.equipo.plataforma,
                    )?;
                }
                leidos.push(leido);
            }
            Err(FalloLectura::SinClave) => {
                // Aviso no bloqueante: se sigue con los ficheros que sí abren
                // y con el inventario actual; se reintenta en la siguiente
                // pasada y vuelve solo a activa si el fichero vuelve a abrir.
                if let Some(equipo) = &equipo {
                    equipos_bd::fijar_estado(conexion, equipo.id, equipos_bd::ESTADO_ILEGIBLE)?;
                    if matches!(&resultado.estado, EstadoLectura::AlDia) {
                        resultado.estado = EstadoLectura::EsperandoClave(equipo.nombre.clone());
                    }
                    resultado.avisos.push(format!(
                        "el fichero de «{}» no se abre con la clave de este móvil; si la cambiaste en el escritorio, escanea la nueva",
                        equipo.nombre
                    ));
                } else {
                    resultado.avisos.push(format!(
                        "un fichero de la carpeta ({}) no se abre con esta clave",
                        entrada.nombre
                    ));
                }
            }
            Err(FalloLectura::Danado(motivo)) => {
                if let Some(equipo) = &equipo {
                    equipos_bd::fijar_estado(conexion, equipo.id, equipos_bd::ESTADO_DANADO)?;
                }
                let nombre = equipo
                    .as_ref()
                    .map(|equipo| equipo.nombre.clone())
                    .unwrap_or_else(|| uuid.clone());
                resultado.avisos.push(format!(
                    "el fichero de {nombre} está dañado ({motivo}); se reintentará"
                ));
            }
            Err(FalloLectura::FormatoNuevo(_)) => {
                if let Some(equipo) = &equipo {
                    equipos_bd::fijar_estado(
                        conexion,
                        equipo.id,
                        equipos_bd::ESTADO_FORMATO_NUEVO,
                    )?;
                }
                let nombre = equipo
                    .as_ref()
                    .map(|equipo| equipo.nombre.clone())
                    .unwrap_or_else(|| uuid.clone());
                resultado.avisos.push(format!(
                    "el equipo {nombre} usa una versión más nueva de MAGI; actualiza"
                ));
            }
            Err(FalloLectura::EquipoInvalido(motivo)) => {
                if let Some(equipo) = &equipo {
                    equipos_bd::fijar_estado(conexion, equipo.id, equipos_bd::ESTADO_DANADO)?;
                }
                resultado.avisos.push(motivo);
            }
        }
    }
    if !ficheros.is_empty() && leidos.is_empty() && fallos_de_lectura == ficheros.len() {
        resultado.estado = EstadoLectura::ErrorCarpeta(
            "no se pudo leer ningún fichero de la carpeta de sincronización".to_string(),
        );
        resultado.conflictos_pendientes = conflictos::contar_pendientes(conexion)?;
        return Ok(resultado);
    }

    // 3. Equipos quitados: si un documento dice que este equipo ya no
    //    sincroniza, se deja de importar. El fichero de un equipo que se acaba
    //    de descubrir quitado no cuenta en esta misma pasada.
    let documentos: Vec<Documento> = leidos.iter().map(|l| l.documento.clone()).collect();
    let quitados = aplicar_equipos_quitados(conexion, &documentos)?;
    leidos.retain(|leido| !quitados.contains_key(&leido.uuid));
    if quitados.contains_key(&este.uuid) {
        equipos_bd::marcar_quitado(conexion, este.id, &crate::modelo::fecha_ahora())?;
        registro::anotar(
            conexion,
            registro::INVENTARIO_SINC_ERROR,
            None,
            None,
            "este equipo ha sido quitado de la sincronización; únete de nuevo como equipo nuevo",
            ResultadoRegistro::Ok,
        )?;
        return Ok(ResultadoLectura::nuevo(EstadoLectura::Quitado));
    }

    // 4. Fusionar con la misma fusión que el escritorio (determinista).
    let documentos: Vec<Documento> = leidos.iter().map(|l| l.documento.clone()).collect();
    let local = elementos::estado_local(conexion, &rutas.hogar)?;
    let plan = fusion::fusionar(&local, &documentos)?;
    let cambia = plan_cambia(&plan, &local);

    let mut resumen = ResumenImportacion {
        equipos: leidos.len(),
        ..ResumenImportacion::default()
    };

    // 5. Aplicar, sin copia y sin preguntar. Los borrados de hosts y túneles
    //    con trabajo vivo se aplazan como conflicto `borrado`, sin lápida
    //    local; en la pasada siguiente se relee todo y se aplican solos.
    if cambia {
        let (aplazados, conflictos_borrado) = aplazar_borrados(&local, &plan, trabajo_vivo);
        versiones::al_importar(conexion, plan.reloj_ms)?;
        let max_conflicto: i64 = conexion
            .query_row("SELECT COALESCE(MAX(id), 0) FROM CONFLICTOS", [], |fila| {
                fila.get(0)
            })
            .unwrap_or(0);
        let opciones = OpcionesAplicacion {
            borrados_aplazados: aplazados,
            tipos_conflicto: Some(tipos_de_conflicto_que_se_guardan()),
        };
        let aplicado = aplicacion::aplicar(conexion, &plan, &opciones, &rutas.hogar)?;
        resumen.creados = aplicado.creados;
        resumen.actualizados = aplicado.actualizados;
        resumen.borrados = aplicado.borrados;
        for conflicto in &conflictos_borrado {
            conflictos::crear_si_nuevo(conexion, &nuevo_conflicto(conflicto))?;
        }
        // Conflictos nuevos de esta pasada: los de borrado aplazado (la
        // aplicación filtra el resto) para REGISTRO y el resumen.
        let nuevos = conflictos_desde(conexion, max_conflicto)?;
        resumen.conflictos = nuevos.len();
        for (tipo, elemento_tipo, nombre) in &nuevos {
            registro::anotar(
                conexion,
                registro::INVENTARIO_CONFLICTO,
                None,
                None,
                &format!("conflicto {tipo} de {elemento_tipo} «{nombre}»"),
                ResultadoRegistro::Ok,
            )?;
        }
        registro::anotar(
            conexion,
            registro::INVENTARIO_IMPORTADO,
            None,
            None,
            &format!(
                "equipos: {}, creados: {}, actualizados: {}, borrados: {}, conflictos: {}",
                resumen.equipos,
                resumen.creados,
                resumen.actualizados,
                resumen.borrados,
                resumen.conflictos
            ),
            ResultadoRegistro::Ok,
        )?;
    }

    // 6. Huellas: añadir solo si no hay ninguna de ese tipo; conflicto si hay
    //    otra. Nunca se sustituye sola (igual que el escritorio).
    let hay_huellas = documentos
        .iter()
        .any(|documento| !documento.huellas.is_empty());
    let hay_cambios_huellas = hay_huellas && !leidos.is_empty();
    if hay_huellas {
        let mut todas = Vec::new();
        let mut mapa_equipos: BTreeMap<String, (String, String)> = BTreeMap::new();
        for documento in &documentos {
            todas.extend(documento.huellas.iter().cloned());
            mapa_equipos.insert(
                documento.equipo.uuid.clone(),
                (
                    documento.equipo.nombre.clone(),
                    documento.generado_en.clone(),
                ),
            );
        }
        let resultado_huellas = huellas::importar(
            conexion,
            &rutas.fichero_known_hosts(),
            &todas,
            &mapa_equipos,
        )?;
        resumen.huellas = resultado_huellas.anadidas.len();
        for conflicto in &resultado_huellas.conflictos {
            if conflictos::crear_si_nuevo(conexion, &nuevo_conflicto(conflicto))?.is_some() {
                resumen.conflictos += 1;
                registro::anotar(
                    conexion,
                    registro::INVENTARIO_CONFLICTO,
                    None,
                    None,
                    &format!(
                        "conflicto {} de {} «{}»",
                        conflicto.tipo, conflicto.elemento_tipo, conflicto.nombre
                    ),
                    ResultadoRegistro::Ok,
                )?;
            }
        }
        resultado.avisos.extend(resultado_huellas.avisos);
    }

    // 7. Marcar los ficheros leídos como importados.
    for leido in &leidos {
        if let Some(equipo) = equipos_bd::por_uuid(conexion, &leido.uuid)? {
            equipos_bd::fijar_ultimo_fichero(
                conexion,
                equipo.id,
                &leido.hash,
                &leido.documento.generado_en,
                Some(&leido.documento.app),
            )?;
            equipos_bd::fijar_importacion(conexion, equipo.id, &leido.documento.generado_en)?;
        }
    }

    // 8. Borrados aplazados que dejan de estarlo: si el elemento resucitó, se
    //    marca `recuperado`; si ya no existe, el borrado se aplicó y se marca
    //    `aceptado`. Sin esto, un conflicto pendiente obligaría a releer todos
    //    los ficheros en cada pasada para siempre.
    // Si algún fichero no se pudo leer, la fusión es parcial: no se resuelven
    // los borrados aplazados o se daría por «recuperado» un host que solo
    // parecía vivo por no haber leído la lápida.
    if fallos_de_lectura == 0 && !matches!(resultado.estado, EstadoLectura::EsperandoClave(_)) {
        resolver_borrados_obsoletos(conexion, &plan)?;
    }

    if cambia || hay_cambios_huellas {
        resultado.resumen = Some(resumen);
    }
    resultado.conflictos_pendientes = conflictos::contar_pendientes(conexion)?;
    Ok(resultado)
}

/// Separa los borrados que hay que aplazar por trabajo vivo de los que se
/// aplicarán. Devuelve los uuids aplazados y los conflictos `borrado` que hay
/// que guardar.
fn aplazar_borrados(
    local: &[fusion::Candidato],
    plan: &PlanFusion,
    trabajo_vivo: &dyn TrabajoVivo,
) -> (BTreeSet<String>, Vec<ConflictoFusion>) {
    let mut aplazados = BTreeSet::new();
    let mut conflictos = Vec::new();
    let vivos_plan: BTreeSet<&str> = plan.vivos.iter().map(|vivo| vivo.uuid.as_str()).collect();
    for candidato in local {
        if candidato.contenido.is_none() || vivos_plan.contains(candidato.uuid.as_str()) {
            continue;
        }
        if candidato.tipo != "host" && candidato.tipo != "tunel" {
            continue;
        }
        if !trabajo_vivo.tiene_trabajo(&candidato.tipo, &candidato.uuid) {
            continue;
        }
        aplazados.insert(candidato.uuid.clone());
        conflictos.push(ConflictoFusion {
            tipo: conflictos::TIPO_BORRADO.to_string(),
            elemento_tipo: candidato.tipo.clone(),
            uuid: candidato.uuid.clone(),
            nombre: candidato.nombre.clone(),
            version_perdedora_json: None,
            version_ganadora: None,
            equipo_ganador: None,
        });
    }
    (aplazados, conflictos)
}

/// Resuelve los conflictos de borrado que ya no lo están: el elemento volvió
/// (una edición nueva lo resucitó) o el borrado se aplicó en esta pasada.
fn resolver_borrados_obsoletos(conexion: &Connection, plan: &PlanFusion) -> Result<()> {
    for conflicto in conflictos::listar_pendientes(conexion)? {
        if conflicto.tipo != conflictos::TIPO_BORRADO {
            continue;
        }
        let vivo = plan.vivos.iter().any(|candidato| {
            candidato.uuid == conflicto.uuid && candidato.tipo == conflicto.elemento_tipo
        });
        if vivo {
            conflictos::resolver(conexion, conflicto.id, conflictos::RESOLUCION_RECUPERADO)?;
            continue;
        }
        if !elemento_existe(conexion, &conflicto.elemento_tipo, &conflicto.uuid)? {
            conflictos::resolver(conexion, conflicto.id, conflictos::RESOLUCION_ACEPTADO)?;
        }
    }
    Ok(())
}

/// ¿Sigue existiendo la fila del elemento? Solo los tipos que pueden tener un
/// borrado aplazado (host y túnel); un tipo desconocido se da por vivo para no
/// resolver un conflicto que no entendemos.
fn elemento_existe(conexion: &Connection, elemento_tipo: &str, uuid: &str) -> Result<bool> {
    let tabla = match elemento_tipo {
        "host" => "HOSTS",
        "tunel" => "TUNELES",
        _ => return Ok(true),
    };
    let existe: i64 = conexion.query_row(
        &format!("SELECT COUNT(*) FROM {tabla} WHERE uuid = ?1"),
        rusqlite::params![uuid],
        |fila| fila.get(0),
    )?;
    Ok(existe > 0)
}

/// Copia un conflicto de la fusión al almacén.
fn nuevo_conflicto(conflicto: &ConflictoFusion) -> conflictos::NuevoConflicto {
    conflictos::NuevoConflicto {
        tipo: conflicto.tipo.clone(),
        elemento_tipo: conflicto.elemento_tipo.clone(),
        uuid: conflicto.uuid.clone(),
        nombre: conflicto.nombre.clone(),
        version_perdedora_json: conflicto.version_perdedora_json.clone(),
        version_ganadora: conflicto.version_ganadora.clone(),
        equipo_ganador: conflicto.equipo_ganador.clone(),
    }
}

/// Conflictos creados después de `desde_id`, en orden de creación.
fn conflictos_desde(conexion: &Connection, desde_id: i64) -> Result<Vec<(String, String, String)>> {
    let mut sentencia = conexion
        .prepare("SELECT tipo, elemento_tipo, nombre FROM CONFLICTOS WHERE id > ?1 ORDER BY id")?;
    let filas = sentencia.query_map(rusqlite::params![desde_id], |fila| {
        Ok((
            fila.get::<_, String>(0)?,
            fila.get::<_, String>(1)?,
            fila.get::<_, String>(2)?,
        ))
    })?;
    Ok(filas.collect::<rusqlite::Result<Vec<_>>>()?)
}
