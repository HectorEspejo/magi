use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;

use super::parser::{self, BloqueHost, Parseo};
use crate::almacen::{grupos, hosts, tuneles};
use crate::modelo::{DatosHost, DatosTunel, Host, IdentidadRef, Origen};

pub const GRUPO_IMPORTADO: &str = "~/.ssh/config";

#[derive(Debug, Clone)]
pub struct Candidato {
    pub nombre: String,
    pub datos: DatosHost,
    pub proxyjump: Option<String>,
    /// Reenvíos del bloque (`LocalForward`, `RemoteForward`,
    /// `DynamicForward`), todavía sin `host_id`: lo fija `aplicar` al crear
    /// las filas de `TUNELES`.
    pub tuneles: Vec<ReenvioImportado>,
}

/// Un reenvío de un bloque de ssh_config con la línea de la que salió: si no
/// se puede convertir en túnel, la línea vuelve a `opciones_extra` tal cual.
#[derive(Debug, Clone)]
pub struct ReenvioImportado {
    pub linea: String,
    pub datos: DatosTunel,
}

#[derive(Debug, Clone)]
pub struct Omitido {
    pub descripcion: String,
    pub motivo: String,
}

#[derive(Debug, Clone, Default)]
pub struct Analisis {
    pub candidatos: Vec<Candidato>,
    pub omitidos: Vec<Omitido>,
    pub avisos: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Resumen {
    pub importados: usize,
    pub sobrescritos: usize,
    pub omitidos: Vec<Omitido>,
    /// Reenvíos que no se pudieron convertir en túnel y se han quedado en
    /// `opciones_extra`, para que la importación lo diga en vez de callarse.
    pub avisos: Vec<String>,
}

pub fn analizar_fichero(ruta: &Path, hogar: &Path) -> Result<Analisis> {
    let parseo = parser::leer_config(ruta, hogar)?;
    Ok(analizar(&parseo))
}

pub fn analizar(parseo: &Parseo) -> Analisis {
    let mut analisis = Analisis {
        avisos: parseo.avisos.clone(),
        ..Analisis::default()
    };
    for bloque in &parseo.bloques {
        if bloque.patrones.len() != 1 {
            analisis.omitidos.push(Omitido {
                descripcion: format!("línea {}: Host {}", bloque.linea, bloque.patrones.join(" ")),
                motivo: "el bloque Host tiene varios patrones".to_string(),
            });
            continue;
        }
        let patron = &bloque.patrones[0];
        if patron.contains('*') || patron.contains('?') {
            analisis.omitidos.push(Omitido {
                descripcion: format!("línea {}: Host {patron}", bloque.linea),
                motivo: "patrón con comodines".to_string(),
            });
            continue;
        }
        match mapear(patron, bloque) {
            Ok(candidato) => analisis.candidatos.push(candidato),
            Err((descripcion, motivo)) => analisis.omitidos.push(Omitido {
                descripcion,
                motivo,
            }),
        }
    }
    analisis
}

fn mapear(nombre: &str, bloque: &BloqueHost) -> Result<Candidato, (String, String)> {
    let mut datos = DatosHost {
        nombre: nombre.to_string(),
        ..DatosHost::default()
    };
    datos.keepalive_seg = None;
    let mut proxyjump = None;
    let mut extras: Vec<String> = Vec::new();
    let mut reenvios: Vec<ReenvioImportado> = Vec::new();
    let mut identityfile_visto = false;
    for directiva in &bloque.directivas {
        let valor = parser::sin_comillas(&directiva.valor);
        let literal = format!("{} {}", directiva.nombre, directiva.valor);
        match directiva.nombre.to_lowercase().as_str() {
            "hostname" => {
                if !valor.is_empty() {
                    datos.direccion = valor.to_string();
                }
            }
            "port" => match valor.parse::<u16>() {
                Ok(puerto) if puerto > 0 => datos.puerto = puerto,
                _ => extras.push(literal),
            },
            "user" => datos.usuario = Some(valor.to_string()),
            "identityfile" => {
                if identityfile_visto {
                    extras.push(literal);
                } else {
                    datos.identidad_ref = IdentidadRef::Fichero(valor.to_string());
                    identityfile_visto = true;
                }
            }
            "proxyjump" => {
                if proxyjump.is_none() {
                    proxyjump = Some(valor.to_string());
                } else {
                    extras.push(literal);
                }
            }
            "controlmaster" => {
                datos.multiplexar =
                    !matches!(valor.to_lowercase().as_str(), "no" | "none" | "false");
            }
            "controlpath" => {
                // El ControlPath canónico lo genera MAGI al exportar; se
                // consume para que la ida y vuelta sea estable. Un valor
                // propio del usuario se conserva como opción extra.
                if valor != "~/.ssh/cm-%r@%h:%p" {
                    extras.push(literal);
                }
            }
            "controlpersist" => {
                if !valor.eq_ignore_ascii_case("10m") {
                    extras.push(literal);
                }
            }
            "serveraliveinterval" => match valor.parse::<u32>() {
                Ok(segundos) if (5..=600).contains(&segundos) => {
                    datos.keepalive_seg = Some(segundos)
                }
                _ => extras.push(literal),
            },
            "localforward" | "remoteforward" | "dynamicforward" => {
                // Los reenvíos van a `TUNELES`, no a `opciones_extra`; una
                // forma que no se entienda se conserva como estaba.
                match super::tuneles::desde_directiva(directiva) {
                    Some(datos) => reenvios.push(ReenvioImportado {
                        linea: literal,
                        datos,
                    }),
                    None => extras.push(literal),
                }
            }
            _ => extras.push(literal),
        }
    }
    datos.opciones_extra = extras.join("\n");
    if datos.direccion.is_empty() {
        datos.direccion = nombre.to_string();
    }
    if let Some(usuario) = &datos.usuario {
        if usuario.is_empty() {
            datos.usuario = None;
        }
    }
    Ok(Candidato {
        nombre: nombre.to_string(),
        datos,
        proxyjump,
        tuneles: reenvios,
    })
}

/// Nombres de los candidatos que chocan con hosts existentes.
pub fn conflictos(conexion: &Connection, analisis: &Analisis) -> Result<Vec<String>> {
    let mut nombres = Vec::new();
    for candidato in &analisis.candidatos {
        if hosts::existe_nombre(conexion, &candidato.nombre, None)? {
            nombres.push(candidato.nombre.clone());
        }
    }
    Ok(nombres)
}

/// Aplica la importación. `decisiones` indica, por nombre en conflicto, si se
/// sobrescribe (`true`) u omite (`false`); los candidatos sin conflicto se
/// importan siempre.
pub fn aplicar(
    conexion: &Connection,
    analisis: &Analisis,
    decisiones: &HashMap<String, bool>,
) -> Result<Resumen> {
    let mut resumen = Resumen::default();
    resumen.omitidos.extend(analisis.omitidos.iter().cloned());
    let a_aplicar: Vec<&Candidato> = analisis
        .candidatos
        .iter()
        .filter(|candidato| !matches!(decisiones.get(&candidato.nombre), Some(false)))
        .collect();
    for candidato in &analisis.candidatos {
        if matches!(decisiones.get(&candidato.nombre), Some(false)) {
            resumen.omitidos.push(Omitido {
                descripcion: candidato.nombre.clone(),
                motivo: "conflicto con un host existente".to_string(),
            });
        }
    }
    if a_aplicar.is_empty() {
        return Ok(resumen);
    }
    let grupo_id = match grupos::por_nombre(conexion, GRUPO_IMPORTADO)? {
        Some(grupo) => grupo.id,
        None => grupos::crear(conexion, GRUPO_IMPORTADO)?,
    };
    let mut aplicados: Vec<(i64, &Candidato)> = Vec::new();
    for candidato in a_aplicar {
        let mut datos = candidato.datos.clone();
        datos.grupo_id = Some(grupo_id);
        match hosts::por_nombre(conexion, &candidato.nombre)? {
            Some(existente) => {
                datos.etiquetas = existente.etiquetas.clone();
                datos.servicios = existente.servicios.clone();
                hosts::actualizar(conexion, existente.id, &datos)?;
                hosts::marcar_origen(conexion, existente.id, Origen::SshConfig)?;
                resumen.sobrescritos += 1;
                aplicados.push((existente.id, candidato));
            }
            None => {
                let id = hosts::crear(conexion, &datos, Origen::SshConfig)?;
                resumen.importados += 1;
                aplicados.push((id, candidato));
            }
        }
    }
    for (id, candidato) in &aplicados {
        let Some(bruto) = &candidato.proxyjump else {
            continue;
        };
        match resolver_proxyjump(conexion, bruto, *id)? {
            Some(salto_id) => hosts::fijar_salto(conexion, *id, Some(salto_id))?,
            None => hosts::anadir_opciones_extra(conexion, *id, &format!("ProxyJump {bruto}"))?,
        }
    }
    // Los reenvíos del bloque, ya con el host creado o actualizado. Un túnel
    // que no se pueda crear no tumba la importación del host, pero **la línea
    // no se pierde**: vuelve a `opciones_extra` tal cual venía, que es donde
    // estaba antes de esta fase. Reimportar el mismo fichero, en cambio, no
    // duplica nada: si el túnel ya está idéntico, se da por hecho.
    for (id, candidato) in &aplicados {
        for reenvio in &candidato.tuneles {
            let datos = DatosTunel {
                host_id: *id,
                ..reenvio.datos.clone()
            };
            let ya_esta = tuneles::por_nombre(conexion, *id, &datos.nombre)?.is_some_and(|tunel| {
                tunel.tipo == datos.tipo
                    && tunel.escucha == tuneles::canonica(&datos.escucha).unwrap_or_default()
                    && tunel.destino == datos.destino
            });
            if ya_esta {
                continue;
            }
            if let Err(error) = tuneles::crear(conexion, &datos) {
                tracing::warn!(
                    "importación: no se pudo crear el túnel «{}» de {}: {error}; la línea se queda en opciones extra",
                    datos.nombre,
                    candidato.nombre
                );
                hosts::anadir_opciones_extra(conexion, *id, &reenvio.linea)?;
                resumen.avisos.push(format!(
                    "{}: «{}» se queda en opciones extra ({error})",
                    candidato.nombre, reenvio.linea
                ));
            }
        }
    }
    Ok(resumen)
}

/// Resuelve `ProxyJump` por nombre. Las formas con usuario, puerto o varios
/// saltos se dejan como directiva literal para no alterar su significado.
fn resolver_proxyjump(conexion: &Connection, bruto: &str, host_id: i64) -> Result<Option<i64>> {
    let bruto = bruto.trim();
    if bruto.is_empty() || bruto.contains(',') || bruto.contains('@') || bruto.contains(':') {
        return Ok(None);
    }
    let Some(destino) = hosts::por_nombre(conexion, bruto)? else {
        return Ok(None);
    };
    if destino.id == host_id {
        return Ok(None);
    }
    let mapa: HashMap<i64, Host> = hosts::listar(conexion)?
        .into_iter()
        .map(|host| (host.id, host))
        .collect();
    if crate::modelo::validar_salto(Some(host_id), Some(destino.id), &mapa).is_err() {
        return Ok(None);
    }
    Ok(Some(destino.id))
}
