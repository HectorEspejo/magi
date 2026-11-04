//! Formato del fichero de sincronización, `formato` 1 (Fase 10).
//!
//! El fichero es una instantánea completa del inventario de un equipo,
//! cifrada con age y con una firma HMAC dentro. En claro, antes de cifrar:
//!
//! \`\`\`text
//! MAGI-SINCRONIZACION 1
//! firma: <64 hex>
//! {"formato":1,...}
//! \`\`\`
//!
//! La firma es HMAC-SHA256 del JSON (los bytes que empiezan en la llave de
//! apertura y acaban en la de cierre, sin el salto de línea final) con una
//! clave derivada de la identidad de sincronización. El formato completo está
//! documentado en docs/formato-sincronizacion.md para la app Android.

use std::collections::BTreeMap;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// Versión del formato que escribe y entiende este binario.
pub const FORMATO: u32 = 1;

/// Primera línea del contenido en claro, antes del número de formato.
pub const CABECERA: &str = "MAGI-SINCRONIZACION";

/// Prefijo de los ficheros de equipo: magi-<uuid>.age.
pub const PREFIJO_FICHERO: &str = "magi-";

/// Extensión de los ficheros de equipo.
pub const EXTENSION_FICHERO: &str = ".age";

/// Namespace de los UUIDv5 de las identidades: el mismo en todos los equipos y
/// en la app Android. El uuid de una identidad es
/// UUIDv5(NAMESPACE_IDENTIDADES, huella UTF-8).
pub const NAMESPACE_IDENTIDADES: uuid::Uuid =
    uuid::Uuid::from_u128(0x3f0b_9c1e_6d24_4c8a_9e57_2a1d_5b7f_4c90);

/// Salt de HKDF para derivar la clave de firma.
pub const SAL_HKDF: &[u8] = b"magi";

/// Info de HKDF para derivar la clave de firma.
pub const INFO_FIRMA: &[u8] = b"magi-sincronizacion-firma-v1";

/// Documento JSON de una instantánea.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Documento {
    pub formato: u32,
    /// Versión de MAGI que escribió el fichero ("1.1.0").
    pub app: String,
    pub equipo: EquipoDocumento,
    /// Fecha ISO 8601 con zona del momento de la exportación.
    pub generado_en: String,
    /// Reloj híbrido del equipo que exportó.
    pub reloj_ms: i64,
    #[serde(default)]
    pub elementos: Vec<ElementoDocumento>,
    #[serde(default)]
    pub lapidas: Vec<LapidaDocumento>,
    #[serde(default)]
    pub huellas: Vec<HuellaDocumento>,
    #[serde(default)]
    pub equipos_quitados: Vec<EquipoQuitadoDocumento>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquipoDocumento {
    pub uuid: String,
    pub nombre: String,
    pub plataforma: String,
}

/// Un elemento vivo. Los campos que viajan van en `datos`, con la forma que
/// documenta docs/formato-sincronizacion.md según el tipo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementoDocumento {
    pub tipo: String,
    pub uuid: String,
    pub version: String,
    pub vector: BTreeMap<String, i64>,
    pub datos: serde_json::Value,
}

/// Una lápida: un elemento borrado. El contenido no viaja.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LapidaDocumento {
    pub tipo: String,
    pub uuid: String,
    pub nombre: String,
    pub version: String,
    pub vector: BTreeMap<String, i64>,
    pub borrado_en: String,
}

/// Una huella de known_hosts de un host del inventario.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HuellaDocumento {
    /// uuid del host al que pertenece (no su nombre).
    pub host: String,
    /// Tipo de clave ("ssh-ed25519", ...).
    pub tipo: String,
    /// Clave pública en base64, sin tipo ni comentario.
    pub clave: String,
    /// Cuándo la aceptó el equipo que exporta (ISO 8601).
    pub aceptada_en: String,
}

/// Un equipo que ya no sincroniza.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquipoQuitadoDocumento {
    pub uuid: String,
    pub quitado_en: String,
}

/// Contenido en claro de un fichero, ya separado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnClaro {
    pub formato: u32,
    pub firma: String,
    pub json: String,
}

/// Compone el contenido en claro: cabecera, firma y JSON con salto final.
pub fn componer(json: &str, firma: &str) -> Vec<u8> {
    let mut texto = String::with_capacity(json.len() + 128);
    texto.push_str(CABECERA);
    texto.push(' ');
    texto.push_str(&FORMATO.to_string());
    texto.push('\n');
    texto.push_str("firma: ");
    texto.push_str(firma);
    texto.push('\n');
    texto.push_str(json);
    texto.push('\n');
    texto.into_bytes()
}

/// Número de formato de una primera línea de cabecera, si la tiene.
pub fn formato_de_cabecera(linea: &str) -> Option<u32> {
    let resto = linea.strip_prefix(CABECERA)?.strip_prefix(' ')?;
    resto.trim().parse().ok()
}

/// Separa cabecera, firma y JSON del contenido en claro. La firma se calcula
/// sobre `json`, tal cual se devuelve aquí (sin el salto de línea final).
pub fn separar(plano: &[u8]) -> Result<EnClaro> {
    let texto =
        std::str::from_utf8(plano).map_err(|_| anyhow::anyhow!("el contenido no es UTF-8"))?;
    let (cabecera, resto) = texto
        .split_once('\n')
        .ok_or_else(|| anyhow::anyhow!("falta la cabecera del fichero"))?;
    let formato = formato_de_cabecera(cabecera)
        .ok_or_else(|| anyhow::anyhow!("cabecera de sincronización no válida"))?;
    let (linea_firma, resto) = resto
        .split_once('\n')
        .ok_or_else(|| anyhow::anyhow!("falta la línea de firma"))?;
    let firma = linea_firma
        .strip_prefix("firma: ")
        .ok_or_else(|| anyhow::anyhow!("falta la línea de firma"))?
        .trim()
        .to_string();
    if firma.is_empty() {
        bail!("falta la línea de firma");
    }
    let json = resto.strip_suffix('\n').unwrap_or(resto).to_string();
    if !json.starts_with('{') {
        bail!("falta el JSON del documento");
    }
    Ok(EnClaro {
        formato,
        firma,
        json,
    })
}

/// Serializa el documento a JSON compacto.
pub fn serializar(documento: &Documento) -> Result<String> {
    serde_json::to_string(documento)
        .map_err(|error| anyhow::anyhow!("serializando el documento: {error}"))
}

/// Parsea un JSON de documento y comprueba que el formato no es más nuevo.
pub fn parsear(json: &str) -> Result<Documento> {
    let documento: Documento =
        serde_json::from_str(json).map_err(|error| anyhow::anyhow!("JSON no válido: {error}"))?;
    if documento.formato > FORMATO {
        bail!(
            "el documento usa el formato {} y este MAGI conoce el {}",
            documento.formato,
            FORMATO
        );
    }
    Ok(documento)
}
