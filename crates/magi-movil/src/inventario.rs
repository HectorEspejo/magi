//! Inventario del móvil: consultas de solo lectura para Hosts y la ficha,
//! más el plegado de grupos (campo propio del teléfono).

use magi_nucleo::almacen::movil as movil_bd;
use magi_nucleo::almacen::versiones::Tabla;
use magi_nucleo::conexion::huellas::entradas_de_host;
use magi_nucleo::modelo::{normalizar_busqueda, UltimoEstado};

use crate::api::EstadoSesion;
use crate::api::{
    ErrorNucleo, EstadoHost, FichaHost, GrupoInventario, HostInventario, HuellaHost, TipoHostMovil,
};
use crate::estado::{con_almacen, EstadoNucleo};

/// Traduce el tipo de host del núcleo al DTO de UniFFI (la app no ve el tipo
/// del núcleo directamente).
fn tipo_a_vista(tipo: magi_nucleo::escritorio::TipoHost) -> TipoHostMovil {
    use magi_nucleo::escritorio::TipoHost;
    match tipo {
        TipoHost::Ssh => TipoHostMovil::Ssh,
        TipoHost::Rdp => TipoHostMovil::Rdp,
        TipoHost::Vnc => TipoHostMovil::Vnc,
        TipoHost::Voxl => TipoHostMovil::Voxl,
    }
}

/// Grupos del inventario con su plegado y el número de hosts.
pub fn grupos(estado: &EstadoNucleo) -> Result<Vec<GrupoInventario>, ErrorNucleo> {
    let grupos = con_almacen(estado, |almacen| movil_bd::grupos(almacen.conexion()))
        .map_err(|error| ErrorNucleo::General {
            detalle: error.to_string(),
        })?
        .map_err(|error| ErrorNucleo::General {
            detalle: format!("leyendo los grupos: {error:#}"),
        })?;
    Ok(grupos
        .into_iter()
        .map(|grupo| GrupoInventario {
            uuid: grupo.uuid,
            nombre: grupo.nombre,
            num_hosts: grupo.hosts.min(i64::from(u32::MAX)) as u32,
            plegado: grupo.plegado,
        })
        .collect())
}

/// Hosts con estado, sesiones abiertas y etiquetas; filtro por subcadena.
pub fn hosts(
    estado: &EstadoNucleo,
    filtro: Option<&str>,
) -> Result<Vec<HostInventario>, ErrorNucleo> {
    let hosts = con_almacen(estado, |almacen| {
        let hosts = movil_bd::hosts(almacen.conexion())?;
        let grupos = movil_bd::uuid_por_id(almacen.conexion(), Tabla::Grupos)?;
        Ok::<_, anyhow::Error>((hosts, grupos))
    })
    .map_err(|error| ErrorNucleo::General {
        detalle: error.to_string(),
    })?
    .map_err(|error| ErrorNucleo::General {
        detalle: format!("leyendo los hosts: {error:#}"),
    })?;
    let (hosts, grupos) = hosts;
    let consulta = filtro.map(normalizar_busqueda);
    let errores = estado
        .ultimo_error
        .lock()
        .map(|errores| errores.clone())
        .unwrap_or_default();
    let mut filas = Vec::new();
    for fila in hosts {
        let host = fila.host;
        if let Some(consulta) = &consulta {
            if !consulta.is_empty() {
                let coincide = [
                    normalizar_busqueda(&host.nombre),
                    normalizar_busqueda(&host.direccion),
                    normalizar_busqueda(host.usuario.as_deref().unwrap_or_default()),
                    host.grupo_nombre
                        .as_deref()
                        .map(normalizar_busqueda)
                        .unwrap_or_default(),
                    host.etiquetas
                        .iter()
                        .map(|etiqueta| normalizar_busqueda(etiqueta))
                        .collect::<Vec<_>>()
                        .join(" "),
                ]
                .iter()
                .any(|texto| texto.contains(consulta.as_str()));
                if !coincide {
                    continue;
                }
            }
        }
        let uuid_host = fila.uuid.clone();
        let tipo = tipo_a_vista(host.tipo);
        let (abiertas, abriendo, _caidas) = estado.recuento_sesiones(&uuid_host);
        let estado_host = if abriendas(&estado.sesiones, &fila.uuid) {
            EstadoHost::Conectando
        } else if abiertas > 0 {
            EstadoHost::ConSesiones
        } else if host.ultimo_estado == Some(UltimoEstado::Error) {
            EstadoHost::Error
        } else {
            EstadoHost::SinSesion
        };
        let _ = abriendo;
        filas.push(HostInventario {
            uuid: uuid_host.clone(),
            nombre: host.nombre,
            direccion: host.direccion,
            puerto: u32::from(host.puerto),
            tipo,
            usuario: host.usuario.unwrap_or_default(),
            grupo_uuid: host.grupo_id.and_then(|id| grupos.get(&id).cloned()),
            etiquetas: host.etiquetas,
            estado: estado_host,
            sesiones_abiertas: abiertas as u32,
            ultimo_error: errores.get(&uuid_host).cloned(),
            ultima_conexion_en: host.ultima_conexion_en.as_deref().and_then(fecha_a_epoca),
        });
    }
    Ok(filas)
}

/// Ficha de solo lectura de un host, con sus huellas y si hay contraseña.
pub fn ficha(estado: &EstadoNucleo, host_uuid: &str) -> Result<FichaHost, ErrorNucleo> {
    let host_uuid_buscado = host_uuid.to_string();
    let obtenido = con_almacen(estado, move |almacen| {
        let conexion = almacen.conexion();
        let Some(host) = movil_bd::host_por_uuid(conexion, &host_uuid_buscado)? else {
            return Ok::<_, anyhow::Error>(None);
        };
        let uuids = movil_bd::uuid_por_id(conexion, Tabla::Hosts)?;
        let salto_uuid = host.salto_host_id.and_then(|id| uuids.get(&id).cloned());
        let grupos = movil_bd::uuid_por_id(conexion, Tabla::Grupos)?;
        let grupo_uuid = host.grupo_id.and_then(|id| grupos.get(&id).cloned());
        Ok::<_, anyhow::Error>(Some((host, salto_uuid, grupo_uuid)))
    })
    .map_err(|error| ErrorNucleo::General {
        detalle: error.to_string(),
    })?
    .map_err(|error| ErrorNucleo::General {
        detalle: format!("leyendo la ficha: {error:#}"),
    })?;
    let Some((host, salto_uuid, grupo_uuid)) = obtenido else {
        return Err(ErrorNucleo::NoEncontrado {
            detalle: format!("el host {host_uuid}"),
        });
    };
    let huellas = entradas_de_host(
        &estado.rutas.fichero_known_hosts(),
        &host.direccion,
        host.puerto,
    )
    .unwrap_or_default()
    .into_iter()
    .map(|(tipo, sha256)| HuellaHost { tipo, sha256 })
    .collect();
    let tiene_contrasena = estado
        .llavero
        .tiene_contrasena(&host.nombre)
        .unwrap_or(false);
    Ok(FichaHost {
        uuid: host_uuid.to_string(),
        nombre: host.nombre.clone(),
        direccion: host.direccion,
        puerto: u32::from(host.puerto),
        tipo: tipo_a_vista(host.tipo),
        usuario: host.usuario.unwrap_or_default(),
        grupo_uuid,
        grupo_nombre: host.grupo_nombre.clone(),
        etiquetas: host.etiquetas.clone(),
        salto_uuid,
        salto_nombre: host.salto_nombre.clone(),
        keepalive_segundos: host.keepalive_seg,
        identidad_escritorio: host
            .identidad_ref
            .a_bd()
            .or_else(|| Some("auto".to_string())),
        huellas,
        tiene_contrasena,
    })
}

/// Guarda el plegado de un grupo (campo propio del móvil).
pub fn plegar(estado: &EstadoNucleo, uuid: &str, plegado: bool) -> Result<(), ErrorNucleo> {
    let uuid_cierre = uuid.to_string();
    let encontrado = con_almacen(estado, move |almacen| {
        movil_bd::plegar_grupo(almacen.conexion(), &uuid_cierre, plegado)
    })
    .map_err(|error| ErrorNucleo::General {
        detalle: error.to_string(),
    })?
    .map_err(|error| ErrorNucleo::General {
        detalle: format!("plegando el grupo: {error:#}"),
    })?;
    if !encontrado {
        return Err(ErrorNucleo::NoEncontrado {
            detalle: format!("el grupo {uuid}"),
        });
    }
    Ok(())
}

/// ¿Hay alguna sesión abriendo en ese host?
fn abriendas(
    sesiones: &std::sync::Mutex<
        std::collections::HashMap<u64, std::sync::Arc<crate::estado::SesionViva>>,
    >,
    host_uuid: &str,
) -> bool {
    sesiones
        .lock()
        .map(|sesiones| {
            sesiones.values().any(|sesion| {
                sesion.host_uuid == host_uuid && sesion.estado() == EstadoSesion::Abriendo
            })
        })
        .unwrap_or(false)
}

/// Convierte una fecha ISO de magi.db en segundos desde la época.
fn fecha_a_epoca(fecha: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(fecha)
        .ok()
        .map(|fecha| fecha.timestamp())
}
