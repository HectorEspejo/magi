//! Snippets del móvil (informe §5.1, §6.8 y §7.6): la lista y el detalle de los
//! snippets sincronizados, con sus destinos resueltos y sus variables, y la
//! preparación de una ejecución (variables escapadas, errores por campo y
//! motivos de deliberación). El móvil es lector: nunca crea ni edita snippets.
//!
//! La preparación se ofrece de dos formas: `preparar_ejecucion`, que devuelve
//! el DTO de la API, y `preparar_interno`, que además entrega al motor de
//! ejecuciones el id del snippet y los hosts resueltos con su uuid.

use std::collections::{BTreeMap, HashMap, HashSet};

use magi_nucleo::almacen::movil as movil_bd;
use magi_nucleo::almacen::versiones::Tabla;
use magi_nucleo::almacen::{
    grupos as grupos_bd, snippets as snippets_bd, verificaciones as verificaciones_bd, Almacen,
};
use magi_nucleo::deliberacion::Verificaciones;
use magi_nucleo::modelo::{Grupo, Host};
use magi_nucleo::snippets::variables::{self as variables_snippet, Variable};
use magi_nucleo::snippets::{self as nucleo_snippets, Destino, Snippet};

use crate::api::{
    DestinoMovil, ErrorCampoMovil, ErrorNucleo, HostDestinoMovil, PreparacionEjecucion,
    PropiedadMovil, SnippetMovil, VariableSnippetMovil,
};
use crate::estado::{con_almacen, EstadoNucleo};

// ===========================================================================
// Snippets: inventario y preparación (informe §5.1, §6.8 y §7.6)
// ===========================================================================

/// Lo que hace falta para listar y preparar snippets, leído de una vez de
/// `SNIPPETS`, `SNIPPET_DESTINOS`, `HOSTS` y `GRUPOS`. El móvil es lector: no
/// crea ni edita snippets.
struct InventarioSnippets {
    snippets: Vec<Snippet>,
    /// uuid de sincronización de cada snippet, por id.
    uuids_snippets: HashMap<i64, String>,
    hosts: Vec<Host>,
    /// uuid de sincronización de cada host, por id.
    uuids_hosts: HashMap<i64, String>,
    grupos: Vec<Grupo>,
}

fn leer_inventario(almacen: &Almacen) -> anyhow::Result<InventarioSnippets> {
    let conexion = almacen.conexion();
    let snippets = snippets_bd::listar(conexion)?;
    let uuids_snippets = movil_bd::uuid_por_id(conexion, Tabla::Snippets)?;
    let hosts_inventario = movil_bd::hosts(conexion)?;
    let mut hosts = Vec::with_capacity(hosts_inventario.len());
    let mut uuids_hosts = HashMap::with_capacity(hosts_inventario.len());
    for fila in hosts_inventario {
        uuids_hosts.insert(fila.host.id, fila.uuid);
        hosts.push(fila.host);
    }
    let grupos = grupos_bd::listar(conexion)?;
    Ok(InventarioSnippets {
        snippets,
        uuids_snippets,
        hosts,
        uuids_hosts,
        grupos,
    })
}

/// Un host resuelto, ya con el uuid que usa la app.
fn host_destino(host: &Host, uuids: &HashMap<i64, String>) -> Option<HostDestinoMovil> {
    let uuid = uuids.get(&host.id)?;
    if uuid.is_empty() {
        return None;
    }
    Some(HostDestinoMovil {
        uuid: uuid.clone(),
        nombre: host.nombre.clone(),
    })
}

/// Convierte un snippet del núcleo al DTO de la puerta, con sus destinos y sus
/// hosts ya resueltos y sus variables detectadas.
fn a_snippet_movil(uuid: &str, snippet: &Snippet, inventario: &InventarioSnippets) -> SnippetMovil {
    let destinos = snippet
        .destinos
        .iter()
        .filter_map(|destino| match destino {
            Destino::Etiqueta(etiqueta) => Some(DestinoMovil::Etiqueta(etiqueta.clone())),
            Destino::Host { id, nombre } => {
                let uuid = inventario.uuids_hosts.get(id)?;
                if uuid.is_empty() {
                    return None;
                }
                Some(DestinoMovil::Host(HostDestinoMovil {
                    uuid: uuid.clone(),
                    nombre: nombre.clone(),
                }))
            }
        })
        .collect();
    let hosts = nucleo_snippets::resolver(&snippet.destinos, &inventario.hosts, &inventario.grupos)
        .iter()
        .filter_map(|host| host_destino(host, &inventario.uuids_hosts))
        .collect();
    let variables = variables_snippet::detectar(&snippet.comando)
        .into_iter()
        .map(|variable| VariableSnippetMovil {
            nombre: variable.nombre,
            valor_defecto: variable.defecto,
            valor: None,
        })
        .collect();
    SnippetMovil {
        uuid: uuid.to_string(),
        nombre: snippet.nombre.clone(),
        comando: snippet.comando.clone(),
        descripcion: snippet.descripcion.clone(),
        etiquetas: snippet.etiquetas.clone(),
        critico: snippet.critico,
        timeout_seg: snippet.timeout_seg,
        parar_al_fallo: snippet.parar_al_fallo,
        usado_veces: u64::try_from(snippet.usado_veces).unwrap_or(0),
        ultimo_uso_en: snippet.ultimo_uso_en.as_deref().and_then(epoca),
        destinos,
        hosts,
        variables,
        escritorios_omitidos: u32::try_from(nucleo_snippets::escritorios_omitidos(
            &snippet.destinos,
            &inventario.hosts,
        ))
        .unwrap_or(u32::MAX),
    }
}

/// Snippets del inventario, filtrados por texto y, si se pide, por host.
fn filtrar_snippets(
    inventario: &InventarioSnippets,
    filtro: Option<&str>,
    solo_host: Option<i64>,
) -> Vec<SnippetMovil> {
    let consulta = filtro.map(str::trim).filter(|texto| !texto.is_empty());
    let mut lista = Vec::new();
    for snippet in &inventario.snippets {
        if let Some(host_id) = solo_host {
            if !nucleo_snippets::apunta_a(
                &snippet.destinos,
                &inventario.hosts,
                &inventario.grupos,
                host_id,
            ) {
                continue;
            }
        }
        if let Some(consulta) = consulta {
            if !nucleo_snippets::coincide(snippet, consulta) {
                continue;
            }
        }
        let Some(uuid) = inventario.uuids_snippets.get(&snippet.id) else {
            continue;
        };
        if uuid.is_empty() {
            continue;
        }
        lista.push(a_snippet_movil(uuid, snippet, inventario));
    }
    lista
}

/// Snippets del inventario (solo lectura), con el filtro de la vista.
pub fn snippets(
    estado: &EstadoNucleo,
    filtro: Option<&str>,
) -> Result<Vec<SnippetMovil>, ErrorNucleo> {
    let filtro = filtro.map(str::to_string);
    let resultado = con_almacen(estado, move |almacen| {
        let inventario = leer_inventario(almacen)?;
        Ok::<_, anyhow::Error>(filtrar_snippets(&inventario, filtro.as_deref(), None))
    })?;
    resultado.map_err(|error| ErrorNucleo::General {
        detalle: format!("leyendo los snippets: {error:#}"),
    })
}

/// Los snippets que tienen ese host entre sus destinos (informe §6.2).
pub fn snippets_de_host(
    estado: &EstadoNucleo,
    host_uuid: &str,
) -> Result<Vec<SnippetMovil>, ErrorNucleo> {
    let host_uuid = host_uuid.to_string();
    let uuid_buscado = host_uuid.clone();
    let resultado = con_almacen(estado, move |almacen| {
        let inventario = leer_inventario(almacen)?;
        let Some(host_id) = movil_bd::id_por_uuid(almacen.conexion(), Tabla::Hosts, &uuid_buscado)?
        else {
            return Ok::<_, anyhow::Error>(None);
        };
        Ok(Some(filtrar_snippets(&inventario, None, Some(host_id))))
    })?;
    let lista = resultado.map_err(|error| ErrorNucleo::General {
        detalle: format!("leyendo los snippets del host: {error:#}"),
    })?;
    lista.ok_or_else(|| ErrorNucleo::NoEncontrado {
        detalle: format!("el host {host_uuid}"),
    })
}

/// Detalle de un snippet por su uuid de sincronización (informe §5.1).
pub fn snippet(estado: &EstadoNucleo, uuid: &str) -> Result<SnippetMovil, ErrorNucleo> {
    let uuid_pedido = uuid.to_string();
    let resultado = con_almacen(estado, move |almacen| {
        let inventario = leer_inventario(almacen)?;
        let Some(id) = movil_bd::id_por_uuid(almacen.conexion(), Tabla::Snippets, &uuid_pedido)?
        else {
            return Ok::<_, anyhow::Error>(None);
        };
        let snippet = snippets_bd::obtener(almacen.conexion(), id)?;
        Ok(Some(a_snippet_movil(&uuid_pedido, &snippet, &inventario)))
    })?;
    let snippet = resultado.map_err(|error| ErrorNucleo::General {
        detalle: format!("leyendo el snippet: {error:#}"),
    })?;
    snippet.ok_or_else(|| ErrorNucleo::NoEncontrado {
        detalle: format!("el snippet {uuid}"),
    })
}

/// Prepara una ejecución y devuelve además lo que necesita el motor de
/// ejecuciones (id del snippet y hosts elegidos con su uuid). Uso interno del
/// móvil: la app llama a [`preparar_ejecucion`].
pub fn preparar_interno(
    estado: &EstadoNucleo,
    snippet_uuid: &str,
    hosts: &[String],
    variables: &[VariableSnippetMovil],
) -> Result<Preparada, ErrorNucleo> {
    let uuid_buscado = snippet_uuid.to_string();
    let hosts_pedidos = hosts.to_vec();
    let variables_pedidas = variables.to_vec();
    let datos = con_almacen(estado, move |almacen| {
        let inventario = leer_inventario(almacen)?;
        let Some(id) = movil_bd::id_por_uuid(almacen.conexion(), Tabla::Snippets, &uuid_buscado)?
        else {
            return Ok::<_, anyhow::Error>(None);
        };
        let snippet = snippets_bd::obtener(almacen.conexion(), id)?;
        let verificaciones = verificaciones_bd::por_host(almacen.conexion())?;
        Ok(Some((snippet, inventario, verificaciones)))
    })?;
    let datos = datos.map_err(|error| ErrorNucleo::General {
        detalle: format!("preparando la ejecución: {error:#}"),
    })?;
    let Some((snippet, inventario, verificaciones)) = datos else {
        return Err(ErrorNucleo::NoEncontrado {
            detalle: format!("el snippet {snippet_uuid}"),
        });
    };
    let (publica, hosts) = preparar_con(
        snippet_uuid,
        &snippet,
        &inventario,
        &verificaciones,
        &hosts_pedidos,
        &variables_pedidas,
    );
    Ok(Preparada {
        publica,
        snippet_id: snippet.id,
        hosts,
    })
}

/// Prepara una ejecución: resuelve destinos, sustituye las variables con
/// escape, valida y calcula los motivos de deliberación (informe §5.1 y §7.6).
pub fn preparar_ejecucion(
    estado: &EstadoNucleo,
    snippet_uuid: &str,
    hosts: &[String],
    variables: &[VariableSnippetMovil],
) -> Result<PreparacionEjecucion, ErrorNucleo> {
    Ok(preparar_interno(estado, snippet_uuid, hosts, variables)?.publica)
}

/// Una preparación ya resuelta: lo que ve la app y lo que necesita el motor
/// de ejecuciones (id del snippet y hosts elegidos con su uuid).
pub struct Preparada {
    pub publica: PreparacionEjecucion,
    pub snippet_id: i64,
    /// (host_id, nombre, uuid) en el orden de los hosts elegidos.
    pub hosts: Vec<(i64, String, String)>,
}

impl Preparada {
    pub fn hosts_uuid(&self) -> Vec<String> {
        self.hosts.iter().map(|(_, _, uuid)| uuid.clone()).collect()
    }
}

/// La preparación en sí, sin base de datos: se prueba entera en las pruebas.
#[allow(clippy::too_many_arguments)]
fn preparar_con(
    snippet_uuid: &str,
    snippet: &Snippet,
    inventario: &InventarioSnippets,
    verificaciones: &HashMap<i64, Verificaciones>,
    hosts_pedidos: &[String],
    variables: &[VariableSnippetMovil],
) -> (PreparacionEjecucion, Vec<(i64, String, String)>) {
    let mut errores = Vec::new();
    let resueltos =
        nucleo_snippets::resolver(&snippet.destinos, &inventario.hosts, &inventario.grupos);
    let mut elegidos: Vec<&Host> = Vec::new();
    if resueltos.is_empty() {
        errores.push(ErrorCampoMovil {
            campo: "snippet".to_string(),
            mensaje: "el snippet no apunta a ningún host".to_string(),
        });
    } else if hosts_pedidos.is_empty() {
        errores.push(ErrorCampoMovil {
            campo: "hosts".to_string(),
            mensaje: "elige al menos un host".to_string(),
        });
    } else {
        let pedidos: HashSet<&str> = hosts_pedidos.iter().map(String::as_str).collect();
        let conocidos: HashSet<&str> = inventario
            .uuids_hosts
            .values()
            .map(String::as_str)
            .collect();
        for uuid in &pedidos {
            if !conocidos.contains(*uuid) {
                errores.push(ErrorCampoMovil {
                    campo: "hosts".to_string(),
                    mensaje: "ese host ya no existe".to_string(),
                });
            }
        }
        for host in &resueltos {
            let Some(uuid) = inventario.uuids_hosts.get(&host.id) else {
                continue;
            };
            if pedidos.contains(uuid.as_str()) {
                elegidos.push(host);
            }
        }
        let destino: HashSet<&str> = elegidos
            .iter()
            .filter_map(|host| inventario.uuids_hosts.get(&host.id).map(String::as_str))
            .collect();
        for uuid in &pedidos {
            if conocidos.contains(*uuid) && !destino.contains(*uuid) {
                let nombre = inventario
                    .uuids_hosts
                    .iter()
                    .find(|(_, vista)| vista.as_str() == *uuid)
                    .and_then(|(id, _)| inventario.hosts.iter().find(|host| host.id == *id))
                    .map(|host| host.nombre.clone())
                    .unwrap_or_else(|| uuid.to_string());
                errores.push(ErrorCampoMovil {
                    campo: "hosts".to_string(),
                    mensaje: format!("«{nombre}» no es destino de este snippet"),
                });
            }
        }
    }

    // Variables: se sustituyen siempre escapadas y una vacía sin defecto no
    // deja continuar (informe §4.5 y §7.6).
    let detectadas = variables_snippet::detectar(&snippet.comando);
    if let Err(motivo) = variables_snippet::validar_contexto(&snippet.comando) {
        let campo = campo_de_contexto(&motivo, &detectadas);
        errores.push(ErrorCampoMovil {
            campo,
            mensaje: motivo,
        });
    }
    let proporcionadas: HashMap<&str, &VariableSnippetMovil> = variables
        .iter()
        .map(|variable| (variable.nombre.as_str(), variable))
        .collect();
    let mut valores = BTreeMap::new();
    let mut usadas = Vec::new();
    let mut lista_valores = Vec::new();
    for variable in &detectadas {
        let escrita = proporcionadas
            .get(variable.nombre.as_str())
            .and_then(|vista| vista.valor.as_deref())
            .unwrap_or("");
        match variables_snippet::valor_efectivo(variable, escrita) {
            Ok(valor) => {
                valores.insert(variable.nombre.clone(), valor.clone());
                lista_valores.push(PropiedadMovil {
                    clave: variable.nombre.clone(),
                    valor: valor.clone(),
                });
                usadas.push(VariableSnippetMovil {
                    nombre: variable.nombre.clone(),
                    valor_defecto: variable.defecto.clone(),
                    valor: Some(valor),
                });
            }
            Err(error) => {
                errores.push(ErrorCampoMovil {
                    campo: variable.nombre.clone(),
                    mensaje: error.to_string(),
                });
                usadas.push(VariableSnippetMovil {
                    nombre: variable.nombre.clone(),
                    valor_defecto: variable.defecto.clone(),
                    valor: proporcionadas
                        .get(variable.nombre.as_str())
                        .and_then(|vista| vista.valor.clone()),
                });
            }
        }
    }
    let comando = if errores.is_empty() {
        match variables_snippet::sustituir(&snippet.comando, &valores) {
            Ok(comando) => comando,
            Err(error) => {
                // No debería llegar: se ha validado antes variable a variable.
                errores.push(ErrorCampoMovil {
                    campo: "snippet".to_string(),
                    mensaje: error.to_string(),
                });
                snippet.comando.clone()
            }
        }
    } else {
        snippet.comando.clone()
    };

    let pares: Vec<(i64, String)> = elegidos
        .iter()
        .map(|host| (host.id, host.nombre.clone()))
        .collect();
    let motivos_nucleo =
        nucleo_snippets::motivos_deliberacion(snippet.critico, &pares, verificaciones);
    let motivos: Vec<String> = motivos_nucleo
        .iter()
        .map(nucleo_snippets::MotivoDeliberacion::texto)
        .collect();
    let hosts = elegidos
        .iter()
        .filter_map(|host| host_destino(host, &inventario.uuids_hosts))
        .collect();
    let resueltos_hosts: Vec<(i64, String, String)> = elegidos
        .iter()
        .filter_map(|host| {
            let uuid = inventario.uuids_hosts.get(&host.id)?.clone();
            if uuid.is_empty() {
                return None;
            }
            Some((host.id, host.nombre.clone(), uuid))
        })
        .collect();
    (
        PreparacionEjecucion {
            snippet_uuid: snippet_uuid.to_string(),
            snippet_nombre: snippet.nombre.clone(),
            comando,
            hosts,
            variables: usadas,
            errores,
            delibera: !motivos.is_empty(),
            motivos,
            critico: snippet.critico,
            timeout_seg: snippet.timeout_seg,
            parar_al_fallo: snippet.parar_al_fallo,
            valores: lista_valores,
        },
        resueltos_hosts,
    )
}

/// Campo al que culpar de un error de contexto: la variable que nombra el
/// mensaje, o el snippet entero si no se reconoce.
fn campo_de_contexto(motivo: &str, detectadas: &[Variable]) -> String {
    motivo
        .split('«')
        .nth(1)
        .and_then(|resto| resto.split('»').next())
        .filter(|nombre| detectadas.iter().any(|variable| variable.nombre == *nombre))
        .map(str::to_string)
        .unwrap_or_else(|| "snippet".to_string())
}

/// Fecha ISO de `SNIPPETS.ultimo_uso_en` en segundos desde la época.
fn epoca(fecha: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(fecha)
        .ok()
        .map(|fecha| fecha.timestamp())
}

#[cfg(test)]
mod pruebas_snippets {
    use super::*;

    use magi_nucleo::modelo::host_de_prueba;

    fn host(id: i64, nombre: &str, etiquetas: &[&str]) -> Host {
        let mut host = host_de_prueba();
        host.id = id;
        host.nombre = nombre.to_string();
        host.grupo_id = None;
        host.etiquetas = etiquetas
            .iter()
            .map(|etiqueta| etiqueta.to_string())
            .collect();
        host.tipo = magi_nucleo::escritorio::TipoHost::Ssh;
        host
    }

    fn escritorio(id: i64, nombre: &str, etiquetas: &[&str]) -> Host {
        let mut host = host(id, nombre, etiquetas);
        host.tipo = magi_nucleo::escritorio::TipoHost::Rdp;
        host
    }

    fn snippet(id: i64, comando: &str, critico: bool, destinos: Vec<Destino>) -> Snippet {
        Snippet {
            id,
            nombre: "reiniciar nginx".to_string(),
            comando: comando.to_string(),
            descripcion: String::new(),
            etiquetas: Vec::new(),
            critico,
            timeout_seg: 60,
            parar_al_fallo: false,
            usado_veces: 3,
            ultimo_uso_en: Some("2026-11-09T12:00:00+01:00".to_string()),
            creado_en: String::new(),
            actualizado_en: String::new(),
            destinos,
        }
    }

    fn inventario(hosts: Vec<Host>, snippets: Vec<Snippet>) -> InventarioSnippets {
        let mut uuids_hosts = HashMap::new();
        for host in &hosts {
            uuids_hosts.insert(host.id, format!("uuid-{}", host.nombre));
        }
        let mut uuids_snippets = HashMap::new();
        for snippet in &snippets {
            uuids_snippets.insert(snippet.id, format!("snippet-{}", snippet.id));
        }
        InventarioSnippets {
            snippets,
            uuids_snippets,
            hosts,
            uuids_hosts,
            grupos: Vec::new(),
        }
    }

    fn variable(nombre: &str, valor: Option<&str>) -> VariableSnippetMovil {
        VariableSnippetMovil {
            nombre: nombre.to_string(),
            valor_defecto: None,
            valor: valor.map(str::to_string),
        }
    }

    fn preparar(
        inventario: &InventarioSnippets,
        verificaciones: &HashMap<i64, Verificaciones>,
        hosts: &[&str],
        variables: &[VariableSnippetMovil],
    ) -> PreparacionEjecucion {
        let pedidos: Vec<String> = hosts.iter().map(|uuid| uuid.to_string()).collect();
        preparar_con(
            "snippet-7",
            &inventario.snippets[0],
            inventario,
            verificaciones,
            &pedidos,
            variables,
        )
        .0
    }

    #[test]
    fn el_snippet_del_movil_lleva_destinos_resueltos_y_variables() {
        let hosts = vec![
            host(1, "web-01", &["web"]),
            host(2, "db-01", &[]),
            escritorio(3, "rdp-01", &["web"]),
        ];
        let snippet = snippet(
            7,
            "systemctl restart {{servicio:nginx}}",
            false,
            vec![
                Destino::Etiqueta("web".to_string()),
                Destino::Host {
                    id: 2,
                    nombre: "db-01".to_string(),
                },
            ],
        );
        let inventario = inventario(hosts, vec![snippet]);
        let dto = a_snippet_movil("snippet-7", &inventario.snippets[0], &inventario);
        assert_eq!(dto.uuid, "snippet-7");
        assert_eq!(
            dto.hosts,
            vec![
                HostDestinoMovil {
                    uuid: "uuid-db-01".to_string(),
                    nombre: "db-01".to_string(),
                },
                HostDestinoMovil {
                    uuid: "uuid-web-01".to_string(),
                    nombre: "web-01".to_string(),
                },
            ]
        );
        // El escritorio remoto no ejecuta snippets: se omite y se cuenta.
        assert_eq!(dto.escritorios_omitidos, 1);
        assert_eq!(dto.variables.len(), 1);
        assert_eq!(dto.variables[0].nombre, "servicio");
        assert_eq!(dto.variables[0].valor_defecto.as_deref(), Some("nginx"));
        assert_eq!(dto.variables[0].valor, None);
        assert_eq!(dto.usado_veces, 3);
        assert_eq!(dto.ultimo_uso_en, Some(1_794_222_000));
        assert!(matches!(dto.destinos[1], DestinoMovil::Host(_)));
    }

    #[test]
    fn la_lista_filtra_por_texto_y_por_host() {
        let hosts = vec![host(1, "web-01", &["web"]), host(2, "db-01", &["db"])];
        let mut uno = snippet(
            7,
            "systemctl restart nginx",
            false,
            vec![Destino::Etiqueta("web".to_string())],
        );
        uno.nombre = "reiniciar nginx".to_string();
        let mut dos = snippet(
            8,
            "uptime",
            false,
            vec![Destino::Etiqueta("db".to_string())],
        );
        dos.nombre = "uptime".to_string();
        let inventario = inventario(hosts, vec![uno, dos]);
        assert_eq!(filtrar_snippets(&inventario, None, None).len(), 2);
        let nginx = filtrar_snippets(&inventario, Some("nginx"), None);
        assert_eq!(nginx.len(), 1);
        assert_eq!(nginx[0].uuid, "snippet-7");
        let uptime = filtrar_snippets(&inventario, Some("uptime"), None);
        assert_eq!(uptime[0].uuid, "snippet-8");
        let de_db = filtrar_snippets(&inventario, None, Some(2));
        assert_eq!(de_db.len(), 1);
        assert_eq!(de_db[0].uuid, "snippet-8");
        // Un filtro que no casa con nada y un host sin snippets.
        assert!(filtrar_snippets(&inventario, Some("postgres"), None).is_empty());
        assert!(filtrar_snippets(&inventario, None, Some(99)).is_empty());
    }

    #[test]
    fn la_preparacion_escapa_las_variables_para_el_shell() {
        let hosts = vec![host(1, "web-01", &["web"])];
        let snippet = snippet(
            7,
            "systemctl restart {{servicio}}",
            false,
            vec![Destino::Etiqueta("web".to_string())],
        );
        let inventario = inventario(hosts, vec![snippet]);
        let preparacion = preparar(
            &inventario,
            &HashMap::new(),
            &["uuid-web-01"],
            &[variable("servicio", Some("a b; rm"))],
        );
        assert!(preparacion.errores.is_empty(), "{:?}", preparacion.errores);
        assert_eq!(preparacion.comando, "systemctl restart 'a b; rm'");
        assert_eq!(preparacion.hosts.len(), 1);
        assert!(!preparacion.delibera);
        assert!(preparacion.motivos.is_empty());
        assert_eq!(
            preparacion.valores,
            vec![PropiedadMovil {
                clave: "servicio".to_string(),
                valor: "a b; rm".to_string(),
            }]
        );
        assert_eq!(preparacion.variables[0].valor.as_deref(), Some("a b; rm"));
        assert!(!preparacion.parar_al_fallo);
        assert_eq!(preparacion.timeout_seg, 60);
    }

    #[test]
    fn una_variable_vacia_sin_defecto_es_un_error_de_su_campo() {
        let hosts = vec![host(1, "web-01", &["web"])];
        let snippet = snippet(
            7,
            "systemctl restart {{servicio}}",
            false,
            vec![Destino::Etiqueta("web".to_string())],
        );
        let inventario = inventario(hosts, vec![snippet]);
        let preparacion = preparar(&inventario, &HashMap::new(), &["uuid-web-01"], &[]);
        assert_eq!(preparacion.errores.len(), 1);
        assert_eq!(preparacion.errores[0].campo, "servicio");
        assert!(preparacion.errores[0].mensaje.contains("vacía"));
        assert_eq!(preparacion.comando, "systemctl restart {{servicio}}");
    }

    #[test]
    fn una_variable_con_defecto_usa_el_defecto() {
        let hosts = vec![host(1, "web-01", &["web"])];
        let snippet = snippet(
            7,
            "journalctl --vacuum-time={{dias:7}}d",
            false,
            vec![Destino::Etiqueta("web".to_string())],
        );
        let inventario = inventario(hosts, vec![snippet]);
        let preparacion = preparar(&inventario, &HashMap::new(), &["uuid-web-01"], &[]);
        assert!(preparacion.errores.is_empty(), "{:?}", preparacion.errores);
        assert_eq!(preparacion.comando, "journalctl --vacuum-time=7d");
        assert_eq!(
            preparacion.valores,
            vec![PropiedadMovil {
                clave: "dias".to_string(),
                valor: "7".to_string(),
            }]
        );
    }

    #[test]
    fn una_variable_entre_comillas_es_un_error_en_su_campo() {
        let hosts = vec![host(1, "web-01", &["web"])];
        let snippet = snippet(
            7,
            "echo \"{{saludo}}\"",
            false,
            vec![Destino::Etiqueta("web".to_string())],
        );
        let inventario = inventario(hosts, vec![snippet]);
        let preparacion = preparar(
            &inventario,
            &HashMap::new(),
            &["uuid-web-01"],
            &[variable("saludo", Some("hola"))],
        );
        assert_eq!(preparacion.errores.len(), 1);
        assert_eq!(preparacion.errores[0].campo, "saludo");
        assert!(preparacion.errores[0].mensaje.contains("comillas"));
    }

    #[test]
    fn un_host_que_no_es_destino_es_un_error_de_hosts() {
        let hosts = vec![host(1, "web-01", &["web"]), host(2, "db-01", &["db"])];
        let snippet = snippet(
            7,
            "uptime",
            false,
            vec![Destino::Etiqueta("web".to_string())],
        );
        let inventario = inventario(hosts, vec![snippet]);
        let ajena = preparar(&inventario, &HashMap::new(), &["uuid-db-01"], &[]);
        assert!(ajena.hosts.is_empty());
        assert!(ajena
            .errores
            .iter()
            .any(|error| error.campo == "hosts" && error.mensaje.contains("no es destino")));
        let fantasma = preparar(&inventario, &HashMap::new(), &["uuid-inesperado"], &[]);
        assert!(fantasma
            .errores
            .iter()
            .any(|error| error.campo == "hosts" && error.mensaje.contains("ya no existe")));
        let sin_marcar = preparar(&inventario, &HashMap::new(), &[], &[]);
        assert!(sin_marcar
            .errores
            .iter()
            .any(|error| error.campo == "hosts" && error.mensaje.contains("al menos un host")));
    }

    #[test]
    fn sin_hosts_resueltos_es_un_error_del_snippet() {
        let hosts = vec![host(1, "web-01", &["web"])];
        let snippet = snippet(
            7,
            "uptime",
            false,
            vec![Destino::Etiqueta("nadie".to_string())],
        );
        let inventario = inventario(hosts, vec![snippet]);
        let preparacion = preparar(&inventario, &HashMap::new(), &["uuid-web-01"], &[]);
        assert!(preparacion.errores.iter().any(|error| {
            error.campo == "snippet" && error.mensaje.contains("no apunta a ningún host")
        }));
    }

    #[test]
    fn delibera_por_critico_por_varios_hosts_o_por_verificaciones() {
        let critico = snippet(
            7,
            "uptime",
            true,
            vec![Destino::Etiqueta("web".to_string())],
        );
        let con_critico = inventario(
            vec![host(1, "web-01", &["web"]), host(2, "web-02", &["web"])],
            vec![critico],
        );
        let preparacion = preparar(&con_critico, &HashMap::new(), &["uuid-web-01"], &[]);
        assert!(preparacion.delibera);
        assert_eq!(preparacion.motivos, vec!["crítico".to_string()]);

        let normal = snippet(
            7,
            "uptime",
            false,
            vec![Destino::Etiqueta("web".to_string())],
        );
        let con_dos = inventario(
            vec![host(1, "web-01", &["web"]), host(2, "web-02", &["web"])],
            vec![normal],
        );
        let ambos = preparar(
            &con_dos,
            &HashMap::new(),
            &["uuid-web-01", "uuid-web-02"],
            &[],
        );
        assert_eq!(ambos.hosts.len(), 2);
        assert_eq!(ambos.motivos, vec!["2 hosts".to_string()]);
        // Solo uno marcado: no delibera por varios hosts.
        let uno = preparar(&con_dos, &HashMap::new(), &["uuid-web-01"], &[]);
        assert!(!uno.delibera);

        let mut verificaciones = HashMap::new();
        verificaciones.insert(
            1,
            Verificaciones {
                host_id: 1,
                salud: true,
                ..Default::default()
            },
        );
        let con_verificaciones = preparar(&con_dos, &verificaciones, &["uuid-web-01"], &[]);
        assert_eq!(
            con_verificaciones.motivos,
            vec!["verificaciones en web-01".to_string()]
        );
    }
}
