//! Fusión determinista de elementos por versión y vector (Fase 10, §7.2).
//!
//! Funciones puras: no tocan SQLite. Reciben el estado local ya convertido a
//! candidatos y los documentos de los demás equipos, y devuelven qué queda
//! vivo, qué lápidas hay, qué se renombra y qué conflictos hay que guardar.
//! El resultado no depende del orden ni del número de documentos.

use std::collections::BTreeMap;

use anyhow::Result;

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
    /// Mayor milisegundo visto entre todas las versiones.
    pub reloj_ms: i64,
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

/// Nombre natural de un elemento a partir de su contenido JSON. Sirve para
/// mostrar conflictos y para detectar colisiones de nombre.
pub fn nombre_de_contenido(tipo: &str, contenido: &serde_json::Value) -> String {
    let clave = match tipo {
        "grupo" | "host" | "snippet" => "nombre",
        "identidad" => "alias",
        "tunel" | "sincronizacion_dir" => "nombre",
        _ => "nombre",
    };
    contenido
        .get(clave)
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// Clave natural para detectar colisiones: el nombre y, en túneles y
/// sincronizaciones, el uuid del host.
pub fn clave_natural(tipo: &str, uuid_host: Option<&str>, contenido: &serde_json::Value) -> String {
    let nombre = nombre_de_contenido(tipo, contenido);
    match tipo {
        "tunel" | "sincronizacion_dir" => format!("{}|{}", uuid_host.unwrap_or(""), nombre),
        "host" | "grupo" | "snippet" | "identidad" => nombre,
        _ => uuid_host.map_or(nombre.clone(), |host| format!("{host}|{nombre}")),
    }
}

/// Fusión completa: estado local + documentos legibles. Pura y determinista.
///
/// Implementación pendiente en la Fase 10 (la hace el equipo de fusión).
pub fn fusionar(_local: &[Candidato], _documentos: &[Documento]) -> Result<PlanFusion> {
    anyhow::bail!("la fusión de la fase 10 aún no está implementada")
}

/// Aplica la regla de nombres de §7.3: si dos elementos distintos acaban con
/// la misma clave natural, el de uuid mayor pasa a "nombre (2)". Devuelve
/// los candidatos con el contenido renombrado y la lista de renombrados.
///
/// Implementación pendiente en la Fase 10.
pub fn resolver_nombres(_vivos: &mut [Candidato]) -> Vec<(String, String)> {
    Vec::new()
}
