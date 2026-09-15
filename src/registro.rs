use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde::Serialize;

use crate::ficheros::escribir_atomico;
use crate::modelo::{fecha_ahora, EntradaRegistro, ResultadoRegistro};

/// Tipos de evento admitidos en el registro. Los valores se validan en código,
/// nunca con `CHECK` en SQLite.
pub const CONEXION_ABIERTA: &str = "conexion_abierta";
pub const CONEXION_FALLIDA: &str = "conexion_fallida";
pub const HUELLA_ACEPTADA: &str = "huella_aceptada";
pub const HUELLA_SUSTITUIDA: &str = "huella_sustituida";
pub const IMPORTACION: &str = "importacion";
pub const EXPORTACION: &str = "exportacion";
pub const CLAVE_GENERADA: &str = "clave_generada";
pub const CLAVE_IMPORTADA: &str = "clave_importada";
pub const REFERENCIA_REVOCADA: &str = "referencia_revocada";
pub const SONDEO_FALLIDO: &str = "sondeo_fallido";
pub const SONDEO_RECUPERADO: &str = "sondeo_recuperado";
pub const SESION_CERRADA: &str = "sesion_cerrada";
pub const SESION_RECONECTADA: &str = "sesion_reconectada";
pub const SERVIDOR_ARRANCADO: &str = "servidor_arrancado";
pub const SERVIDOR_DETENIDO: &str = "servidor_detenido";
pub const SERVIDOR_CAIDO: &str = "servidor_caido";

pub const TIPOS: [&str; 16] = [
    CONEXION_ABIERTA,
    CONEXION_FALLIDA,
    HUELLA_ACEPTADA,
    HUELLA_SUSTITUIDA,
    IMPORTACION,
    EXPORTACION,
    CLAVE_GENERADA,
    CLAVE_IMPORTADA,
    REFERENCIA_REVOCADA,
    SONDEO_FALLIDO,
    SONDEO_RECUPERADO,
    SESION_CERRADA,
    SESION_RECONECTADA,
    SERVIDOR_ARRANCADO,
    SERVIDOR_DETENIDO,
    SERVIDOR_CAIDO,
];

/// Filtros rápidos de la vista Registro (`t` cicla por ellos).
pub const FILTROS: [(&str, &[&str]); 6] = [
    ("todos", &[]),
    (
        "conexiones",
        &[
            CONEXION_ABIERTA,
            CONEXION_FALLIDA,
            SESION_CERRADA,
            SESION_RECONECTADA,
            SERVIDOR_ARRANCADO,
            SERVIDOR_DETENIDO,
            SERVIDOR_CAIDO,
        ],
    ),
    ("huellas", &[HUELLA_ACEPTADA, HUELLA_SUSTITUIDA]),
    (
        "claves",
        &[CLAVE_GENERADA, CLAVE_IMPORTADA, REFERENCIA_REVOCADA],
    ),
    ("importación", &[IMPORTACION, EXPORTACION]),
    ("sondeos", &[SONDEO_FALLIDO, SONDEO_RECUPERADO]),
];

#[derive(Debug, Clone, Default)]
pub struct FiltroRegistro {
    pub texto: String,
    pub tipos: Vec<&'static str>,
}

impl FiltroRegistro {
    pub fn indice_tipo(&self) -> usize {
        FILTROS
            .iter()
            .position(|(_, tipos)| *tipos == self.tipos.as_slice())
            .unwrap_or(0)
    }

    pub fn etiqueta_tipo(&self) -> &'static str {
        FILTROS[self.indice_tipo()].0
    }

    pub fn ciclo_tipo(&mut self) {
        let siguiente = (self.indice_tipo() + 1) % FILTROS.len();
        self.tipos = FILTROS[siguiente].1.to_vec();
    }
}

/// Única puerta de escritura en `REGISTRO`. Las filas nunca se editan: solo se
/// purgan por antigüedad.
pub fn anotar(
    conexion: &Connection,
    tipo: &str,
    host_id: Option<i64>,
    identidad_id: Option<i64>,
    detalle: &str,
    resultado: ResultadoRegistro,
) -> Result<()> {
    if !TIPOS.contains(&tipo) {
        anyhow::bail!("tipo de registro desconocido: «{tipo}»");
    }
    conexion
        .execute(
            "INSERT INTO REGISTRO (fecha, tipo, host_id, identidad_id, detalle, resultado)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                fecha_ahora(),
                tipo,
                host_id,
                identidad_id,
                detalle,
                resultado.como_texto(),
            ],
        )
        .context("escribiendo en REGISTRO")?;
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct FilaExportada {
    pub fecha: String,
    pub tipo: String,
    pub host: String,
    pub identidad: String,
    pub resultado: String,
    pub detalle: String,
}

pub fn filas_exportables(entradas: &[EntradaRegistro]) -> Vec<FilaExportada> {
    entradas
        .iter()
        .map(|entrada| FilaExportada {
            fecha: entrada.fecha.clone(),
            tipo: entrada.tipo.clone(),
            host: entrada
                .host_nombre
                .clone()
                .unwrap_or_else(|| "—".to_string()),
            identidad: entrada
                .identidad_alias
                .clone()
                .unwrap_or_else(|| "—".to_string()),
            resultado: entrada.resultado.como_texto().to_string(),
            detalle: entrada.detalle.clone(),
        })
        .collect()
}

/// Exporta las entradas a CSV con permisos 600.
pub fn exportar_csv(ruta: &Path, entradas: &[EntradaRegistro]) -> Result<usize> {
    let filas = filas_exportables(entradas);
    let mut escritor = csv::Writer::from_writer(Vec::new());
    for fila in &filas {
        escritor.serialize(fila)?;
    }
    let contenido = escritor
        .into_inner()
        .map_err(|error| anyhow::anyhow!("serializando CSV: {error}"))?;
    escribir_atomico(ruta, &contenido, 0o600)?;
    Ok(filas.len())
}

/// Exporta las entradas a JSON con permisos 600.
pub fn exportar_json(ruta: &Path, entradas: &[EntradaRegistro]) -> Result<usize> {
    let filas = filas_exportables(entradas);
    let contenido = serde_json::to_vec_pretty(&filas)?;
    escribir_atomico(ruta, &contenido, 0o600)?;
    Ok(filas.len())
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_filtro_rapido_cicla_por_todos_los_grupos() {
        let mut filtro = FiltroRegistro::default();
        assert_eq!(filtro.etiqueta_tipo(), "todos");
        for etiqueta in [
            "conexiones",
            "huellas",
            "claves",
            "importación",
            "sondeos",
            "todos",
        ] {
            filtro.ciclo_tipo();
            assert_eq!(filtro.etiqueta_tipo(), etiqueta);
        }
    }
}
