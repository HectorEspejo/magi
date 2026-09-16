use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::Connection;

use crate::almacen::{grupos, hosts, tuneles};
use crate::ficheros::{copia_con_fecha, escribir_atomico};
use crate::modelo::{Host, Tunel};

#[derive(Debug, Clone)]
pub struct Resultado {
    pub ruta: PathBuf,
    pub hosts: usize,
    pub include_presente: bool,
}

/// Genera y escribe `~/.ssh/magi_config` de forma atómica con permisos 600.
pub fn exportar(conexion: &Connection, dir_ssh: &Path) -> Result<Resultado> {
    if !dir_ssh.exists() {
        anyhow::bail!(
            "no existe el directorio {}; no se puede exportar",
            dir_ssh.display()
        );
    }
    let hosts = hosts::listar(conexion)?;
    let grupos = grupos::listar(conexion)?;
    let tuneles = tuneles::por_host(conexion)?;
    let texto = generar_texto(&hosts, &grupos, &tuneles);
    let ruta = dir_ssh.join("magi_config");
    escribir_atomico(&ruta, texto.as_bytes(), 0o600)?;
    let include_presente = comprobar_include(&dir_ssh.join("config"), &ruta);
    Ok(Resultado {
        ruta,
        hosts: hosts.len(),
        include_presente,
    })
}

pub fn generar_texto(
    hosts: &[Host],
    grupos: &[crate::modelo::Grupo],
    tuneles: &HashMap<i64, Vec<Tunel>>,
) -> String {
    let orden: HashMap<i64, i64> = grupos.iter().map(|grupo| (grupo.id, grupo.orden)).collect();
    let mut ordenados = hosts.to_vec();
    ordenados.sort_by_key(|host| {
        let grupo = host
            .grupo_id
            .and_then(|id| orden.get(&id).copied())
            .unwrap_or(i64::MAX);
        (grupo, host.nombre.to_lowercase())
    });
    let ordenados = con_saltos_primero(&ordenados);

    let mut texto = String::new();
    texto.push_str(&format!(
        "# Generado por MAGI el {} — no editar a mano; usa `magi`\n\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M")
    ));
    let mut grupo_actual: Option<Option<String>> = None;
    for host in &ordenados {
        let grupo = host.grupo_nombre.clone();
        if grupo_actual.as_ref() != Some(&grupo) {
            match &grupo {
                Some(nombre) => texto.push_str(&format!("# Grupo: {nombre}\n")),
                None => texto.push_str("# Sin grupo\n"),
            }
            grupo_actual = Some(grupo);
        }
        texto.push_str(&bloque(
            host,
            tuneles.get(&host.id).map(Vec::as_slice).unwrap_or_default(),
        ));
        texto.push('\n');
    }
    texto
}

fn bloque(host: &Host, tuneles: &[Tunel]) -> String {
    let mut lineas = vec![format!("Host {}", host.nombre)];
    lineas.push(format!("    HostName {}", host.direccion));
    lineas.push(format!("    Port {}", host.puerto));
    if let Some(usuario) = &host.usuario {
        if !usuario.is_empty() {
            lineas.push(format!("    User {usuario}"));
        }
    }
    if let crate::modelo::IdentidadRef::Fichero(ruta) = &host.identidad_ref {
        lineas.push(format!("    IdentityFile {ruta}"));
    }
    if let Some(salto) = &host.salto_nombre {
        lineas.push(format!("    ProxyJump {salto}"));
    }
    if host.multiplexar {
        lineas.push("    ControlMaster auto".to_string());
        lineas.push("    ControlPath ~/.ssh/cm-%r@%h:%p".to_string());
        lineas.push("    ControlPersist 10m".to_string());
    }
    if let Some(segundos) = host.keepalive_seg {
        lineas.push(format!("    ServerAliveInterval {segundos}"));
    }
    // Los reenvíos, por nombre (la lista ya viene ordenada), antes de las
    // opciones libres: así `ssh <host>` fuera de MAGI los sigue teniendo. Lo
    // que `ssh` no sabría leer (un local con puerto 0, por ejemplo) se deja
    // comentado: decirlo es mejor que perdérselo, y una línea inválida aquí
    // rompería el `ssh` del usuario para todos los hosts.
    for tunel in tuneles {
        match super::tuneles::a_directiva(tunel) {
            Some(directiva) => lineas.push(format!("    {directiva}")),
            None => lineas.push(format!(
                "    # {} no se puede escribir en ssh_config (solo vale al vuelo en MAGI)",
                tunel.nombre
            )),
        }
    }
    for extra in host.opciones_extra.lines() {
        let extra = extra.trim();
        if !extra.is_empty() {
            lineas.push(format!("    {extra}"));
        }
    }
    let mut texto = lineas.join("\n");
    texto.push('\n');
    texto
}

/// Coloca cada host de salto antes que los hosts que lo usan.
fn con_saltos_primero(hosts: &[Host]) -> Vec<Host> {
    let por_id: HashMap<i64, &Host> = hosts.iter().map(|host| (host.id, host)).collect();
    let mut emitidos = HashSet::new();
    let mut salida = Vec::with_capacity(hosts.len());
    for host in hosts {
        visitar(host, &por_id, &mut emitidos, &mut salida);
    }
    salida
}

fn visitar(
    host: &Host,
    por_id: &HashMap<i64, &Host>,
    emitidos: &mut HashSet<i64>,
    salida: &mut Vec<Host>,
) {
    if !emitidos.insert(host.id) {
        return;
    }
    if let Some(salto_id) = host.salto_host_id {
        if let Some(salto) = por_id.get(&salto_id) {
            visitar(salto, por_id, emitidos, salida);
        }
    }
    salida.push(host.clone());
}

/// Comprueba si `~/.ssh/config` incluye el `magi_config`.
pub fn comprobar_include(ruta_config: &Path, magi_config: &Path) -> bool {
    let Ok(texto) = fs::read_to_string(ruta_config) else {
        return false;
    };
    texto.lines().any(|linea| {
        let limpia = linea.split('#').next().unwrap_or_default().trim();
        let (clave, valor) = match limpia.split_once(char::is_whitespace) {
            Some((clave, valor)) => (clave, valor.trim()),
            None => (limpia, ""),
        };
        clave.eq_ignore_ascii_case("include")
            && valor
                .split_whitespace()
                .any(|destino| es_magi_config(destino.trim_matches('"'), magi_config))
    })
}

fn es_magi_config(destino: &str, magi_config: &Path) -> bool {
    let ruta = Path::new(destino);
    if ruta.file_name().and_then(|n| n.to_str()) != Some("magi_config") {
        return false;
    }
    if destino.starts_with("~/") {
        return true;
    }
    ruta == magi_config || magi_config.ends_with(ruta)
}

/// Inserta `Include ~/.ssh/magi_config` en la primera línea de `~/.ssh/config`
/// previa copia de seguridad con fecha.
pub fn insertar_include(ruta_config: &Path, magi_config: &Path) -> Result<PathBuf> {
    let Some(copia) = copia_con_fecha(ruta_config)? else {
        anyhow::bail!("no existe {}", ruta_config.display());
    };
    let contenido = fs::read_to_string(ruta_config)
        .with_context(|| format!("leyendo {}", ruta_config.display()))?;
    let referencia = referencia_include(magi_config);
    let texto = format!("{referencia}\n{contenido}");
    escribir_atomico(ruta_config, texto.as_bytes(), 0o600)?;
    Ok(copia)
}

fn referencia_include(magi_config: &Path) -> String {
    let es_ssh = magi_config
        .parent()
        .and_then(|padre| padre.file_name())
        .is_some_and(|nombre| nombre == ".ssh");
    if es_ssh {
        "Include ~/.ssh/magi_config".to_string()
    } else {
        format!("Include {}", magi_config.display())
    }
}
