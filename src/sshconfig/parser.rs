use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directiva {
    pub nombre: String,
    pub valor: String,
    pub linea: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BloqueHost {
    pub patrones: Vec<String>,
    pub directivas: Vec<Directiva>,
    pub linea: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Parseo {
    pub bloques: Vec<BloqueHost>,
    pub avisos: Vec<String>,
}

/// Parsea un texto ssh_config sin seguir los `Include` (los anota como
/// aviso).
pub fn parsear(texto: &str) -> Vec<BloqueHost> {
    let mut bloques = Vec::new();
    let mut actual: Option<BloqueHost> = None;
    for (indice, linea_bruta) in texto.lines().enumerate() {
        let numero = indice + 1;
        let linea = limpiar_linea(linea_bruta);
        let linea = linea.trim();
        if linea.is_empty() {
            continue;
        }
        let (clave, valor) = partir_directiva(linea);
        match clave.to_lowercase().as_str() {
            "host" => {
                if let Some(bloque) = actual.take() {
                    bloques.push(bloque);
                }
                actual = Some(BloqueHost {
                    patrones: valor
                        .split_whitespace()
                        .map(|patron| patron.trim_matches('"').to_string())
                        .filter(|patron| !patron.is_empty())
                        .collect(),
                    directivas: Vec::new(),
                    linea: numero,
                });
            }
            "match" => {
                if let Some(bloque) = actual.take() {
                    bloques.push(bloque);
                }
            }
            _ => {
                if let Some(bloque) = actual.as_mut() {
                    bloque.directivas.push(Directiva {
                        nombre: clave.to_string(),
                        valor: valor.to_string(),
                        linea: numero,
                    });
                }
            }
        }
    }
    if let Some(bloque) = actual.take() {
        bloques.push(bloque);
    }
    bloques
}

/// Lee un ssh_config siguiendo los `Include` un solo nivel (ignorando
/// `magi_config`) y devolviendo solo los bloques `Host`.
pub fn leer_config(ruta: &Path, hogar: &Path) -> Result<Parseo> {
    if !ruta.exists() {
        anyhow::bail!("no existe el fichero {}", ruta.display());
    }
    let mut parseo = Parseo::default();
    let mut visitados = HashSet::new();
    leer_fichero(ruta, hogar, 1, &mut visitados, &mut parseo)
        .with_context(|| format!("leyendo {}", ruta.display()))?;
    Ok(parseo)
}

fn leer_fichero(
    ruta: &Path,
    hogar: &Path,
    profundidad: u8,
    visitados: &mut HashSet<PathBuf>,
    parseo: &mut Parseo,
) -> Result<()> {
    let ruta = ruta.canonicalize().unwrap_or_else(|_| ruta.to_path_buf());
    if !visitados.insert(ruta.clone()) {
        return Ok(());
    }
    let texto = std::fs::read_to_string(&ruta)
        .with_context(|| format!("no se pudo leer {}", ruta.display()))?;
    let mut bloque_actual: Option<BloqueHost> = None;
    for (indice, linea_bruta) in texto.lines().enumerate() {
        let numero = indice + 1;
        let linea = limpiar_linea(linea_bruta);
        let linea = linea.trim();
        if linea.is_empty() {
            continue;
        }
        let (clave, valor) = partir_directiva(linea);
        match clave.to_lowercase().as_str() {
            "host" => {
                if let Some(bloque) = bloque_actual.take() {
                    parseo.bloques.push(bloque);
                }
                bloque_actual = Some(BloqueHost {
                    patrones: valor
                        .split_whitespace()
                        .map(|patron| patron.trim_matches('"').to_string())
                        .filter(|patron| !patron.is_empty())
                        .collect(),
                    directivas: Vec::new(),
                    linea: numero,
                });
            }
            "match" => {
                if let Some(bloque) = bloque_actual.take() {
                    parseo.bloques.push(bloque);
                }
            }
            "include" => {
                if bloque_actual.is_some() {
                    if let Some(bloque) = bloque_actual.as_mut() {
                        bloque.directivas.push(Directiva {
                            nombre: "Include".to_string(),
                            valor: valor.to_string(),
                            linea: numero,
                        });
                    }
                    continue;
                }
                if profundidad == 0 {
                    parseo.avisos.push(format!(
                        "línea {numero}: Include anidado ignorado (solo un nivel)"
                    ));
                    continue;
                }
                for destino in valor.split_whitespace() {
                    let destino = destino.trim_matches('"');
                    if destino.contains('*') || destino.contains('?') {
                        parseo.avisos.push(format!(
                            "línea {numero}: Include con comodines ignorado: {destino}"
                        ));
                        continue;
                    }
                    let ruta_destino = resolver_include(destino, hogar, &ruta);
                    if ruta_destino
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n == "magi_config")
                    {
                        continue;
                    }
                    if !ruta_destino.exists() {
                        parseo.avisos.push(format!(
                            "línea {numero}: no existe el Include {}",
                            ruta_destino.display()
                        ));
                        continue;
                    }
                    if let Err(error) = leer_fichero(&ruta_destino, hogar, 0, visitados, parseo) {
                        parseo
                            .avisos
                            .push(format!("línea {numero}: Include no legible: {error}"));
                    }
                }
            }
            _ => {
                if let Some(bloque) = bloque_actual.as_mut() {
                    bloque.directivas.push(Directiva {
                        nombre: clave.to_string(),
                        valor: valor.to_string(),
                        linea: numero,
                    });
                }
            }
        }
    }
    if let Some(bloque) = bloque_actual.take() {
        parseo.bloques.push(bloque);
    }
    Ok(())
}

fn resolver_include(destino: &str, hogar: &Path, ruta_origen: &Path) -> PathBuf {
    if let Some(resto) = destino.strip_prefix("~/") {
        return hogar.join(resto);
    }
    let ruta = PathBuf::from(destino);
    if ruta.is_absolute() {
        ruta
    } else {
        ruta_origen
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(ruta)
    }
}

/// Elimina comentarios fuera de comillas.
fn limpiar_linea(linea: &str) -> String {
    let mut resultado = String::with_capacity(linea.len());
    let mut comilla: Option<char> = None;
    for caracter in linea.chars() {
        match comilla {
            Some(actual) if caracter == actual => {
                comilla = None;
                resultado.push(caracter);
            }
            Some(_) => resultado.push(caracter),
            None => {
                if caracter == '#' {
                    break;
                }
                if caracter == '"' || caracter == '\'' {
                    comilla = Some(caracter);
                }
                resultado.push(caracter);
            }
        }
    }
    resultado
}

/// Separa la directiva del valor: `HostName ejemplo.com` → `("HostName",
/// "ejemplo.com")`.
fn partir_directiva(linea: &str) -> (&str, &str) {
    match linea.split_once(char::is_whitespace) {
        Some((clave, valor)) => (clave, valor.trim()),
        None => (linea, ""),
    }
}

/// Quita las comillas envolventes de un valor si las tiene.
pub fn sin_comillas(valor: &str) -> &str {
    let valor = valor.trim();
    if valor.len() >= 2 {
        let primera = valor.chars().next().unwrap_or(' ');
        let ultima = valor.chars().last().unwrap_or(' ');
        if (primera == '"' && ultima == '"') || (primera == '\'' && ultima == '\'') {
            return &valor[1..valor.len() - 1];
        }
    }
    valor
}
