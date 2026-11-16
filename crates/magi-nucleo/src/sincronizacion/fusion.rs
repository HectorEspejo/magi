//! Fusión determinista de elementos por versión y vector (Fase 10, §7.2 y §7.3).
//!
//! Funciones puras: no tocan SQLite. Reciben el estado local ya convertido a
//! candidatos y los documentos de los demás equipos, y devuelven qué queda
//! vivo, qué lápidas hay, qué se renombra y qué conflictos hay que guardar.
//! El resultado no depende del orden ni del número de documentos: para cada
//! uuid se elige el ganador con un orden total y las listas salen ordenadas.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;

use crate::almacen::conflictos::{TIPO_ELEMENTO, TIPO_HUERFANO, TIPO_NOMBRE};
use crate::sesiones_persistentes::union::{self, FichaUnible};
use crate::sincronizacion::elementos::DatosSesionPersistente;
use crate::sincronizacion::formato::Documento;
use crate::sincronizacion::reloj;

/// Un elemento (vivo o lápida) que participa en la fusión.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidato {
    pub tipo: String,
    pub uuid: String,
    /// Nombre para mostrar (en las lápidas viene del fichero; en los vivos,
    /// extraído de `contenido`).
    pub nombre: String,
    pub version: String,
    pub vector: BTreeMap<String, i64>,
    /// Contenido del elemento; None en una lápida.
    pub contenido: Option<serde_json::Value>,
    /// Fecha del borrado; solo en lápidas.
    pub borrado_en: Option<String>,
}

impl Candidato {
    pub fn es_lapida(&self) -> bool {
        self.contenido.is_none()
    }

    /// JSON canónico del contenido, para comparar dos versiones.
    pub fn contenido_json(&self) -> Option<String> {
        self.contenido
            .as_ref()
            .map(|valor| serde_json::to_string(valor).unwrap_or_default())
    }

    /// Equipo autor de la versión.
    pub fn equipo(&self) -> Option<&str> {
        reloj::uuid_de_version(&self.version)
    }
}

/// Un conflicto detectado en la fusión, antes de escribirlo en CONFLICTOS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictoFusion {
    /// elemento, nombre, huella, borrado u huerfano.
    pub tipo: String,
    pub elemento_tipo: String,
    pub uuid: String,
    pub nombre: String,
    pub version_perdedora_json: Option<String>,
    pub version_ganadora: Option<String>,
    pub equipo_ganador: Option<String>,
}

/// Resultado completo de una fusión.
#[derive(Debug, Clone, Default)]
pub struct PlanFusion {
    /// Elementos vivos finales, con la versión y el vector ganadores.
    pub vivos: Vec<Candidato>,
    /// Lápidas finales (todas las del universo, también las de elementos que
    /// resucitaron: se conservan siempre).
    pub lapidas: Vec<Candidato>,
    /// Elementos renombrados por colisión de nombre: tipo y uuid. Se aplican
    /// como edición nueva cuando se escriben en el almacén.
    pub renombrados: Vec<(String, String)>,
    /// Conflictos que hay que guardar.
    pub conflictos: Vec<ConflictoFusion>,
    /// Fichas de sesión persistente unidas por tener el mismo host y nombre
    /// (D337). La ganadora ya está en `vivos` con sus campos completados y la
    /// perdedora ya no está; la lápida de la perdedora y la edición de la
    /// ganadora las escribe la aplicación (aquí no hay reloj).
    pub unidas: Vec<UnionFicha>,
    /// Mayor milisegundo visto entre todas las versiones.
    pub reloj_ms: i64,
}

/// Una ficha de sesión persistente absorbida por otra con el mismo host y
/// nombre (§7.5, D337). Con tres o más fichas iguales hay una entrada por cada
/// perdedora.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnionFicha {
    /// uuid de la ficha que se queda (el menor).
    pub ganadora: String,
    /// uuid de la ficha que se borra con lápida.
    pub perdedora: String,
    /// Nombre de la sesión (el de tmux), para la lápida y el conflicto.
    pub nombre: String,
    /// Vector de la perdedora tal como se fusionó: la lápida parte de él para
    /// dominar las ediciones que ya se habían visto.
    pub vector_perdedora: BTreeMap<String, i64>,
    /// La ganadora ha recibido campos de la perdedora: al aplicar es una
    /// edición nueva de este equipo.
    pub ganadora_editada: bool,
}

/// Un candidato del universo con su origen. Solo los locales pueden dejar
/// conflicto de elemento: el conflicto se guarda donde se pierde.
struct EnUniverso {
    candidato: Candidato,
    local: bool,
}

/// Convierte un documento a candidatos (elementos vivos y lápidas).
pub fn candidatos_de_documento(documento: &Documento) -> Vec<Candidato> {
    let mut candidatos = Vec::with_capacity(documento.elementos.len() + documento.lapidas.len());
    for elemento in &documento.elementos {
        candidatos.push(Candidato {
            tipo: elemento.tipo.clone(),
            uuid: elemento.uuid.clone(),
            nombre: nombre_de_contenido(&elemento.tipo, &elemento.datos),
            version: elemento.version.clone(),
            vector: elemento.vector.clone(),
            contenido: Some(elemento.datos.clone()),
            borrado_en: None,
        });
    }
    for lapida in &documento.lapidas {
        candidatos.push(Candidato {
            tipo: lapida.tipo.clone(),
            uuid: lapida.uuid.clone(),
            nombre: lapida.nombre.clone(),
            version: lapida.version.clone(),
            vector: lapida.vector.clone(),
            contenido: None,
            borrado_en: Some(lapida.borrado_en.clone()),
        });
    }
    candidatos
}

/// Campo del contenido donde va el nombre natural de cada tipo.
fn clave_nombre(tipo: &str) -> &'static str {
    match tipo {
        "identidad" => "alias",
        // La comprobación propia no tiene columna de nombre: su nombre natural
        // para mostrar (y para las lápidas) es el objetivo.
        "comprobacion_servicio" => "objetivo",
        _ => "nombre",
    }
}

/// Nombre natural de un elemento a partir de su contenido JSON. Sirve para
/// mostrar conflictos y para detectar colisiones de nombre.
pub fn nombre_de_contenido(tipo: &str, contenido: &serde_json::Value) -> String {
    contenido
        .get(clave_nombre(tipo))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// Clave natural para detectar colisiones: el nombre y, en túneles,
/// sincronizaciones y comprobaciones de servicio, el uuid del host.
pub fn clave_natural(tipo: &str, uuid_host: Option<&str>, contenido: &serde_json::Value) -> String {
    let nombre = nombre_de_contenido(tipo, contenido);
    match tipo {
        "tunel" | "sincronizacion_dir" | "sesion_persistente" => {
            format!("{}|{}", uuid_host.unwrap_or(""), nombre)
        }
        "comprobacion_servicio" => format!(
            "{}|{}|{}",
            uuid_host.unwrap_or(""),
            contenido
                .get("objetivo_tipo")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
            nombre
        ),
        "host" | "grupo" | "snippet" | "identidad" => nombre,
        _ => uuid_host.map_or(nombre.clone(), |host| format!("{host}|{nombre}")),
    }
}

/// uuid del host al que cuelga un túnel, una sincronización de directorio o
/// una comprobación de servicio.
fn host_de_contenido(candidato: &Candidato) -> Option<&str> {
    candidato.contenido.as_ref()?.get("host")?.as_str()
}

/// ¿El host de un candidato vivo es SSH? Solo los hosts SSH tienen servicios y
/// sesiones persistentes, así que una comprobación o una ficha que cuelga de
/// otro tipo es huérfana (§7.10 de la Fase 13 y §3.1 de la Fase 16). Un
/// host sin campo `tipo` viene de un fichero del formato 1 (todos eran SSH).
fn host_es_ssh(candidato: &Candidato) -> bool {
    match candidato
        .contenido
        .as_ref()
        .and_then(|contenido| contenido.get("tipo"))
    {
        None | Some(serde_json::Value::Null) => true,
        Some(serde_json::Value::String(tipo)) => tipo == "ssh",
        Some(_) => false,
    }
}

/// Orden total entre candidatos para desempatar de forma determinista: primero
/// la versión (ms y uuid del equipo), después el contenido canónico, la fecha
/// de borrado, el tipo y el nombre.
fn orden_de_ganador(a: &Candidato, b: &Candidato) -> Ordering {
    reloj::comparar(&a.version, &b.version)
        .then_with(|| a.contenido_json().cmp(&b.contenido_json()))
        .then_with(|| a.borrado_en.cmp(&b.borrado_en))
        .then_with(|| a.tipo.cmp(&b.tipo))
        .then_with(|| a.nombre.cmp(&b.nombre))
}

/// Elige el candidato ganador de un uuid, de forma determinista e
/// independiente del orden de los candidatos.
fn elegir_ganador<'a>(candidatos: &[&'a Candidato]) -> &'a Candidato {
    // 1. Si un candidato domina a todos los demás, gana ese. Con vectores
    //    iguales puede haber varios; desempata el orden total.
    let mut dominantes = Vec::new();
    for (indice, candidato) in candidatos.iter().enumerate() {
        if candidatos
            .iter()
            .all(|otro| reloj::domina(&candidato.vector, &otro.vector))
        {
            dominantes.push(indice);
        }
    }
    // 2. Si no hay dominante, son concurrentes: gana la versión más reciente
    //    entre los que no están dominados estrictamente por otro.
    let indices = if dominantes.is_empty() {
        let mut maximales = Vec::new();
        for (indice, candidato) in candidatos.iter().enumerate() {
            let dominado = candidatos.iter().enumerate().any(|(otro_indice, otro)| {
                otro_indice != indice
                    && reloj::domina(&otro.vector, &candidato.vector)
                    && !reloj::domina(&candidato.vector, &otro.vector)
            });
            if !dominado {
                maximales.push(indice);
            }
        }
        // Solo se llega aquí con datos incoherentes (vectores iguales que se
        // dominan mutuamente): entonces participan todos.
        if maximales.is_empty() {
            (0..candidatos.len()).collect()
        } else {
            maximales
        }
    } else {
        dominantes
    };
    indices
        .into_iter()
        .map(|indice| candidatos[indice])
        .max_by(|a, b| orden_de_ganador(a, b))
        .expect("el universo de un uuid siempre tiene al menos un candidato")
}

/// Conflicto de elemento: la versión local perdió frente a una concurrente con
/// contenido distinto. Una lápida local no deja nada que recuperar.
fn conflicto_de_elemento(local: &Candidato, ganador: &Candidato) -> Option<ConflictoFusion> {
    let contenido_local = local.contenido_json()?;
    if local.contenido == ganador.contenido {
        return None;
    }
    if !reloj::concurrentes(&local.vector, &ganador.vector) {
        return None;
    }
    Some(ConflictoFusion {
        tipo: TIPO_ELEMENTO.to_string(),
        elemento_tipo: local.tipo.clone(),
        uuid: local.uuid.clone(),
        nombre: local.nombre.clone(),
        version_perdedora_json: Some(contenido_local),
        version_ganadora: Some(ganador.version.clone()),
        equipo_ganador: ganador.equipo().map(str::to_string),
    })
}

/// Conflicto de huérfano: el elemento vivo se descarta porque su host ya no
/// está vivo. Se anota la versión descartada para poder mostrarla.
fn conflicto_huerfano(candidato: &Candidato) -> ConflictoFusion {
    ConflictoFusion {
        tipo: TIPO_HUERFANO.to_string(),
        elemento_tipo: candidato.tipo.clone(),
        uuid: candidato.uuid.clone(),
        nombre: candidato.nombre.clone(),
        version_perdedora_json: None,
        version_ganadora: Some(candidato.version.clone()),
        equipo_ganador: candidato.equipo().map(str::to_string),
    }
}

/// La lápida más reciente de un grupo (por versión), si hay alguna.
fn lapida_mas_reciente<'a>(candidatos: &[&'a Candidato]) -> Option<&'a Candidato> {
    candidatos
        .iter()
        .copied()
        .filter(|candidato| candidato.es_lapida())
        .max_by(|a, b| orden_de_ganador(a, b))
}

/// Añade un conflicto si no hay ya uno equivalente (una pasada repetida no
/// llena la lista).
fn anyadir_conflicto(conflictos: &mut Vec<ConflictoFusion>, conflicto: ConflictoFusion) {
    if !conflictos.contains(&conflicto) {
        conflictos.push(conflicto);
    }
}

/// Orden canónico de los conflictos, para que el plan no dependa del orden de
/// entrada.
fn orden_de_conflicto(a: &ConflictoFusion, b: &ConflictoFusion) -> Ordering {
    a.tipo
        .cmp(&b.tipo)
        .then_with(|| a.uuid.cmp(&b.uuid))
        .then_with(|| a.elemento_tipo.cmp(&b.elemento_tipo))
        .then_with(|| a.nombre.cmp(&b.nombre))
        .then_with(|| a.version_perdedora_json.cmp(&b.version_perdedora_json))
        .then_with(|| a.version_ganadora.cmp(&b.version_ganadora))
        .then_with(|| a.equipo_ganador.cmp(&b.equipo_ganador))
}

/// Fusión completa: estado local + documentos legibles. Pura y determinista.
///
/// Para cada uuid junta el elemento o lápida local y todos los de los
/// documentos; si un candidato domina al resto gana ese; si no, gana la versión
/// más reciente entre los concurrentes y el vector resultante es el máximo
/// componente a componente de todos. Las lápidas del universo se conservan
/// siempre, también las de elementos resucitados. Después descarta los
/// huérfanos (túnel o sincronización sin host vivo) y resuelve las colisiones
/// de nombre de los vivos.
pub fn fusionar(local: &[Candidato], documentos: &[Documento]) -> Result<PlanFusion> {
    // 1. Universo: por uuid, el estado local y todos los de los ficheros.
    let mut universo: BTreeMap<String, Vec<EnUniverso>> = BTreeMap::new();
    for candidato in local {
        universo
            .entry(candidato.uuid.clone())
            .or_default()
            .push(EnUniverso {
                candidato: candidato.clone(),
                local: true,
            });
    }
    for documento in documentos {
        for candidato in candidatos_de_documento(documento) {
            universo
                .entry(candidato.uuid.clone())
                .or_default()
                .push(EnUniverso {
                    candidato,
                    local: false,
                });
        }
    }

    let mut vivos: Vec<Candidato> = Vec::new();
    let mut lapidas: Vec<Candidato> = Vec::new();
    let mut conflictos: Vec<ConflictoFusion> = Vec::new();
    let mut reloj_ms: i64 = 0;

    // 2. Fusión por uuid.
    for grupo in universo.into_values() {
        let candidatos: Vec<&Candidato> = grupo.iter().map(|entrada| &entrada.candidato).collect();
        for candidato in &candidatos {
            if let Some(ms) = reloj::ms_de_version(&candidato.version) {
                reloj_ms = reloj_ms.max(ms);
            }
        }
        // El vector resultante «ve» todas las ediciones fusionadas.
        let vector = reloj::vector_max(candidatos.iter().map(|candidato| &candidato.vector));
        let ganador = elegir_ganador(&candidatos).clone();
        // Solo el perdedor local deja conflicto: cada equipo guarda el suyo.
        for entrada in &grupo {
            if !entrada.local {
                continue;
            }
            if let Some(conflicto) = conflicto_de_elemento(&entrada.candidato, &ganador) {
                anyadir_conflicto(&mut conflictos, conflicto);
            }
        }
        let mut fusionado = ganador;
        fusionado.vector = vector;
        if fusionado.es_lapida() {
            lapidas.push(fusionado);
        } else {
            // La lápida del elemento que resucita se conserva con su propio
            // vector: así nunca domina a la versión viva.
            if let Some(lapida) = lapida_mas_reciente(&candidatos) {
                lapidas.push(lapida.clone());
            }
            vivos.push(fusionado);
        }
    }

    // 3. Huérfanos: un túnel, una sincronización, una comprobación de servicio o
    //    una ficha de sesión persistente cuyo host ya no está vivo (en la
    //    comprobación y en la ficha, o ya no es SSH) se descarta; su lápida, si
    //    la hay, se queda en `lapidas`.
    let hosts_vivos: BTreeMap<String, bool> = vivos
        .iter()
        .filter(|candidato| candidato.tipo == "host")
        .map(|candidato| (candidato.uuid.clone(), host_es_ssh(candidato)))
        .collect();
    vivos.retain(|candidato| {
        if !matches!(
            candidato.tipo.as_str(),
            "tunel" | "sincronizacion_dir" | "comprobacion_servicio" | "sesion_persistente"
        ) {
            return true;
        }
        let host = host_de_contenido(candidato).unwrap_or("");
        let sirve = match hosts_vivos.get(host) {
            Some(es_ssh) => {
                !matches!(
                    candidato.tipo.as_str(),
                    "comprobacion_servicio" | "sesion_persistente"
                ) || *es_ssh
            }
            None => false,
        };
        if sirve {
            return true;
        }
        anyadir_conflicto(&mut conflictos, conflicto_huerfano(candidato));
        false
    });

    // 3b. Fichas de sesión persistente con el mismo host y nombre: son la misma
    //     sesión de tmux y se unen en la de uuid menor (D337, §7.5). Va antes
    //     de los nombres, que no las renombran: «x (2)» no es una sesión que
    //     exista en el host.
    let unidas = unir_fichas(&mut vivos, &mut conflictos);

    // 4. Nombres: se resuelven sobre los vivos que quedan, después de los
    //    huérfanos y de las uniones, y cada renombrado deja un conflicto de
    //    tipo nombre.
    let renombrados = resolver_nombres(&mut vivos);
    for (tipo, uuid) in &renombrados {
        let Some(candidato) = vivos
            .iter()
            .find(|candidato| &candidato.tipo == tipo && &candidato.uuid == uuid)
        else {
            continue;
        };
        anyadir_conflicto(
            &mut conflictos,
            ConflictoFusion {
                tipo: TIPO_NOMBRE.to_string(),
                elemento_tipo: candidato.tipo.clone(),
                uuid: candidato.uuid.clone(),
                nombre: candidato.nombre.clone(),
                version_perdedora_json: None,
                version_ganadora: Some(candidato.version.clone()),
                equipo_ganador: candidato.equipo().map(str::to_string),
            },
        );
    }

    // 5. Orden canónico: el plan no depende del orden de los documentos.
    conflictos.sort_by(orden_de_conflicto);
    conflictos.dedup();
    Ok(PlanFusion {
        vivos,
        lapidas,
        renombrados,
        conflictos,
        unidas,
        reloj_ms,
    })
}

/// Campo de texto opcional del contenido de una ficha; vacío cuenta como falta.
fn texto_de_ficha(contenido: &serde_json::Value, campo: &str) -> Option<String> {
    contenido
        .get(campo)
        .and_then(serde_json::Value::as_str)
        .filter(|texto| !texto.is_empty())
        .map(str::to_string)
}

/// ¿Los datos de la ficha son los de una ficha válida (§7.7)? Es lo mismo que
/// comprueba la aplicación antes de escribirla.
fn ficha_valida(candidato: &Candidato) -> bool {
    candidato
        .contenido
        .as_ref()
        .and_then(|contenido| {
            serde_json::from_value::<DatosSesionPersistente>(contenido.clone()).ok()
        })
        .is_some_and(|datos| datos.validar().is_ok())
}

/// Une las fichas de sesión persistente vivas que comparten host y nombre
/// (D337, §7.5): queda la de `uuid` menor con lo que le falte de las demás, y
/// las otras desaparecen del plan. Cada perdedora deja un conflicto de tipo
/// `nombre` (sin versión que recuperar: recuperarla duplicaría el nombre).
///
/// Es pura y determinista: el resultado no depende del orden de los vivos. La
/// lápida de la perdedora y la edición nueva de la ganadora se materializan al
/// aplicar el plan, porque aquí no hay reloj.
fn unir_fichas(
    vivos: &mut Vec<Candidato>,
    conflictos: &mut Vec<ConflictoFusion>,
) -> Vec<UnionFicha> {
    // Grupos por (host, nombre) con el índice de cada ficha en `vivos`.
    let mut grupos: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
    for (indice, candidato) in vivos.iter().enumerate() {
        // Una ficha que no valida (la aplicación la descartará como conflicto
        // de elemento) no se une: si no, podría llevarse por delante a una
        // ficha buena con el mismo nombre.
        if candidato.tipo != "sesion_persistente" || !ficha_valida(candidato) {
            continue;
        }
        let host = host_de_contenido(candidato).unwrap_or("").to_string();
        grupos
            .entry((host, candidato.nombre.clone()))
            .or_default()
            .push(indice);
    }

    let mut unidas: Vec<UnionFicha> = Vec::new();
    let mut perdedoras: BTreeSet<String> = BTreeSet::new();
    for ((_host, nombre), indices) in grupos {
        if indices.len() < 2 {
            continue;
        }
        let fichas: Vec<FichaUnible> = indices
            .iter()
            .map(|&indice| {
                let candidato = &vivos[indice];
                let contenido = candidato.contenido.clone().unwrap_or_default();
                FichaUnible {
                    uuid: candidato.uuid.clone(),
                    carpeta: texto_de_ficha(&contenido, "carpeta"),
                    comando: texto_de_ficha(&contenido, "comando"),
                    comando_reanudar: texto_de_ficha(&contenido, "comando_reanudar"),
                    nota: texto_de_ficha(&contenido, "nota"),
                }
            })
            .collect();
        let Some(resultado) = union::unir_grupo(&fichas) else {
            continue;
        };
        let Some(indice_ganadora) = indices
            .iter()
            .copied()
            .find(|&indice| vivos[indice].uuid == resultado.ganadora.uuid)
        else {
            continue;
        };
        // La ganadora se queda con los campos que le faltaban. Solo se toca el
        // contenido si ha cambiado algo: una unión sin aportaciones deja la
        // ficha ganadora tal cual y no es una edición.
        if resultado.editada {
            if let Some(serde_json::Value::Object(mapa)) = vivos[indice_ganadora].contenido.as_mut()
            {
                for campo in union::CAMPOS_UNIBLES {
                    let valor = resultado.ganadora.valor(campo);
                    let actual = mapa
                        .get(campo)
                        .and_then(serde_json::Value::as_str)
                        .filter(|texto| !texto.is_empty());
                    if valor != actual {
                        mapa.insert(
                            campo.to_string(),
                            valor.map_or(serde_json::Value::Null, |texto| {
                                serde_json::Value::String(texto.to_string())
                            }),
                        );
                    }
                }
            }
        }
        let version_ganadora = vivos[indice_ganadora].version.clone();
        let equipo_ganador = vivos[indice_ganadora].equipo().map(str::to_string);
        for &indice in &indices {
            if indice == indice_ganadora {
                continue;
            }
            let perdedora = &vivos[indice];
            unidas.push(UnionFicha {
                ganadora: resultado.ganadora.uuid.clone(),
                perdedora: perdedora.uuid.clone(),
                nombre: nombre.clone(),
                vector_perdedora: perdedora.vector.clone(),
                ganadora_editada: resultado.editada,
            });
            anyadir_conflicto(
                conflictos,
                ConflictoFusion {
                    tipo: TIPO_NOMBRE.to_string(),
                    elemento_tipo: perdedora.tipo.clone(),
                    uuid: perdedora.uuid.clone(),
                    nombre: nombre.clone(),
                    version_perdedora_json: None,
                    version_ganadora: Some(version_ganadora.clone()),
                    equipo_ganador: equipo_ganador.clone(),
                },
            );
            perdedoras.insert(perdedora.uuid.clone());
        }
    }
    vivos.retain(|candidato| {
        candidato.tipo != "sesion_persistente" || !perdedoras.contains(&candidato.uuid)
    });
    unidas.sort_by(|a, b| {
        a.perdedora
            .cmp(&b.perdedora)
            .then_with(|| a.ganadora.cmp(&b.ganadora))
    });
    unidas
}

/// Escribe el nombre nuevo en el candidato y en su contenido (`nombre` o
/// `alias` en las identidades).
fn poner_nombre(candidato: &mut Candidato, nuevo: &str) {
    candidato.nombre = nuevo.to_string();
    let clave = clave_nombre(&candidato.tipo).to_string();
    if let Some(serde_json::Value::Object(mapa)) = candidato.contenido.as_mut() {
        mapa.insert(clave, serde_json::Value::String(nuevo.to_string()));
    }
}

/// Ámbito de unicidad de los nombres: el tipo y, en túneles y sincronizaciones
/// de directorio, el uuid del host al que cuelgan.
fn ambito_de_nombre(candidato: &Candidato) -> (String, String) {
    let host = if matches!(candidato.tipo.as_str(), "tunel" | "sincronizacion_dir") {
        host_de_contenido(candidato).unwrap_or("").to_string()
    } else {
        String::new()
    };
    (candidato.tipo.clone(), host)
}

/// Aplica la regla de nombres de §7.3: si dos elementos distintos acaban con
/// la misma clave natural, el de uuid menor conserva el nombre y los demás, por
/// orden creciente de uuid, pasan a "<nombre> (2)", "<nombre> (3)"… (el primer
/// número libre). Devuelve los renombrados como (tipo, uuid).
///
/// Las comprobaciones de servicio no se renombran: su nombre es el objetivo
/// (una unidad o un proyecto) y un nombre inventado dejaría de ser un objetivo
/// válido. Una colisión real entre dos comprobaciones se resuelve al aplicar,
/// como conflicto de nombre, sin tocar el contenido.
///
/// Las fichas de sesión persistentes tampoco: su nombre es el de una sesión de
/// tmux y «x (2)» no existe en el host. Dos con el mismo host y nombre se unen
/// antes (D337, `unir_fichas`).
pub fn resolver_nombres(vivos: &mut [Candidato]) -> Vec<(String, String)> {
    // Agrupa los índices por ámbito de unicidad.
    let mut ambitos: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
    for (indice, candidato) in vivos.iter().enumerate() {
        if matches!(
            candidato.tipo.as_str(),
            "comprobacion_servicio" | "sesion_persistente"
        ) {
            continue;
        }
        ambitos
            .entry(ambito_de_nombre(candidato))
            .or_default()
            .push(indice);
    }

    let mut renombrados: Vec<(String, String)> = Vec::new();
    for ((tipo, _host), indices) in ambitos {
        // Nombres ya ocupados en el ámbito (los originales y los asignados).
        let mut ocupados: BTreeSet<String> = indices
            .iter()
            .map(|&indice| vivos[indice].nombre.clone())
            .collect();
        // Dentro del ámbito, los que comparten nombre natural.
        let mut por_nombre: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for indice in indices {
            por_nombre
                .entry(vivos[indice].nombre.clone())
                .or_default()
                .push(indice);
        }
        for (nombre, mut indices) in por_nombre {
            if indices.len() < 2 {
                continue;
            }
            indices.sort_by(|a, b| vivos[*a].uuid.cmp(&vivos[*b].uuid));
            // El mismo uuid no se renombra contra sí mismo; el de uuid menor
            // conserva el nombre.
            let mut vistos: BTreeSet<String> = BTreeSet::new();
            let mut primero = true;
            for indice in indices {
                if !vistos.insert(vivos[indice].uuid.clone()) {
                    continue;
                }
                if primero {
                    primero = false;
                    continue;
                }
                let mut numero: u64 = 2;
                let nuevo = loop {
                    let propuesto = format!("{nombre} ({numero})");
                    if !ocupados.contains(&propuesto) {
                        break propuesto;
                    }
                    numero += 1;
                };
                ocupados.insert(nuevo.clone());
                poner_nombre(&mut vivos[indice], &nuevo);
                renombrados.push((tipo.clone(), vivos[indice].uuid.clone()));
            }
        }
    }
    renombrados.sort();
    renombrados.dedup();
    renombrados
}
