//! Aplicación de un plan de fusión al almacén (Fase 10, §7.3).
//!
//! Escribe las versiones del plan tal cual (nunca por el camino que sube
//! versión), resuelve las referencias por uuid, reconstruye ETIQUETAS, deja
//! lápida de lo borrado y registra los conflictos. Todo dentro de una
//! transacción, bajo el cerrojo de la sincronización y con copia previa de
//! magi.db (la copia la hace la pasada, no esto).
//!
//! Las reglas del formato están en docs/formato-sincronizacion.md (§11). Lo que
//! no viaja (campos propios del equipo) no se toca al actualizar: se conserva
//! lo que ya hubiera, y al insertar una fila nueva se rellena con lo que
//! corresponda por defecto.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use crate::almacen::conflictos::{self, Conflicto, NuevoConflicto};
use crate::almacen::lapidas::{self, Lapida};
use crate::almacen::versiones::{self, Tabla};
use crate::deliberacion::validar_verificaciones;
use crate::modelo::{fecha_ahora, IdentidadRef, Origen, OrigenIdentidad, TipoTunel};
use crate::protocolo::Direccion;
use crate::sincronizacion::elementos::{
    ruta_desde_home, DatosGrupo, DatosHost, DatosIdentidad, DatosSincronizacion, DatosSnippet,
    DatosTunel, DatosVerificaciones, DestinoSnippet,
};
use crate::sincronizacion::fusion::{ConflictoFusion, PlanFusion};

/// Prefijo de los nombres temporales: mientras se aplica un plan, las filas
/// que se van a reescribir o borrar se apartan con este nombre para que los
/// UNIQUE de nombre nunca choquen a mitad de la transacción.
const PREFIJO_TEMPORAL: &str = "__magi_sync__";

/// Opciones para aplicar un plan de fusión al almacén.
#[derive(Debug, Clone, Default)]
pub struct OpcionesAplicacion {
    /// uuids cuyo borrado se aplaza porque tienen trabajo vivo: no se borran.
    pub borrados_aplazados: BTreeSet<String>,
}

/// Resumen de lo aplicado, para REGISTRO y para la vista.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResumenAplicacion {
    pub creados: usize,
    pub actualizados: usize,
    pub borrados: usize,
    pub renombrados: usize,
    pub conflictos: usize,
}

// --------------------------------------------------------------- datos del plan

/// Un elemento vivo del plan, ya validado y tipado.
#[derive(Debug, Clone)]
struct VivoPlan<D> {
    version: String,
    vector: BTreeMap<String, i64>,
    datos: D,
}

/// Los elementos vivos del plan ordenados por tabla. Solo hay tipos conocidos:
/// un tipo que este binario no entiende se ignora con aviso (§14 del formato).
#[derive(Debug, Clone, Default)]
struct VivosPlan {
    grupos: BTreeMap<String, VivoPlan<DatosGrupo>>,
    hosts: BTreeMap<String, VivoPlan<DatosHost>>,
    identidades: BTreeMap<String, VivoPlan<DatosIdentidad>>,
    tuneles: BTreeMap<String, VivoPlan<DatosTunel>>,
    snippets: BTreeMap<String, VivoPlan<DatosSnippet>>,
    sincronizaciones: BTreeMap<String, VivoPlan<DatosSincronizacion>>,
}

impl VivosPlan {
    /// uuids vivos de cada tabla, para calcular borrados y reconstruir índices.
    fn uuids_por_tabla(&self) -> HashMap<Tabla, BTreeSet<String>> {
        let mut mapas: HashMap<Tabla, BTreeSet<String>> = HashMap::new();
        for tabla in Tabla::todas() {
            mapas.insert(tabla, BTreeSet::new());
        }
        mapas
            .get_mut(&Tabla::Grupos)
            .expect("tabla creada")
            .extend(self.grupos.keys().cloned());
        mapas
            .get_mut(&Tabla::Hosts)
            .expect("tabla creada")
            .extend(self.hosts.keys().cloned());
        mapas
            .get_mut(&Tabla::Identidades)
            .expect("tabla creada")
            .extend(self.identidades.keys().cloned());
        mapas
            .get_mut(&Tabla::Tuneles)
            .expect("tabla creada")
            .extend(self.tuneles.keys().cloned());
        mapas
            .get_mut(&Tabla::Snippets)
            .expect("tabla creada")
            .extend(self.snippets.keys().cloned());
        mapas
            .get_mut(&Tabla::SincronizacionesDir)
            .expect("tabla creada")
            .extend(self.sincronizaciones.keys().cloned());
        mapas
    }

    /// Versión y vector del candidato vivo de esa tabla, para decidir si una
    /// fila local necesita reescribirse.
    fn metadatos(&self, tabla: Tabla, uuid: &str) -> Option<(&str, &BTreeMap<String, i64>)> {
        match tabla {
            Tabla::Grupos => self
                .grupos
                .get(uuid)
                .map(|vivo| (vivo.version.as_str(), &vivo.vector)),
            Tabla::Hosts => self
                .hosts
                .get(uuid)
                .map(|vivo| (vivo.version.as_str(), &vivo.vector)),
            Tabla::Identidades => self
                .identidades
                .get(uuid)
                .map(|vivo| (vivo.version.as_str(), &vivo.vector)),
            Tabla::Tuneles => self
                .tuneles
                .get(uuid)
                .map(|vivo| (vivo.version.as_str(), &vivo.vector)),
            Tabla::Snippets => self
                .snippets
                .get(uuid)
                .map(|vivo| (vivo.version.as_str(), &vivo.vector)),
            Tabla::SincronizacionesDir => self
                .sincronizaciones
                .get(uuid)
                .map(|vivo| (vivo.version.as_str(), &vivo.vector)),
        }
    }
}

/// El plan de fusión ya validado y tipado.
#[derive(Debug, Clone)]
struct PlanLeido {
    vivos: VivosPlan,
    /// Lápidas del plan por uuid (todas las del universo).
    lapidas: BTreeMap<String, Lapida>,
    /// uuids que hay que tratar como edición nueva al escribir (renombrados).
    renombrados: HashMap<Tabla, BTreeSet<String>>,
}

/// Fila local con lo justo para decidir actualizaciones y borrados.
#[derive(Debug, Clone)]
struct FilaLocal {
    id: i64,
    /// Nombre natural tal como estaba antes de apartarlo para aplicar el plan.
    nombre: String,
    version: Option<String>,
    vector: Option<String>,
    /// Solo en TUNELES y SINCRONIZACIONES_DIR: host actual.
    host_id: Option<i64>,
}

// ------------------------------------------------------------------ entrada

/// Aplica un plan de fusión en una transacción, escribiendo las versiones tal
/// cual vienen del fichero. Devuelve el resumen.
pub fn aplicar(
    conexion: &Connection,
    plan: &PlanFusion,
    opciones: &OpcionesAplicacion,
    hogar: &Path,
) -> Result<ResumenAplicacion> {
    let transaccion = conexion.unchecked_transaction()?;
    // El reloj híbrido de este equipo sube a la mayor versión vista antes de
    // emitir ninguna edición nueva (los renombrados).
    versiones::al_importar(&transaccion, plan.reloj_ms)?;

    let leido = leer_plan(plan)?;
    let vivos_uuids = leido.vivos.uuids_por_tabla();

    // Estado local antes de tocar nada: uuid → fila, y etiquetas por host para
    // no perder las de las filas que se quedan sin reescribir (borrados
    // aplazados o filas ajenas a la sincronización).
    let mut locales: HashMap<Tabla, BTreeMap<String, FilaLocal>> = HashMap::new();
    for tabla in Tabla::todas() {
        locales.insert(tabla, leer_locales(&transaccion, tabla)?);
    }
    let etiquetas_anteriores = crate::almacen::etiquetas::por_host(&transaccion)?;

    // Qué filas locales se reescriben y cuáles se borran.
    let mut a_actualizar: HashMap<Tabla, BTreeSet<String>> = HashMap::new();
    let mut a_borrar: HashMap<Tabla, BTreeSet<String>> = HashMap::new();
    for tabla in Tabla::todas() {
        let vacio = BTreeMap::new();
        let locales_tabla = locales.get(&tabla).unwrap_or(&vacio);
        let vivos_tabla = vivos_uuids.get(&tabla).cloned().unwrap_or_default();
        let renombrados_tabla = leido.renombrados.get(&tabla);
        let mut actualizar = BTreeSet::new();
        let mut borrar = BTreeSet::new();
        for (uuid, fila) in locales_tabla {
            if vivos_tabla.contains(uuid) {
                let forzado = renombrados_tabla.is_some_and(|uuids| uuids.contains(uuid));
                if let Some((version, vector)) = leido.vivos.metadatos(tabla, uuid) {
                    if necesita_actualizar(fila, version, vector, forzado) {
                        actualizar.insert(uuid.clone());
                    }
                }
            } else if !opciones.borrados_aplazados.contains(uuid) {
                // Una fila que el plan no trae ni en vivos ni en lápidas no
                // participó en la fusión: es una fila que la exportación omite
                // (datos ilegibles o sin versión/vector). No se borra, que el
                // usuario la vería desaparecer sin haberla tocado. Solo se borra
                // una fila exportable que el plan omitiera, con una lápida
                // mínima.
                let tiene_lapida = leido.lapidas.contains_key(uuid);
                if tiene_lapida
                    || (fila_participa_en_la_fusion(fila)
                        && fila_exportable(&transaccion, tabla, fila.id))
                {
                    borrar.insert(uuid.clone());
                } else {
                    tracing::warn!(
                        tabla = tabla.nombre(),
                        uuid,
                        "fila que no participa en la fusión: se conserva"
                    );
                }
            }
        }
        a_actualizar.insert(tabla, actualizar);
        a_borrar.insert(tabla, borrar);
    }

    // 1. Nombres temporales: se apartan todas las filas que se van a reescribir
    //    o borrar para que las permutas de nombre y los UNIQUE no choquen.
    for tabla in Tabla::todas() {
        apartar_nombres(
            &transaccion,
            tabla,
            locales.get(&tabla),
            a_actualizar.get(&tabla),
            a_borrar.get(&tabla),
        )?;
    }

    // 2. Borrados: primero la lápida (la del plan o una mínima con lo que había)
    //    y después el DELETE, que arrastra en cascada los hijos de SQLite.
    //    Los hijos van antes que los padres para contarlos una sola vez.
    let orden_borrado = [
        Tabla::Tuneles,
        Tabla::SincronizacionesDir,
        Tabla::Snippets,
        Tabla::Hosts,
        Tabla::Grupos,
        Tabla::Identidades,
    ];
    let mut resumen = ResumenAplicacion::default();
    for tabla in orden_borrado {
        let vacio = BTreeMap::new();
        let locales_tabla = locales.get(&tabla).unwrap_or(&vacio);
        let algunas = a_borrar.get(&tabla).cloned().unwrap_or_default();
        for uuid in &algunas {
            let Some(fila) = locales_tabla.get(uuid) else {
                continue;
            };
            let lapida = match leido.lapidas.get(uuid) {
                Some(lapida) => lapida.clone(),
                None => lapida_minima(&transaccion, tabla, uuid, fila)?,
            };
            lapidas::guardar(&transaccion, &lapida)?;
            let borradas = transaccion
                .execute(
                    &format!("DELETE FROM {} WHERE id = ?1", tabla.nombre()),
                    params![fila.id],
                )
                .with_context(|| format!("borrando el {} {uuid}", tabla.tipo_elemento()))?;
            resumen.borrados += borradas;
        }
    }

    // 3. Altas y actualizaciones. Primero las tablas sin referencias a otras
    //    (grupos, snippets, identidades y hosts sin sus referencias), luego se
    //    resuelven los uuid a id locales en una segunda pasada.
    escribir_grupos(
        &transaccion,
        &leido.vivos,
        &locales,
        &a_actualizar,
        &mut resumen,
    )?;
    escribir_snippets(
        &transaccion,
        &leido.vivos,
        &locales,
        &a_actualizar,
        &mut resumen,
    )?;
    escribir_identidades(
        &transaccion,
        &leido.vivos,
        &locales,
        &a_actualizar,
        &mut resumen,
    )?;
    escribir_hosts(
        &transaccion,
        &leido.vivos,
        &locales,
        &a_actualizar,
        hogar,
        &mut resumen,
    )?;

    // 4. Segunda pasada: id local de cada uuid vivo. Solo los vivos resuelven
    //    referencias; lo que tiene lápida se queda vacío.
    let mut ids: HashMap<Tabla, HashMap<String, i64>> = HashMap::new();
    for tabla in Tabla::todas() {
        let vivos_tabla = vivos_uuids.get(&tabla).cloned().unwrap_or_default();
        ids.insert(tabla, leer_ids(&transaccion, tabla, Some(&vivos_tabla))?);
    }

    fijar_referencias_de_hosts(&transaccion, &leido.vivos, &ids)?;
    escribir_tuneles(
        &transaccion,
        &leido.vivos,
        &locales,
        &a_actualizar,
        &ids,
        &mut resumen,
    )?;
    escribir_sincronizaciones(
        &transaccion,
        &leido.vivos,
        &locales,
        &a_actualizar,
        &ids,
        hogar,
        &mut resumen,
    )?;

    // 5. Hijos que viajan dentro del padre: destinos, etiquetas y
    //    verificaciones.
    reemplazar_destinos_de_snippets(&transaccion, &leido.vivos, &ids)?;
    reconstruir_etiquetas(&transaccion, &leido.vivos, &ids, &etiquetas_anteriores)?;
    actualizar_verificaciones_de_hosts(&transaccion, &leido.vivos, &ids)?;

    // 6. Lápidas del plan: todas se conservan siempre (también las de los
    //    elementos resucitados y las de los borrados aplazados).
    for lapida in leido.lapidas.values() {
        lapidas::guardar(&transaccion, lapida)?;
    }

    // 7. Renombrados: se escribieron ya con el nombre nuevo (el contenido del
    //    plan lo trae); ahora se marcan como edición nueva de este equipo.
    for (tabla, uuids) in &leido.renombrados {
        for uuid in uuids {
            if aplicar_renombrado(&transaccion, *tabla, uuid, &ids, &vivos_uuids)? {
                resumen.renombrados += 1;
            }
        }
    }

    // 8. Conflictos: los que ya estaban pendientes no se duplican ni cuentan.
    for conflicto in &plan.conflictos {
        let nuevo = nuevo_conflicto(conflicto);
        if conflictos::crear_si_nuevo(&transaccion, &nuevo)?.is_some() {
            resumen.conflictos += 1;
        }
    }

    transaccion.commit()?;
    Ok(resumen)
}

// ----------------------------------------------------------------- lápida

/// Recupera la versión local perdedora de un conflicto como edición nueva
/// (acción r de la vista CONFLICTOS).
///
/// La versión perdedora es el JSON del contenido del elemento (los DatosX del
/// formato), tal como lo guarda la fusión. Se escribe en su tabla con versión y
/// vector nuevos de este equipo; si la fila ya no existe (perdió ante una
/// lápida) se reinserta. Las huellas no se recuperan así: las resuelve el
/// usuario contra known_hosts.
pub fn recuperar_version(conexion: &Connection, conflicto: &Conflicto, hogar: &Path) -> Result<()> {
    if conflicto.tipo == conflictos::TIPO_HUELLA {
        bail!("las huellas no se recuperan: elige la del otro equipo o mantén la tuya");
    }
    let Some(perdedora) = conflicto.version_perdedora_json.as_deref() else {
        bail!(
            "el conflicto no guarda una versión perdedora que recuperar («{}» en «{}»)",
            conflicto.tipo,
            conflicto.nombre
        );
    };
    let tabla = Tabla::desde_tipo_elemento(&conflicto.elemento_tipo).with_context(|| {
        format!(
            "el conflicto apunta a un elemento que no se puede recuperar: «{}»",
            conflicto.elemento_tipo
        )
    })?;
    let contenido: serde_json::Value =
        serde_json::from_str(perdedora).context("la versión perdedora del conflicto no es JSON")?;

    let transaccion = conexion.unchecked_transaction()?;
    let vector_anterior =
        vector_de_uuid(&transaccion, tabla, &conflicto.uuid)?.unwrap_or_else(|| "{}".to_string());
    let (version, vector) = versiones::version_de_edicion(&transaccion, &vector_anterior)?;
    let vector = versiones::vector_desde_json(&vector);
    let uuid = conflicto.uuid.clone();

    match tabla {
        Tabla::Grupos => {
            let datos: DatosGrupo = serde_json::from_value(contenido)
                .context("la versión perdedora no son datos de grupo")?;
            escribir_grupo(&transaccion, &uuid, &version, &vector, &datos)?;
        }
        Tabla::Hosts => {
            let datos: DatosHost = serde_json::from_value(contenido)
                .context("la versión perdedora no son datos de host")?;
            let id = escribir_host(&transaccion, &uuid, &version, &vector, &datos, hogar)?;
            fijar_referencias_de_un_host(&transaccion, id, &datos)?;
            reemplazar_etiquetas_de_host(&transaccion, id, &datos.etiquetas)?;
            actualizar_verificaciones(&transaccion, id, datos.verificaciones.as_ref())?;
        }
        Tabla::Identidades => {
            let datos: DatosIdentidad = serde_json::from_value(contenido)
                .context("la versión perdedora no son datos de identidad")?;
            escribir_identidad(&transaccion, &uuid, &version, &vector, &datos)?;
        }
        Tabla::Tuneles => {
            let datos: DatosTunel = serde_json::from_value(contenido)
                .context("la versión perdedora no son datos de túnel")?;
            let host_uuid = texto_no_vacio(&datos.host)
                .context("la versión perdedora del túnel no dice a qué host pertenece")?;
            let Some(host_id) = id_por_uuid(&transaccion, Tabla::Hosts, host_uuid)? else {
                bail!(
                    "no se puede recuperar el túnel «{}»: su host ya no existe",
                    conflicto.nombre
                );
            };
            escribir_tunel(&transaccion, &uuid, &version, &vector, &datos, host_id)?;
        }
        Tabla::Snippets => {
            let datos: DatosSnippet = serde_json::from_value(contenido)
                .context("la versión perdedora no son datos de snippet")?;
            let id = escribir_snippet(&transaccion, &uuid, &version, &vector, &datos)?;
            let hosts = leer_ids(&transaccion, Tabla::Hosts, None)?;
            reemplazar_destinos_de_snippet(&transaccion, id, &datos.destinos, &hosts)?;
        }
        Tabla::SincronizacionesDir => {
            let datos: DatosSincronizacion = serde_json::from_value(contenido)
                .context("la versión perdedora no son datos de sincronización")?;
            let host_uuid = texto_no_vacio(&datos.host).context(
                "la versión perdedora de la sincronización no dice a qué host pertenece",
            )?;
            let Some(host_id) = id_por_uuid(&transaccion, Tabla::Hosts, host_uuid)? else {
                bail!(
                    "no se puede recuperar la sincronización «{}»: su host ya no existe",
                    conflicto.nombre
                );
            };
            escribir_sincronizacion(
                &transaccion,
                &uuid,
                &version,
                &vector,
                &datos,
                host_id,
                hogar,
            )?;
        }
    }

    conflictos::resolver(
        &transaccion,
        conflicto.id,
        conflictos::RESOLUCION_RECUPERADO,
    )?;
    transaccion.commit()?;
    Ok(())
}

// ------------------------------------------------------------- unión por nombre

/// Unión por nombre al añadir un equipo (§7.4): cada elemento local con la
/// misma clave natural que uno del fichero adopta el uuid del fichero. Las
/// identidades no lo necesitan: su uuid sale de la huella, pero se aplica igual
/// por si un inventario antiguo llegó con otro.
///
/// No toca versión ni vector: la fusión posterior mezcla los vectores de los dos
/// y decide qué contenido gana.
pub fn unir_por_nombre(
    conexion: &Connection,
    documentos: &[crate::sincronizacion::formato::Documento],
    _hogar: &Path,
) -> Result<usize> {
    let transaccion = conexion.unchecked_transaction()?;
    let mut unidos = 0;
    // Se hacen dos pasadas: primero los elementos cuya clave natural no
    // depende de otra, para que cuando lleguen los túneles y las
    // sincronizaciones los hosts del fichero ya tengan aquí su uuid.
    for segunda_pasada in [false, true] {
        for documento in documentos {
            for elemento in &documento.elementos {
                let Some(tabla) = Tabla::desde_tipo_elemento(&elemento.tipo) else {
                    continue;
                };
                let depende_del_host = matches!(tabla, Tabla::Tuneles | Tabla::SincronizacionesDir);
                if depende_del_host != segunda_pasada {
                    continue;
                }
                unidos += unir_elemento(&transaccion, tabla, elemento)?;
            }
        }
    }
    transaccion.commit()?;
    Ok(unidos)
}

/// Adopta el uuid del fichero para el elemento local con la misma clave
/// natural, si lo hay. Devuelve cuántos uuid ha adoptado (0 o 1).
fn unir_elemento(
    conexion: &Connection,
    tabla: Tabla,
    elemento: &crate::sincronizacion::formato::ElementoDocumento,
) -> Result<usize> {
    let Some(id) = id_local_por_clave_natural(conexion, tabla, &elemento.datos)? else {
        return Ok(0);
    };
    let uuid_local: Option<String> = conexion.query_row(
        &format!("SELECT uuid FROM {} WHERE id = ?1", tabla.nombre()),
        params![id],
        |fila| fila.get(0),
    )?;
    if uuid_local.as_deref() == Some(elemento.uuid.as_str()) || elemento.uuid.trim().is_empty() {
        return Ok(0);
    }
    // Si el uuid del fichero ya lo tiene otra fila local, no se adopta: sería
    // violar el UNIQUE y no hay una respuesta mejor que dejarlo.
    if id_por_uuid(conexion, tabla, &elemento.uuid)?.is_some() {
        tracing::warn!(
            tabla = tabla.nombre(),
            uuid = %elemento.uuid,
            "el uuid del fichero ya está ocupado en este equipo; no se adopta"
        );
        return Ok(0);
    }
    // Un uuid con lápida no se adopta: el elemento local resucitaría el
    // borrado sin querer.
    if lapidas::obtener(conexion, &elemento.uuid)?.is_some() {
        tracing::warn!(
            tabla = tabla.nombre(),
            uuid = %elemento.uuid,
            "el uuid del fichero tiene lápida en este equipo; no se adopta"
        );
        return Ok(0);
    }
    conexion
        .execute(
            &format!("UPDATE {} SET uuid = ?1 WHERE id = ?2", tabla.nombre()),
            params![elemento.uuid, id],
        )
        .with_context(|| format!("adoptando el uuid {}", elemento.uuid))?;
    Ok(1)
}

/// Fila local con la misma clave natural que los datos del fichero, si la hay:
/// nombre (identidades por huella; túneles y sincronizaciones por nombre y
/// host).
fn id_local_por_clave_natural(
    conexion: &Connection,
    tabla: Tabla,
    datos: &serde_json::Value,
) -> Result<Option<i64>> {
    let consulta: Option<i64> = match tabla {
        Tabla::Grupos => {
            let datos: DatosGrupo = match serde_json::from_value(datos.clone()) {
                Ok(datos) => datos,
                Err(error) => {
                    tracing::warn!("grupo ilegible en el fichero para unir: {error}");
                    return Ok(None);
                }
            };
            let Some(nombre) = texto_no_vacio(&datos.nombre) else {
                return Ok(None);
            };
            conexion
                .query_row(
                    "SELECT id FROM GRUPOS WHERE nombre = ?1",
                    params![nombre],
                    |fila| fila.get(0),
                )
                .optional()?
        }
        Tabla::Hosts => {
            let datos: DatosHost = match serde_json::from_value(datos.clone()) {
                Ok(datos) => datos,
                Err(error) => {
                    tracing::warn!("host ilegible en el fichero para unir: {error}");
                    return Ok(None);
                }
            };
            let Some(nombre) = texto_no_vacio(&datos.nombre) else {
                return Ok(None);
            };
            conexion
                .query_row(
                    "SELECT id FROM HOSTS WHERE nombre = ?1",
                    params![nombre],
                    |fila| fila.get(0),
                )
                .optional()?
        }
        Tabla::Identidades => {
            let datos: DatosIdentidad = match serde_json::from_value(datos.clone()) {
                Ok(datos) => datos,
                Err(error) => {
                    tracing::warn!("identidad ilegible en el fichero para unir: {error}");
                    return Ok(None);
                }
            };
            let Some(huella) = texto_no_vacio(&datos.huella) else {
                return Ok(None);
            };
            conexion
                .query_row(
                    "SELECT id FROM IDENTIDADES WHERE huella = ?1",
                    params![huella],
                    |fila| fila.get(0),
                )
                .optional()?
        }
        Tabla::Snippets => {
            let datos: DatosSnippet = match serde_json::from_value(datos.clone()) {
                Ok(datos) => datos,
                Err(error) => {
                    tracing::warn!("snippet ilegible en el fichero para unir: {error}");
                    return Ok(None);
                }
            };
            let Some(nombre) = texto_no_vacio(&datos.nombre) else {
                return Ok(None);
            };
            conexion
                .query_row(
                    "SELECT id FROM SNIPPETS WHERE nombre = ?1",
                    params![nombre],
                    |fila| fila.get(0),
                )
                .optional()?
        }
        Tabla::Tuneles | Tabla::SincronizacionesDir => {
            let (host_uuid, nombre) = if tabla == Tabla::Tuneles {
                let datos: DatosTunel = match serde_json::from_value(datos.clone()) {
                    Ok(datos) => datos,
                    Err(error) => {
                        tracing::warn!("túnel ilegible en el fichero para unir: {error}");
                        return Ok(None);
                    }
                };
                (datos.host, datos.nombre)
            } else {
                let datos: DatosSincronizacion = match serde_json::from_value(datos.clone()) {
                    Ok(datos) => datos,
                    Err(error) => {
                        tracing::warn!("sincronización ilegible en el fichero para unir: {error}");
                        return Ok(None);
                    }
                };
                (datos.host, datos.nombre)
            };
            let (Some(host_uuid), Some(nombre)) =
                (texto_no_vacio(&host_uuid), texto_no_vacio(&nombre))
            else {
                return Ok(None);
            };
            let Some(host_id) = id_por_uuid(conexion, Tabla::Hosts, host_uuid)? else {
                return Ok(None);
            };
            conexion
                .query_row(
                    &format!(
                        "SELECT id FROM {} WHERE host_id = ?1 AND nombre = ?2",
                        tabla.nombre()
                    ),
                    params![host_id, nombre],
                    |fila| fila.get(0),
                )
                .optional()?
        }
    };
    Ok(consulta)
}

// ------------------------------------------------------------- lectura del plan

/// Valida y tipa el plan. Un tipo desconocido se ignora con aviso (el formato
/// permite tipos nuevos, §14); unos datos que no encajan son un plan corrupto y
/// abortan la transacción entera.
fn leer_plan(plan: &PlanFusion) -> Result<PlanLeido> {
    let mut vivos = VivosPlan::default();
    for candidato in &plan.vivos {
        let Some(tabla) = Tabla::desde_tipo_elemento(&candidato.tipo) else {
            tracing::warn!(
                tipo = %candidato.tipo,
                "elemento de un tipo que este MAGI no conoce; se ignora"
            );
            continue;
        };
        let Some(uuid) = texto_no_vacio(&candidato.uuid) else {
            tracing::warn!("elemento sin uuid en el plan; se ignora");
            continue;
        };
        let Some(contenido) = candidato.contenido.as_ref() else {
            tracing::warn!(uuid, "elemento vivo sin contenido en el plan; se ignora");
            continue;
        };
        if candidato.version.trim().is_empty() {
            bail!("el plan trae un elemento sin versión (uuid {uuid})");
        }
        let version = candidato.version.clone();
        let vector = candidato.vector.clone();
        match tabla {
            Tabla::Grupos => {
                let datos = leer_datos::<DatosGrupo>(contenido, uuid, "grupo")?;
                vivos.grupos.insert(
                    uuid.to_string(),
                    VivoPlan {
                        version,
                        vector,
                        datos,
                    },
                );
            }
            Tabla::Hosts => {
                let datos = leer_datos::<DatosHost>(contenido, uuid, "host")?;
                vivos.hosts.insert(
                    uuid.to_string(),
                    VivoPlan {
                        version,
                        vector,
                        datos,
                    },
                );
            }
            Tabla::Identidades => {
                let datos = leer_datos::<DatosIdentidad>(contenido, uuid, "identidad")?;
                vivos.identidades.insert(
                    uuid.to_string(),
                    VivoPlan {
                        version,
                        vector,
                        datos,
                    },
                );
            }
            Tabla::Tuneles => {
                let datos = leer_datos::<DatosTunel>(contenido, uuid, "túnel")?;
                vivos.tuneles.insert(
                    uuid.to_string(),
                    VivoPlan {
                        version,
                        vector,
                        datos,
                    },
                );
            }
            Tabla::Snippets => {
                let datos = leer_datos::<DatosSnippet>(contenido, uuid, "snippet")?;
                vivos.snippets.insert(
                    uuid.to_string(),
                    VivoPlan {
                        version,
                        vector,
                        datos,
                    },
                );
            }
            Tabla::SincronizacionesDir => {
                let datos = leer_datos::<DatosSincronizacion>(contenido, uuid, "sincronización")?;
                vivos.sincronizaciones.insert(
                    uuid.to_string(),
                    VivoPlan {
                        version,
                        vector,
                        datos,
                    },
                );
            }
        }
    }

    let mut lapidas = BTreeMap::new();
    for candidato in &plan.lapidas {
        if !lapidas::tipo_valido(&candidato.tipo) {
            tracing::warn!(
                tipo = %candidato.tipo,
                "lápida de un tipo que este MAGI no conoce; se ignora"
            );
            continue;
        }
        let Some(uuid) = texto_no_vacio(&candidato.uuid) else {
            tracing::warn!("lápida sin uuid en el plan; se ignora");
            continue;
        };
        let lapida = Lapida {
            uuid: uuid.to_string(),
            tipo: candidato.tipo.clone(),
            nombre: candidato.nombre.clone(),
            version: candidato.version.clone(),
            vector: versiones::vector_a_json(&candidato.vector),
            borrado_en: candidato.borrado_en.clone().unwrap_or_else(fecha_ahora),
        };
        lapidas.insert(uuid.to_string(), lapida);
    }

    let mut renombrados: HashMap<Tabla, BTreeSet<String>> = HashMap::new();
    for (tipo, uuid) in &plan.renombrados {
        let Some(tabla) = Tabla::desde_tipo_elemento(tipo) else {
            tracing::warn!(tipo = %tipo, "renombrado de un tipo desconocido; se ignora");
            continue;
        };
        if let Some(uuid) = texto_no_vacio(uuid) {
            renombrados
                .entry(tabla)
                .or_default()
                .insert(uuid.to_string());
        }
    }

    Ok(PlanLeido {
        vivos,
        lapidas,
        renombrados,
    })
}

fn leer_datos<T>(contenido: &serde_json::Value, uuid: &str, que: &str) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(contenido.clone())
        .with_context(|| format!("los datos del {que} {uuid} del plan no son válidos"))
}

// ------------------------------------------------------------- estado local

/// Filas locales con uuid de una tabla (las que participan en la sincronización).
fn leer_locales(conexion: &Connection, tabla: Tabla) -> Result<BTreeMap<String, FilaLocal>> {
    let con_host = matches!(tabla, Tabla::Tuneles | Tabla::SincronizacionesDir);
    let consulta = format!(
        "SELECT id, uuid, version, vector, {}{} FROM {} ORDER BY id",
        if con_host { "host_id, " } else { "" },
        columna_nombre(tabla),
        tabla.nombre()
    );
    let mut sentencia = conexion.prepare(&consulta)?;
    let filas = sentencia.query_map([], |fila| {
        let uuid: Option<String> = fila.get(1)?;
        let (host_id, indice_nombre) = if con_host {
            (Some(fila.get(4)?), 5)
        } else {
            (None, 4)
        };
        Ok((
            uuid,
            FilaLocal {
                id: fila.get(0)?,
                nombre: fila.get(indice_nombre)?,
                version: fila.get(2)?,
                vector: fila.get(3)?,
                host_id,
            },
        ))
    })?;
    let mut mapa = BTreeMap::new();
    for fila in filas {
        let (uuid, fila) = fila?;
        // Una fila sin uuid no participa en la sincronización: no se toca.
        let Some(uuid) = uuid.filter(|uuid| !uuid.trim().is_empty()) else {
            continue;
        };
        mapa.insert(uuid, fila);
    }
    Ok(mapa)
}

/// id local de cada uuid de la tabla; si se pasa una lista, se limita a ella.
fn leer_ids(
    conexion: &Connection,
    tabla: Tabla,
    solo: Option<&BTreeSet<String>>,
) -> Result<HashMap<String, i64>> {
    let mut sentencia = conexion.prepare(&format!(
        "SELECT id, uuid FROM {} WHERE uuid IS NOT NULL",
        tabla.nombre()
    ))?;
    let filas = sentencia.query_map([], |fila| {
        Ok((fila.get::<_, i64>(0)?, fila.get::<_, String>(1)?))
    })?;
    let mut mapa = HashMap::new();
    for fila in filas {
        let (id, uuid) = fila?;
        if uuid.trim().is_empty() {
            continue;
        }
        if let Some(solo) = solo {
            if !solo.contains(&uuid) {
                continue;
            }
        }
        mapa.insert(uuid, id);
    }
    Ok(mapa)
}

fn id_por_uuid(conexion: &Connection, tabla: Tabla, uuid: &str) -> Result<Option<i64>> {
    Ok(conexion
        .query_row(
            &format!("SELECT id FROM {} WHERE uuid = ?1", tabla.nombre()),
            params![uuid],
            |fila| fila.get(0),
        )
        .optional()?)
}

fn vector_de_uuid(conexion: &Connection, tabla: Tabla, uuid: &str) -> Result<Option<String>> {
    Ok(conexion
        .query_row(
            &format!("SELECT vector FROM {} WHERE uuid = ?1", tabla.nombre()),
            params![uuid],
            |fila| fila.get(0),
        )
        .optional()?)
}

/// ¿Hay que reescribir la fila? La versión identifica la edición: si coincide
/// con la del plan, el contenido es el mismo (solo el vector puede haberse
/// extendido al fusionar). Los renombrados siempre se escriben para aplicar el
/// nombre nuevo antes de subirles la versión.
fn necesita_actualizar(
    fila: &FilaLocal,
    version: &str,
    vector: &BTreeMap<String, i64>,
    forzado: bool,
) -> bool {
    if forzado {
        return true;
    }
    if fila.version.as_deref() != Some(version) {
        return true;
    }
    versiones::vector_desde_json(fila.vector.as_deref().unwrap_or("{}")) != *vector
}

/// ¿La fila tiene lo mínimo para participar en la fusión? Las filas sin
/// versión o sin vector (que no deberían existir) las omite la exportación.
fn fila_participa_en_la_fusion(fila: &FilaLocal) -> bool {
    let version = fila.version.as_deref().unwrap_or_default();
    if version.trim().is_empty() {
        return false;
    }
    let vector = versiones::vector_desde_json(fila.vector.as_deref().unwrap_or("{}"));
    !vector.is_empty()
}

/// ¿La exportación puede leer los datos viajeros de esta fila? Las filas
/// ilegibles no entran en la fusión y por tanto no se borran al aplicar: sería
/// perder datos que el usuario no ha visto.
fn fila_exportable(conexion: &Connection, tabla: Tabla, id: i64) -> bool {
    match crate::sincronizacion::elementos::datos_de_fila(conexion, tabla, id) {
        Ok(Some(_)) => true,
        Ok(None) => false,
        Err(error) => {
            tracing::warn!(
                tabla = tabla.nombre(),
                id,
                "fila ilegible, no se borra: {error}"
            );
            false
        }
    }
}

/// Columna del nombre natural de cada tabla (la que tiene UNIQUE).
fn columna_nombre(tabla: Tabla) -> &'static str {
    match tabla {
        Tabla::Identidades => "alias",
        Tabla::Grupos
        | Tabla::Hosts
        | Tabla::Tuneles
        | Tabla::Snippets
        | Tabla::SincronizacionesDir => "nombre",
    }
}

/// Aparta las filas que se van a reescribir o borrar a un nombre temporal único
/// para que las permutas de nombre y los UNIQUE no choquen.
fn apartar_nombres(
    conexion: &Connection,
    tabla: Tabla,
    locales: Option<&BTreeMap<String, FilaLocal>>,
    a_actualizar: Option<&BTreeSet<String>>,
    a_borrar: Option<&BTreeSet<String>>,
) -> Result<()> {
    let Some(locales) = locales else {
        return Ok(());
    };
    let vacio = BTreeSet::new();
    let a_actualizar = a_actualizar.unwrap_or(&vacio);
    let a_borrar = a_borrar.unwrap_or(&vacio);
    if a_actualizar.is_empty() && a_borrar.is_empty() {
        return Ok(());
    }
    let mut sentencia = conexion.prepare(&format!(
        "UPDATE {} SET {} = ?1 WHERE id = ?2",
        tabla.nombre(),
        columna_nombre(tabla)
    ))?;
    for uuid in a_actualizar.iter().chain(a_borrar.iter()) {
        let Some(fila) = locales.get(uuid) else {
            continue;
        };
        sentencia.execute(params![format!("{PREFIJO_TEMPORAL}{uuid}"), fila.id])?;
    }
    Ok(())
}

/// Lápida para una fila local que el plan no trae: se queda con el nombre, la
/// versión y el vector que tenía antes de apartarla para aplicar el plan.
fn lapida_minima(
    conexion: &Connection,
    tabla: Tabla,
    uuid: &str,
    fila: &FilaLocal,
) -> Result<Lapida> {
    let (version, vector) = match (fila.version.clone(), fila.vector.clone()) {
        (Some(version), Some(vector)) if !version.trim().is_empty() => (version, vector),
        (_, vector) => {
            versiones::version_de_lapida(conexion, &vector.unwrap_or_else(|| "{}".to_string()))?
        }
    };
    Ok(Lapida {
        uuid: uuid.to_string(),
        tipo: tabla.tipo_elemento().to_string(),
        nombre: fila.nombre.clone(),
        version,
        vector,
        borrado_en: fecha_ahora(),
    })
}

// ------------------------------------------------------------------- escritura

fn escribir_grupos(
    conexion: &Connection,
    vivos: &VivosPlan,
    locales: &HashMap<Tabla, BTreeMap<String, FilaLocal>>,
    a_actualizar: &HashMap<Tabla, BTreeSet<String>>,
    resumen: &mut ResumenAplicacion,
) -> Result<()> {
    let locales_tabla = locales.get(&Tabla::Grupos);
    let actualizar = a_actualizar.get(&Tabla::Grupos);
    for (uuid, vivo) in &vivos.grupos {
        match locales_tabla.and_then(|mapa| mapa.get(uuid)) {
            None => {
                escribir_grupo(conexion, uuid, &vivo.version, &vivo.vector, &vivo.datos)?;
                resumen.creados += 1;
            }
            Some(_) => {
                if actualizar.is_some_and(|uuids| uuids.contains(uuid)) {
                    escribir_grupo(conexion, uuid, &vivo.version, &vivo.vector, &vivo.datos)?;
                    resumen.actualizados += 1;
                }
            }
        }
    }
    Ok(())
}

fn escribir_snippets(
    conexion: &Connection,
    vivos: &VivosPlan,
    locales: &HashMap<Tabla, BTreeMap<String, FilaLocal>>,
    a_actualizar: &HashMap<Tabla, BTreeSet<String>>,
    resumen: &mut ResumenAplicacion,
) -> Result<()> {
    let locales_tabla = locales.get(&Tabla::Snippets);
    let actualizar = a_actualizar.get(&Tabla::Snippets);
    for (uuid, vivo) in &vivos.snippets {
        match locales_tabla.and_then(|mapa| mapa.get(uuid)) {
            None => {
                escribir_snippet(conexion, uuid, &vivo.version, &vivo.vector, &vivo.datos)?;
                resumen.creados += 1;
            }
            Some(_) => {
                if actualizar.is_some_and(|uuids| uuids.contains(uuid)) {
                    escribir_snippet(conexion, uuid, &vivo.version, &vivo.vector, &vivo.datos)?;
                    resumen.actualizados += 1;
                }
            }
        }
    }
    Ok(())
}

fn escribir_identidades(
    conexion: &Connection,
    vivos: &VivosPlan,
    locales: &HashMap<Tabla, BTreeMap<String, FilaLocal>>,
    a_actualizar: &HashMap<Tabla, BTreeSet<String>>,
    resumen: &mut ResumenAplicacion,
) -> Result<()> {
    let locales_tabla = locales.get(&Tabla::Identidades);
    let actualizar = a_actualizar.get(&Tabla::Identidades);
    for (uuid, vivo) in &vivos.identidades {
        match locales_tabla.and_then(|mapa| mapa.get(uuid)) {
            None => {
                escribir_identidad(conexion, uuid, &vivo.version, &vivo.vector, &vivo.datos)?;
                resumen.creados += 1;
            }
            Some(_) => {
                if actualizar.is_some_and(|uuids| uuids.contains(uuid)) {
                    escribir_identidad(conexion, uuid, &vivo.version, &vivo.vector, &vivo.datos)?;
                    resumen.actualizados += 1;
                }
            }
        }
    }
    Ok(())
}

fn escribir_hosts(
    conexion: &Connection,
    vivos: &VivosPlan,
    locales: &HashMap<Tabla, BTreeMap<String, FilaLocal>>,
    a_actualizar: &HashMap<Tabla, BTreeSet<String>>,
    hogar: &Path,
    resumen: &mut ResumenAplicacion,
) -> Result<()> {
    let locales_tabla = locales.get(&Tabla::Hosts);
    let actualizar = a_actualizar.get(&Tabla::Hosts);
    for (uuid, vivo) in &vivos.hosts {
        match locales_tabla.and_then(|mapa| mapa.get(uuid)) {
            None => {
                escribir_host(
                    conexion,
                    uuid,
                    &vivo.version,
                    &vivo.vector,
                    &vivo.datos,
                    hogar,
                )?;
                resumen.creados += 1;
            }
            Some(_) => {
                if actualizar.is_some_and(|uuids| uuids.contains(uuid)) {
                    escribir_host(
                        conexion,
                        uuid,
                        &vivo.version,
                        &vivo.vector,
                        &vivo.datos,
                        hogar,
                    )?;
                    resumen.actualizados += 1;
                }
            }
        }
    }
    Ok(())
}

fn escribir_tuneles(
    conexion: &Connection,
    vivos: &VivosPlan,
    locales: &HashMap<Tabla, BTreeMap<String, FilaLocal>>,
    a_actualizar: &HashMap<Tabla, BTreeSet<String>>,
    ids: &HashMap<Tabla, HashMap<String, i64>>,
    resumen: &mut ResumenAplicacion,
) -> Result<()> {
    let ids_hosts = ids.get(&Tabla::Hosts).cloned().unwrap_or_default();
    let locales_tabla = locales.get(&Tabla::Tuneles);
    let actualizar = a_actualizar.get(&Tabla::Tuneles);
    for (uuid, vivo) in &vivos.tuneles {
        let Some(host_id) = resolver_uuid(&ids_hosts, Some(vivo.datos.host.as_str())) else {
            // Huérfano: la fusión (§7.3) ya lo descarta y deja conflicto.
            tracing::warn!(uuid, "túnel cuyo host no está vivo; no se aplica");
            continue;
        };
        match locales_tabla.and_then(|mapa| mapa.get(uuid)) {
            None => {
                escribir_tunel(
                    conexion,
                    uuid,
                    &vivo.version,
                    &vivo.vector,
                    &vivo.datos,
                    host_id,
                )?;
                resumen.creados += 1;
            }
            Some(fila) => {
                let cambiar = actualizar.is_some_and(|uuids| uuids.contains(uuid))
                    || fila.host_id != Some(host_id);
                if cambiar {
                    escribir_tunel(
                        conexion,
                        uuid,
                        &vivo.version,
                        &vivo.vector,
                        &vivo.datos,
                        host_id,
                    )?;
                    resumen.actualizados += 1;
                }
            }
        }
    }
    Ok(())
}

fn escribir_sincronizaciones(
    conexion: &Connection,
    vivos: &VivosPlan,
    locales: &HashMap<Tabla, BTreeMap<String, FilaLocal>>,
    a_actualizar: &HashMap<Tabla, BTreeSet<String>>,
    ids: &HashMap<Tabla, HashMap<String, i64>>,
    hogar: &Path,
    resumen: &mut ResumenAplicacion,
) -> Result<()> {
    let ids_hosts = ids.get(&Tabla::Hosts).cloned().unwrap_or_default();
    let locales_tabla = locales.get(&Tabla::SincronizacionesDir);
    let actualizar = a_actualizar.get(&Tabla::SincronizacionesDir);
    for (uuid, vivo) in &vivos.sincronizaciones {
        let Some(host_id) = resolver_uuid(&ids_hosts, Some(vivo.datos.host.as_str())) else {
            tracing::warn!(uuid, "sincronización cuyo host no está vivo; no se aplica");
            continue;
        };
        match locales_tabla.and_then(|mapa| mapa.get(uuid)) {
            None => {
                escribir_sincronizacion(
                    conexion,
                    uuid,
                    &vivo.version,
                    &vivo.vector,
                    &vivo.datos,
                    host_id,
                    hogar,
                )?;
                resumen.creados += 1;
            }
            Some(fila) => {
                let cambiar = actualizar.is_some_and(|uuids| uuids.contains(uuid))
                    || fila.host_id != Some(host_id);
                if cambiar {
                    escribir_sincronizacion(
                        conexion,
                        uuid,
                        &vivo.version,
                        &vivo.vector,
                        &vivo.datos,
                        host_id,
                        hogar,
                    )?;
                    resumen.actualizados += 1;
                }
            }
        }
    }
    Ok(())
}

// --------------------------------------------------------- escrituras de fila

fn escribir_grupo(
    conexion: &Connection,
    uuid: &str,
    version: &str,
    vector: &BTreeMap<String, i64>,
    datos: &DatosGrupo,
) -> Result<i64> {
    let nombre = texto_no_vacio(&datos.nombre)
        .with_context(|| format!("el grupo {uuid} del plan no tiene nombre"))?;
    let vector = versiones::vector_a_json(vector);
    match id_por_uuid(conexion, Tabla::Grupos, uuid)? {
        Some(id) => {
            conexion
                .execute(
                    "UPDATE GRUPOS SET nombre = ?1, orden = ?2, version = ?3, vector = ?4
                      WHERE id = ?5",
                    params![nombre, datos.orden, version, vector, id],
                )
                .map_err(|error| conflicto_de_nombre("grupo", nombre, error))?;
            Ok(id)
        }
        None => {
            conexion
                .execute(
                    "INSERT INTO GRUPOS (nombre, orden, plegado, creado_en, uuid, version, vector)
                     VALUES (?1, ?2, 0, ?3, ?4, ?5, ?6)",
                    params![nombre, datos.orden, fecha_ahora(), uuid, version, vector],
                )
                .map_err(|error| conflicto_de_nombre("grupo", nombre, error))?;
            Ok(conexion.last_insert_rowid())
        }
    }
}

fn escribir_identidad(
    conexion: &Connection,
    uuid: &str,
    version: &str,
    vector: &BTreeMap<String, i64>,
    datos: &DatosIdentidad,
) -> Result<i64> {
    let huella = texto_no_vacio(&datos.huella)
        .with_context(|| format!("la identidad {uuid} del plan no tiene huella"))?;
    let alias = texto_no_vacio(&datos.alias)
        .with_context(|| format!("la identidad {uuid} del plan no tiene alias"))?;
    let tipo = texto_no_vacio(&datos.tipo)
        .with_context(|| format!("la identidad {uuid} del plan no tiene tipo"))?;
    let comentario = datos
        .comentario
        .as_deref()
        .map(str::trim)
        .filter(|texto| !texto.is_empty());
    let vector = versiones::vector_a_json(vector);
    match id_por_uuid(conexion, Tabla::Identidades, uuid)? {
        Some(id) => {
            let huella_actual: String = conexion.query_row(
                "SELECT huella FROM IDENTIDADES WHERE id = ?1",
                params![id],
                |fila| fila.get(0),
            )?;
            if huella_actual != huella {
                bail!(
                    "la identidad «{alias}» del plan cambia de huella: {huella_actual} por {huella}"
                );
            }
            // origen, ruta, anadida_en y ultimo_uso_en son propios de este
            // equipo y no viajan: se conservan.
            conexion
                .execute(
                    "UPDATE IDENTIDADES
                        SET alias = ?1, tipo = ?2, comentario = ?3, revocada_en = ?4,
                            version = ?5, vector = ?6
                      WHERE id = ?7",
                    params![
                        alias,
                        tipo,
                        comentario,
                        datos.revocada_en,
                        version,
                        vector,
                        id
                    ],
                )
                .map_err(|error| conflicto_de_nombre("identidad", alias, error))?;
            Ok(id)
        }
        None => {
            // El origen y la ruta son locales: un alta importada se registra
            // como del agente hasta que este equipo la vea en ~/.ssh o en el
            // agente y la vuelva a sincronizar.
            conexion
                .execute(
                    "INSERT INTO IDENTIDADES (
                        alias, tipo, huella, origen, ruta, comentario, anadida_en,
                        ultimo_uso_en, revocada_en, uuid, version, vector
                     ) VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, NULL, ?7, ?8, ?9, ?10)",
                    params![
                        alias,
                        tipo,
                        huella,
                        OrigenIdentidad::Agente.como_texto(),
                        comentario,
                        fecha_ahora(),
                        datos.revocada_en,
                        uuid,
                        version,
                        vector
                    ],
                )
                .map_err(|error| conflicto_de_nombre("identidad", alias, error))?;
            Ok(conexion.last_insert_rowid())
        }
    }
}

fn escribir_host(
    conexion: &Connection,
    uuid: &str,
    version: &str,
    vector: &BTreeMap<String, i64>,
    datos: &DatosHost,
    hogar: &Path,
) -> Result<i64> {
    let nombre = texto_no_vacio(&datos.nombre)
        .with_context(|| format!("el host {uuid} del plan no tiene nombre"))?;
    let direccion = texto_no_vacio(&datos.direccion)
        .with_context(|| format!("el host «{nombre}» del plan no tiene dirección"))?;
    let usuario = datos
        .usuario
        .as_deref()
        .map(str::trim)
        .filter(|texto| !texto.is_empty());
    let identidad_ref = identidad_ref_para_guardar(datos.identidad_ref.as_deref(), hogar);
    let origen = Origen::desde_texto(&datos.origen).como_texto();
    let multiplexar = datos.multiplexar as i64;
    let keepalive = datos.keepalive_seg.map(i64::from);
    let opciones_extra = &datos.opciones_extra;
    let servicios = &datos.servicios;
    let vector = versiones::vector_a_json(vector);
    let ahora = fecha_ahora();
    match id_por_uuid(conexion, Tabla::Hosts, uuid)? {
        Some(id) => {
            // Las referencias (grupo, salto, snippet) se fijan después, cuando
            // ya existen las filas nuevas del plan.
            conexion
                .execute(
                    "UPDATE HOSTS SET
                        nombre = ?1, direccion = ?2, puerto = ?3, usuario = ?4,
                        identidad_ref = ?5, multiplexar = ?6, keepalive_seg = ?7,
                        opciones_extra = ?8, origen = ?9, servicios = ?10,
                        actualizado_en = ?11, version = ?12, vector = ?13
                      WHERE id = ?14",
                    params![
                        nombre,
                        direccion,
                        datos.puerto,
                        usuario,
                        identidad_ref,
                        multiplexar,
                        keepalive,
                        opciones_extra,
                        origen,
                        servicios,
                        ahora,
                        version,
                        vector,
                        id
                    ],
                )
                .map_err(|error| conflicto_de_nombre("host", nombre, error))?;
            Ok(id)
        }
        None => {
            conexion
                .execute(
                    "INSERT INTO HOSTS (
                        nombre, direccion, puerto, usuario, identidad_ref, multiplexar,
                        keepalive_seg, opciones_extra, origen, servicios, creado_en,
                        actualizado_en, uuid, version, vector
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11, ?12, ?13, ?14)",
                    params![
                        nombre,
                        direccion,
                        datos.puerto,
                        usuario,
                        identidad_ref,
                        multiplexar,
                        keepalive,
                        opciones_extra,
                        origen,
                        servicios,
                        ahora,
                        uuid,
                        version,
                        vector
                    ],
                )
                .map_err(|error| conflicto_de_nombre("host", nombre, error))?;
            Ok(conexion.last_insert_rowid())
        }
    }
}

fn escribir_tunel(
    conexion: &Connection,
    uuid: &str,
    version: &str,
    vector: &BTreeMap<String, i64>,
    datos: &DatosTunel,
    host_id: i64,
) -> Result<i64> {
    let nombre = texto_no_vacio(&datos.nombre)
        .with_context(|| format!("el túnel {uuid} del plan no tiene nombre"))?;
    let tipo = TipoTunel::desde_texto(&datos.tipo)
        .with_context(|| format!("el túnel «{nombre}» del plan tiene un tipo desconocido"))?;
    let escucha = crate::almacen::tuneles::canonica(&datos.escucha)
        .with_context(|| format!("el túnel «{nombre}» del plan tiene una escucha no válida"))?;
    let destino =
        if tipo.lleva_destino() {
            let destino = datos
                .destino
                .as_deref()
                .with_context(|| format!("el túnel «{nombre}» del plan no tiene destino"))?;
            Some(crate::almacen::tuneles::canonica(destino).with_context(|| {
                format!("el túnel «{nombre}» del plan tiene un destino no válido")
            })?)
        } else {
            None
        };
    let automatico = datos.automatico as i64;
    let vector = versiones::vector_a_json(vector);
    let ahora = fecha_ahora();
    match id_por_uuid(conexion, Tabla::Tuneles, uuid)? {
        Some(id) => {
            conexion
                .execute(
                    "UPDATE TUNELES SET host_id = ?1, nombre = ?2, tipo = ?3, escucha = ?4,
                        destino = ?5, automatico = ?6, actualizado_en = ?7,
                        version = ?8, vector = ?9
                      WHERE id = ?10",
                    params![
                        host_id,
                        nombre,
                        tipo.como_texto(),
                        escucha,
                        destino,
                        automatico,
                        ahora,
                        version,
                        vector,
                        id
                    ],
                )
                .map_err(|error| conflicto_de_nombre("túnel", nombre, error))?;
            Ok(id)
        }
        None => {
            conexion
                .execute(
                    "INSERT INTO TUNELES (
                        host_id, nombre, tipo, escucha, destino, automatico, creado_en,
                        actualizado_en, uuid, version, vector
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8, ?9, ?10)",
                    params![
                        host_id,
                        nombre,
                        tipo.como_texto(),
                        escucha,
                        destino,
                        automatico,
                        ahora,
                        uuid,
                        version,
                        vector
                    ],
                )
                .map_err(|error| conflicto_de_nombre("túnel", nombre, error))?;
            Ok(conexion.last_insert_rowid())
        }
    }
}

fn escribir_snippet(
    conexion: &Connection,
    uuid: &str,
    version: &str,
    vector: &BTreeMap<String, i64>,
    datos: &DatosSnippet,
) -> Result<i64> {
    let nombre = texto_no_vacio(&datos.nombre)
        .with_context(|| format!("el snippet {uuid} del plan no tiene nombre"))?;
    if datos.comando.trim().is_empty() {
        bail!("el snippet «{nombre}» del plan no tiene comando");
    }
    let timeout = u32::try_from(datos.timeout_seg)
        .with_context(|| format!("el snippet «{nombre}» del plan tiene un timeout enorme"))?;
    if !(crate::snippets::TIMEOUT_MIN..=crate::snippets::TIMEOUT_MAX).contains(&timeout) {
        bail!(
            "el snippet «{nombre}» del plan tiene el timeout {timeout} fuera de rango ({}-{})",
            crate::snippets::TIMEOUT_MIN,
            crate::snippets::TIMEOUT_MAX
        );
    }
    let etiquetas = etiquetas_de_snippet(&datos.etiquetas);
    let critico = datos.critico as i64;
    let parar_al_fallo = datos.parar_al_fallo as i64;
    let vector = versiones::vector_a_json(vector);
    let ahora = fecha_ahora();
    match id_por_uuid(conexion, Tabla::Snippets, uuid)? {
        Some(id) => {
            conexion
                .execute(
                    "UPDATE SNIPPETS SET nombre = ?1, comando = ?2, descripcion = ?3,
                        etiquetas = ?4, critico = ?5, timeout_seg = ?6,
                        parar_al_fallo = ?7, actualizado_en = ?8, version = ?9, vector = ?10
                      WHERE id = ?11",
                    params![
                        nombre,
                        datos.comando,
                        datos.descripcion,
                        etiquetas,
                        critico,
                        timeout,
                        parar_al_fallo,
                        ahora,
                        version,
                        vector,
                        id
                    ],
                )
                .map_err(|error| conflicto_de_nombre("snippet", nombre, error))?;
            Ok(id)
        }
        None => {
            conexion
                .execute(
                    "INSERT INTO SNIPPETS (
                        nombre, comando, descripcion, etiquetas, critico, timeout_seg,
                        parar_al_fallo, usado_veces, ultimo_uso_en, creado_en,
                        actualizado_en, uuid, version, vector
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, NULL, ?8, ?8, ?9, ?10, ?11)",
                    params![
                        nombre,
                        datos.comando,
                        datos.descripcion,
                        etiquetas,
                        critico,
                        timeout,
                        parar_al_fallo,
                        ahora,
                        uuid,
                        version,
                        vector
                    ],
                )
                .map_err(|error| conflicto_de_nombre("snippet", nombre, error))?;
            Ok(conexion.last_insert_rowid())
        }
    }
}

fn escribir_sincronizacion(
    conexion: &Connection,
    uuid: &str,
    version: &str,
    vector: &BTreeMap<String, i64>,
    datos: &DatosSincronizacion,
    host_id: i64,
    hogar: &Path,
) -> Result<i64> {
    let nombre = texto_no_vacio(&datos.nombre)
        .with_context(|| format!("la sincronización {uuid} del plan no tiene nombre"))?;
    let ruta_local = texto_no_vacio(&datos.ruta_local)
        .with_context(|| format!("la sincronización «{nombre}» del plan no tiene ruta local"))?;
    let ruta_local = ruta_desde_home(ruta_local, hogar);
    let ruta_remota = texto_no_vacio(&datos.ruta_remota)
        .with_context(|| format!("la sincronización «{nombre}» del plan no tiene ruta remota"))?;
    let direccion = Direccion::desde_texto(&datos.direccion).with_context(|| {
        format!("la sincronización «{nombre}» del plan tiene una dirección desconocida")
    })?;
    let exclusiones =
        crate::almacen::sincronizaciones::partir_exclusiones(&datos.exclusiones).join("\n");
    let borrar = datos.borrar as i64;
    let vector = versiones::vector_a_json(vector);
    let ahora = fecha_ahora();
    match id_por_uuid(conexion, Tabla::SincronizacionesDir, uuid)? {
        Some(id) => {
            conexion
                .execute(
                    "UPDATE SINCRONIZACIONES_DIR SET host_id = ?1, nombre = ?2,
                        ruta_local = ?3, ruta_remota = ?4, direccion = ?5, borrar = ?6,
                        exclusiones = ?7, actualizado_en = ?8, version = ?9, vector = ?10
                      WHERE id = ?11",
                    params![
                        host_id,
                        nombre,
                        ruta_local,
                        ruta_remota,
                        direccion.texto(),
                        borrar,
                        exclusiones,
                        ahora,
                        version,
                        vector,
                        id
                    ],
                )
                .map_err(|error| conflicto_de_nombre("sincronización", nombre, error))?;
            Ok(id)
        }
        None => {
            conexion
                .execute(
                    "INSERT INTO SINCRONIZACIONES_DIR (
                        host_id, nombre, ruta_local, ruta_remota, direccion, borrar,
                        exclusiones, ultima_ejecucion_en, ultimo_resultado, creado_en,
                        actualizado_en, uuid, version, vector
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, NULL, ?8, ?8, ?9, ?10, ?11)",
                    params![
                        host_id,
                        nombre,
                        ruta_local,
                        ruta_remota,
                        direccion.texto(),
                        borrar,
                        exclusiones,
                        ahora,
                        uuid,
                        version,
                        vector
                    ],
                )
                .map_err(|error| conflicto_de_nombre("sincronización", nombre, error))?;
            Ok(conexion.last_insert_rowid())
        }
    }
}

// ------------------------------------------------------ referencias y etiquetas

/// Fija las referencias por uuid de todos los hosts vivos: grupo, salto y
/// snippet al conectar. Lo que no está vivo (no existe o tiene lápida) queda a
/// NULL.
fn fijar_referencias_de_hosts(
    conexion: &Connection,
    vivos: &VivosPlan,
    ids: &HashMap<Tabla, HashMap<String, i64>>,
) -> Result<()> {
    let grupos = ids.get(&Tabla::Grupos).cloned().unwrap_or_default();
    let hosts = ids.get(&Tabla::Hosts).cloned().unwrap_or_default();
    let snippets = ids.get(&Tabla::Snippets).cloned().unwrap_or_default();
    for (uuid, vivo) in &vivos.hosts {
        let Some(id) = hosts.get(uuid) else {
            continue;
        };
        let grupo = resolver_uuid(&grupos, vivo.datos.grupo.as_deref());
        let salto = resolver_uuid(&hosts, vivo.datos.salto.as_deref());
        let snippet = resolver_uuid(&snippets, vivo.datos.snippet_al_conectar.as_deref());
        conexion.execute(
            "UPDATE HOSTS SET grupo_id = ?1, salto_host_id = ?2, snippet_al_conectar_id = ?3
              WHERE id = ?4",
            params![grupo, salto, snippet, id],
        )?;
    }
    Ok(())
}

/// Igual que la anterior pero para una sola fila recuperada de un conflicto:
/// resuelve contra el estado actual del almacén.
fn fijar_referencias_de_un_host(conexion: &Connection, id: i64, datos: &DatosHost) -> Result<()> {
    let grupo = resolver_uuid_actual(conexion, Tabla::Grupos, datos.grupo.as_deref())?;
    let salto = resolver_uuid_actual(conexion, Tabla::Hosts, datos.salto.as_deref())?;
    let snippet = resolver_uuid_actual(
        conexion,
        Tabla::Snippets,
        datos.snippet_al_conectar.as_deref(),
    )?;
    conexion.execute(
        "UPDATE HOSTS SET grupo_id = ?1, salto_host_id = ?2, snippet_al_conectar_id = ?3
          WHERE id = ?4",
        params![grupo, salto, snippet, id],
    )?;
    Ok(())
}

/// uuid del plan resuelto a id local; None si está vacío o no está vivo.
fn resolver_uuid(mapa: &HashMap<String, i64>, uuid: Option<&str>) -> Option<i64> {
    let uuid = texto_no_vacio(uuid?)?;
    mapa.get(uuid).copied()
}

fn resolver_uuid_actual(
    conexion: &Connection,
    tabla: Tabla,
    uuid: Option<&str>,
) -> Result<Option<i64>> {
    let Some(uuid) = uuid.and_then(texto_no_vacio) else {
        return Ok(None);
    };
    id_por_uuid(conexion, tabla, uuid)
}

/// Sustituye los destinos de todos los snippets vivos. Un destino por host cuyo
/// uuid no esté vivo se descarta (§7.3).
fn reemplazar_destinos_de_snippets(
    conexion: &Connection,
    vivos: &VivosPlan,
    ids: &HashMap<Tabla, HashMap<String, i64>>,
) -> Result<()> {
    let hosts = ids.get(&Tabla::Hosts).cloned().unwrap_or_default();
    let snippets = ids.get(&Tabla::Snippets).cloned().unwrap_or_default();
    for (uuid, vivo) in &vivos.snippets {
        let Some(id) = snippets.get(uuid) else {
            continue;
        };
        reemplazar_destinos_de_snippet(conexion, *id, &vivo.datos.destinos, &hosts)?;
    }
    Ok(())
}

/// Reemplaza los SNIPPET_DESTINOS de un snippet. Los destinos por etiqueta
/// viajan por nombre; los de host, por uuid.
fn reemplazar_destinos_de_snippet(
    conexion: &Connection,
    snippet_id: i64,
    destinos: &[DestinoSnippet],
    hosts: &HashMap<String, i64>,
) -> Result<()> {
    conexion.execute(
        "DELETE FROM SNIPPET_DESTINOS WHERE snippet_id = ?1",
        params![snippet_id],
    )?;
    for destino in destinos {
        match (destino.etiqueta.as_deref(), destino.host.as_deref()) {
            (Some(etiqueta), None) => {
                let Some(etiqueta) = texto_no_vacio(etiqueta) else {
                    continue;
                };
                let etiqueta = etiqueta.to_lowercase();
                conexion.execute(
                    "INSERT OR IGNORE INTO SNIPPET_DESTINOS (snippet_id, etiqueta, host_id)
                     VALUES (?1, ?2, NULL)",
                    params![snippet_id, etiqueta],
                )?;
            }
            (None, Some(uuid)) => {
                let Some(host_id) = resolver_uuid(hosts, Some(uuid)) else {
                    // Un destino por host que ya no existe se descarta.
                    tracing::warn!(%uuid, "destino de snippet cuyo host no está vivo; se descarta");
                    continue;
                };
                conexion.execute(
                    "INSERT OR IGNORE INTO SNIPPET_DESTINOS (snippet_id, etiqueta, host_id)
                     VALUES (?1, NULL, ?2)",
                    params![snippet_id, host_id],
                )?;
            }
            _ => {
                tracing::warn!(
                    snippet = snippet_id,
                    "destino de snippet ilegible (debe tener etiqueta o host); se salta"
                );
            }
        }
    }
    Ok(())
}

/// Reconstruye ETIQUETAS con los nombres que usan los hosts y los destinos de
/// los snippets, y HOST_ETIQUETAS para cada host. Las etiquetas de los hosts que
/// no reescribe el plan (borrados aplazados, filas sin uuid) se conservan.
fn reconstruir_etiquetas(
    conexion: &Connection,
    vivos: &VivosPlan,
    ids: &HashMap<Tabla, HashMap<String, i64>>,
    etiquetas_anteriores: &HashMap<i64, Vec<String>>,
) -> Result<()> {
    let hosts = ids.get(&Tabla::Hosts).cloned().unwrap_or_default();
    let mut etiquetas_nuevas: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    for (uuid, vivo) in &vivos.hosts {
        let Some(id) = hosts.get(uuid) else {
            continue;
        };
        etiquetas_nuevas.insert(
            *id,
            crate::almacen::etiquetas::normalizar(&vivo.datos.etiquetas),
        );
    }
    // Filas que no están en el plan: se quedan con lo que tenían.
    let hosts_finales: Vec<(i64, Option<String>)> = {
        let mut sentencia = conexion.prepare("SELECT id, uuid FROM HOSTS ORDER BY id")?;
        let filas = sentencia.query_map([], |fila| Ok((fila.get(0)?, fila.get(1)?)))?;
        filas.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (id, uuid) in hosts_finales {
        let en_el_plan = uuid
            .as_deref()
            .is_some_and(|uuid| vivos.hosts.contains_key(uuid));
        if en_el_plan {
            continue;
        }
        if let Some(anteriores) = etiquetas_anteriores.get(&id) {
            etiquetas_nuevas.insert(id, anteriores.clone());
        }
    }

    conexion.execute("DELETE FROM HOST_ETIQUETAS", [])?;
    conexion.execute("DELETE FROM ETIQUETAS", [])?;
    for (host_id, nombres) in &etiquetas_nuevas {
        for nombre in nombres {
            conexion.execute(
                "INSERT OR IGNORE INTO ETIQUETAS (nombre) VALUES (?1)",
                params![nombre],
            )?;
            let etiqueta_id: i64 = conexion.query_row(
                "SELECT id FROM ETIQUETAS WHERE nombre = ?1",
                params![nombre],
                |fila| fila.get(0),
            )?;
            conexion.execute(
                "INSERT OR IGNORE INTO HOST_ETIQUETAS (host_id, etiqueta_id) VALUES (?1, ?2)",
                params![host_id, etiqueta_id],
            )?;
        }
    }

    // Los destinos de snippet guardan la etiqueta por nombre; las etiquetas que
    // solo usan ellos también viven en ETIQUETAS (autocompletado de la ficha).
    let etiquetas_de_destinos: Vec<String> = {
        let mut sentencia = conexion.prepare(
            "SELECT DISTINCT etiqueta FROM SNIPPET_DESTINOS
              WHERE etiqueta IS NOT NULL ORDER BY etiqueta",
        )?;
        let filas = sentencia.query_map([], |fila| fila.get::<_, String>(0))?;
        filas.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for etiqueta in etiquetas_de_destinos {
        conexion.execute(
            "INSERT OR IGNORE INTO ETIQUETAS (nombre) VALUES (?1)",
            params![etiqueta],
        )?;
    }
    Ok(())
}

/// Etiquetas de un host recuperado de un conflicto: las suyas, sin tocar las
/// demás.
fn reemplazar_etiquetas_de_host(
    conexion: &Connection,
    host_id: i64,
    nombres: &[String],
) -> Result<()> {
    conexion.execute(
        "DELETE FROM HOST_ETIQUETAS WHERE host_id = ?1",
        params![host_id],
    )?;
    for nombre in crate::almacen::etiquetas::normalizar(nombres) {
        conexion.execute(
            "INSERT OR IGNORE INTO ETIQUETAS (nombre) VALUES (?1)",
            params![nombre],
        )?;
        let etiqueta_id: i64 = conexion.query_row(
            "SELECT id FROM ETIQUETAS WHERE nombre = ?1",
            params![nombre],
            |fila| fila.get(0),
        )?;
        conexion.execute(
            "INSERT OR IGNORE INTO HOST_ETIQUETAS (host_id, etiqueta_id) VALUES (?1, ?2)",
            params![host_id, etiqueta_id],
        )?;
    }
    Ok(())
}

/// Upsert o borrado de VERIFICACIONES_HOST según lo que traiga el host.
fn actualizar_verificaciones_de_hosts(
    conexion: &Connection,
    vivos: &VivosPlan,
    ids: &HashMap<Tabla, HashMap<String, i64>>,
) -> Result<()> {
    let hosts = ids.get(&Tabla::Hosts).cloned().unwrap_or_default();
    for (uuid, vivo) in &vivos.hosts {
        let Some(id) = hosts.get(uuid) else {
            continue;
        };
        actualizar_verificaciones(conexion, *id, vivo.datos.verificaciones.as_ref())?;
    }
    Ok(())
}

fn actualizar_verificaciones(
    conexion: &Connection,
    host_id: i64,
    datos: Option<&DatosVerificaciones>,
) -> Result<()> {
    let Some(datos) = datos else {
        conexion.execute(
            "DELETE FROM VERIFICACIONES_HOST WHERE host_id = ?1",
            params![host_id],
        )?;
        return Ok(());
    };
    let datos = crate::deliberacion::DatosVerificaciones {
        salud: datos.salud,
        backup: datos.backup,
        backup_ruta: datos.backup_ruta.clone(),
        backup_patron: datos.backup_patron.clone(),
        tests: datos.tests,
        tests_comando: datos.tests_comando.clone(),
    }
    .normalizada();
    validar_verificaciones(&datos).map_err(|motivo| {
        anyhow::anyhow!("las verificaciones del host {host_id} del plan no son válidas: {motivo}")
    })?;
    conexion.execute(
        "INSERT INTO VERIFICACIONES_HOST (
            host_id, salud, backup, backup_ruta, backup_patron, tests, tests_comando,
            actualizado_en
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(host_id) DO UPDATE SET
            salud = excluded.salud,
            backup = excluded.backup,
            backup_ruta = excluded.backup_ruta,
            backup_patron = excluded.backup_patron,
            tests = excluded.tests,
            tests_comando = excluded.tests_comando,
            actualizado_en = excluded.actualizado_en",
        params![
            host_id,
            datos.salud as i64,
            datos.backup as i64,
            datos.backup_ruta,
            datos.backup_patron,
            datos.tests as i64,
            datos.tests_comando,
            fecha_ahora()
        ],
    )?;
    Ok(())
}

// ---------------------------------------------------------------- renombrados

/// Marca un elemento renombrado como edición nueva de este equipo: el contenido
/// (con el nombre nuevo) ya está escrito; ahora sube versión y vector.
fn aplicar_renombrado(
    conexion: &Connection,
    tabla: Tabla,
    uuid: &str,
    ids: &HashMap<Tabla, HashMap<String, i64>>,
    vivos_uuids: &HashMap<Tabla, BTreeSet<String>>,
) -> Result<bool> {
    let Some(uuid) = texto_no_vacio(uuid) else {
        return Ok(false);
    };
    let vivo = vivos_uuids
        .get(&tabla)
        .is_some_and(|uuids| uuids.contains(uuid));
    if !vivo {
        tracing::warn!(
            tabla = tabla.nombre(),
            uuid,
            "renombrado que no está vivo en el plan; no se aplica"
        );
        return Ok(false);
    }
    let Some(id) = ids.get(&tabla).and_then(|mapa| mapa.get(uuid)) else {
        tracing::warn!(
            tabla = tabla.nombre(),
            uuid,
            "renombrado sin fila local en el plan; no se aplica"
        );
        return Ok(false);
    };
    let vector_anterior = conexion
        .query_row(
            &format!("SELECT vector FROM {} WHERE id = ?1", tabla.nombre()),
            params![id],
            |fila| fila.get::<_, Option<String>>(0),
        )?
        .unwrap_or_else(|| "{}".to_string());
    let (version, vector) = versiones::version_de_edicion(conexion, &vector_anterior)?;
    conexion.execute(
        &format!(
            "UPDATE {} SET version = ?1, vector = ?2 WHERE id = ?3",
            tabla.nombre()
        ),
        params![version, vector, id],
    )?;
    Ok(true)
}

// ------------------------------------------------------------------ utilidades

fn nuevo_conflicto(conflicto: &ConflictoFusion) -> NuevoConflicto {
    NuevoConflicto {
        tipo: conflicto.tipo.clone(),
        elemento_tipo: conflicto.elemento_tipo.clone(),
        uuid: conflicto.uuid.clone(),
        nombre: conflicto.nombre.clone(),
        version_perdedora_json: conflicto.version_perdedora_json.clone(),
        version_ganadora: conflicto.version_ganadora.clone(),
        equipo_ganador: conflicto.equipo_ganador.clone(),
    }
}

/// Texto recortado, o None si queda vacío.
fn texto_no_vacio(texto: &str) -> Option<&str> {
    let texto = texto.trim();
    if texto.is_empty() {
        None
    } else {
        Some(texto)
    }
}

/// Referencia de identidad en la forma que se guarda: se expande ~ con el home
/// de este equipo y se deja la forma canónica que entiende IdentidadRef
/// (agente:, fichero:, contrasena...).
fn identidad_ref_para_guardar(texto: Option<&str>, hogar: &Path) -> Option<String> {
    let texto = texto_no_vacio(texto?)?;
    let canonico = match texto.strip_prefix("fichero:") {
        Some(ruta) => format!("fichero:{}", ruta_desde_home(ruta, hogar)),
        None => texto.to_string(),
    };
    IdentidadRef::desde_bd(Some(&canonico)).a_bd()
}

/// Etiquetas de un snippet en la forma que guarda la columna: minúsculas, sin
/// duplicados y separadas por espacios. El formato las documenta separadas por
/// comas; se aceptan las dos formas.
fn etiquetas_de_snippet(texto: &str) -> String {
    let mut etiquetas: Vec<String> = Vec::new();
    for etiqueta in texto.split(|caracter: char| caracter == ',' || caracter.is_whitespace()) {
        let etiqueta = etiqueta.trim().to_lowercase();
        if etiqueta.is_empty() || etiquetas.contains(&etiqueta) {
            continue;
        }
        etiquetas.push(etiqueta);
    }
    etiquetas.join(" ")
}

fn conflicto_de_nombre(que: &str, nombre: &str, error: rusqlite::Error) -> anyhow::Error {
    if crate::almacen::es_conflicto_unico(&error) {
        anyhow::anyhow!(
            "el plan trae un {que} con el nombre «{nombre}» que ya está ocupado en este equipo"
        )
    } else {
        anyhow::Error::new(error)
    }
}
