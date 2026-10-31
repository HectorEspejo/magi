//! Sincronización cifrada del inventario entre equipos (Fase 10).
//!
//! Una pasada lee los ficheros magi-<uuid>.age de la carpeta elegida,
//! descifra y comprueba su firma, fusiona el inventario por versiones y
//! vectores, aplica el resultado en una transacción (con copia previa de
//! magi.db), importa las huellas y exporta el fichero de este equipo si su
//! contenido cambió. Un cerrojo garantiza que solo un proceso sincroniza a la
//! vez.
//!
//! El formato completo está en docs/formato-sincronizacion.md.

pub mod aplicacion;
pub mod carpeta;
pub mod cifrado;
pub mod elementos;
pub mod equipos;
pub mod formato;
pub mod fusion;
pub mod huellas;
pub mod qr;
pub mod reloj;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use rusqlite::Connection;
use serde::Serialize;

use crate::almacen::{conflictos, copias, equipos as equipos_bd, versiones};
use crate::config::{Config, Rutas};
use crate::modelo::ResultadoRegistro;
use crate::registro;
use cifrado::ClaveSincronizacion;
use formato::{Documento, ElementoDocumento, EquipoDocumento, LapidaDocumento};

/// Qué hacer cuando una importación borra más hosts de los umbrales.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoliticaBorrados {
    /// Preguntar (TUI) o devolver código 2 (CLI): no se aplica nada.
    Preguntar,
    /// Aplicar los borrados (el usuario ha aceptado).
    Aplicar,
    /// Mantener los míos: edición nueva de cada host que iba a borrarse.
    Mantener,
}

/// Información de la pasada que necesita saber qué trabajo está vivo: el
/// servidor de sesiones custodia sesiones, túneles, transferencias y
/// ejecuciones. Recibe (tipo de elemento, uuid) y devuelve si hay trabajo vivo.
pub struct OpcionesPasada<'a> {
    pub politica_borrados: PoliticaBorrados,
    pub trabajo_vivo: &'a dyn Fn(&str, &str) -> bool,
}

impl<'a> OpcionesPasada<'a> {
    pub fn sin_trabajo_vivo(politica: PoliticaBorrados) -> Self {
        fn ninguno(_tipo: &str, _uuid: &str) -> bool {
            false
        }
        Self {
            politica_borrados: politica,
            trabajo_vivo: &ninguno,
        }
    }
}

/// Estado final de una pasada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EstadoPasada {
    /// No hay carpeta configurada.
    SinConfigurar,
    /// Hay carpeta pero no hay clave en el llavero (o no está disponible).
    SinClave,
    /// Todo fusionado y exportado; nada pendiente.
    AlDia,
    /// Este equipo ha sido quitado por otro: deja de sincronizar.
    Quitado,
    /// Un fichero de un equipo conocido no se abre con las claves de este
    /// equipo: hace falta introducir la clave nueva.
    EsperandoClave(String),
    /// Hay que confirmar borrados masivos antes de aplicar nada.
    DecisionPendiente(BorradosPendientes),
    /// Error de carpeta, llavero o fichero.
    Error(String),
}

/// Lo que hace falta para preguntar por los borrados masivos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BorradosPendientes {
    /// Copia de magi.db que ya existe antes de aplicar nada.
    pub copia: PathBuf,
    /// Nombres de los hosts que se van a borrar.
    pub hosts: Vec<String>,
    pub total_hosts: usize,
}

/// Resumen de una importación aplicada, para REGISTRO y la vista.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResumenImportacion {
    /// Equipos cuyos ficheros se leyeron.
    pub equipos: usize,
    pub creados: usize,
    pub actualizados: usize,
    pub borrados: usize,
    pub conflictos: usize,
    pub huellas: usize,
}

/// Resultado completo de una pasada.
#[derive(Debug, Clone)]
pub struct ResultadoPasada {
    pub estado: EstadoPasada,
    /// Presente si se aplicó una importación (o se intentó con cambios).
    pub resumen: Option<ResumenImportacion>,
    /// Si se escribió el fichero propio.
    pub exportado: bool,
    /// Conflictos pendientes después de la pasada.
    pub conflictos_pendientes: i64,
    /// Avisos legibles para la barra o la vista.
    pub avisos: Vec<String>,
}

impl ResultadoPasada {
    pub fn nuevo(estado: EstadoPasada) -> Self {
        Self {
            estado,
            resumen: None,
            exportado: false,
            conflictos_pendientes: 0,
            avisos: Vec::new(),
        }
    }
}

/// Umbral de borrados masivos (§7.5): más de `umbral_n` hosts o al menos
/// `umbral_pct` % de ellos.
pub fn requiere_confirmacion(
    borrados_hosts: usize,
    total_hosts: usize,
    umbral_n: u64,
    umbral_pct: u64,
) -> bool {
    if borrados_hosts == 0 || total_hosts == 0 {
        return false;
    }
    borrados_hosts as u64 > umbral_n
        || (borrados_hosts as u64) * 100 >= umbral_pct.saturating_mul(total_hosts as u64)
}

/// Espera máxima por el cerrojo en la CLI.
pub const ESPERA_CERROJO: Duration = Duration::from_secs(10);

/// Fichero leído y validado de otro equipo.
struct FicheroLeido {
    uuid: String,
    hash: String,
    documento: Documento,
}

/// Motivo por el que no se puede leer un fichero de otro equipo.
enum FalloLectura {
    /// Ninguna clave lo descifra.
    SinClave,
    /// Se descifra pero la cabecera o la firma no son válidas.
    Danado(String),
    /// Usa un formato más nuevo.
    FormatoNuevo(u32),
    /// El equipo del documento no coincide con el nombre del fichero.
    EquipoInvalido(String),
}

/// Lee, descifra y verifica el fichero de un equipo. Prueba todas las claves
/// (la actual y, tras una rotación, la anterior).
fn leer_fichero(
    ruta: &Path,
    claves: &[ClaveSincronizacion],
    uuid_esperado: &str,
) -> Result<std::result::Result<FicheroLeido, FalloLectura>> {
    // Un fichero de sincronización no debería pasar de unos pocos MB: uno
    // enorme (o corrupto) se rechaza como dañado en vez de cargarlo en memoria.
    const TAMANO_MAX: u64 = 16 * 1024 * 1024;
    let tamano = std::fs::metadata(ruta)
        .map(|metadatos| metadatos.len())
        .unwrap_or(0);
    if tamano > TAMANO_MAX {
        return Ok(Err(FalloLectura::Danado(format!(
            "el fichero ocupa {tamano} bytes, más del máximo de {TAMANO_MAX}"
        ))));
    }
    let bytes = carpeta::leer(ruta)?;
    let hash = cifrado::sha256_hex(&bytes);
    for clave in claves {
        let Ok(plano) = clave.descifrar(&bytes) else {
            continue;
        };
        let Ok(en_claro) = formato::separar(&plano) else {
            return Ok(Err(FalloLectura::Danado(
                "cabecera de sincronización no válida".to_string(),
            )));
        };
        if !clave.verificar(en_claro.json.as_bytes(), &en_claro.firma) {
            return Ok(Err(FalloLectura::Danado("firma no válida".to_string())));
        }
        if en_claro.formato > formato::FORMATO {
            return Ok(Err(FalloLectura::FormatoNuevo(en_claro.formato)));
        }
        let documento = match formato::parsear(&en_claro.json) {
            Ok(documento) => documento,
            Err(error) => return Ok(Err(FalloLectura::Danado(error.to_string()))),
        };
        if documento.equipo.uuid != uuid_esperado {
            return Ok(Err(FalloLectura::EquipoInvalido(format!(
                "el fichero dice ser del equipo {} y se llama {}",
                documento.equipo.uuid, uuid_esperado
            ))));
        }
        return Ok(Ok(FicheroLeido {
            uuid: uuid_esperado.to_string(),
            hash,
            documento,
        }));
    }
    Ok(Err(FalloLectura::SinClave))
}

/// Construye el documento propio (instantánea completa de este equipo).
pub fn documento_propio(conexion: &Connection, rutas: &Rutas) -> Result<Documento> {
    let este = equipos_bd::este(conexion)?;
    let candidatos = elementos::estado_local(conexion, &rutas.hogar)?;
    let mut elementos = Vec::new();
    let mut lapidas_doc = Vec::new();
    for candidato in candidatos {
        match candidato.contenido {
            Some(datos) => elementos.push(ElementoDocumento {
                tipo: candidato.tipo,
                uuid: candidato.uuid,
                version: candidato.version,
                vector: candidato.vector,
                datos,
            }),
            None => lapidas_doc.push(LapidaDocumento {
                tipo: candidato.tipo,
                uuid: candidato.uuid,
                nombre: candidato.nombre,
                version: candidato.version,
                vector: candidato.vector,
                borrado_en: candidato.borrado_en.unwrap_or_default(),
            }),
        }
    }
    elementos.sort_by(|a, b| (&a.tipo, &a.uuid).cmp(&(&b.tipo, &b.uuid)));
    lapidas_doc.sort_by(|a, b| (&a.tipo, &a.uuid).cmp(&(&b.tipo, &b.uuid)));
    let mut huellas = huellas::exportar(conexion, &rutas.fichero_known_hosts())?;
    huellas.sort_by(|a, b| (&a.host, &a.tipo, &a.clave).cmp(&(&b.host, &b.tipo, &b.clave)));
    let mut equipos_quitados = Vec::new();
    for equipo in equipos_bd::listar(conexion)? {
        if equipo.estado == equipos_bd::ESTADO_QUITADO {
            equipos_quitados.push(formato::EquipoQuitadoDocumento {
                uuid: equipo.uuid,
                quitado_en: equipo.quitado_en.unwrap_or_default(),
            });
        }
    }
    equipos_quitados.sort_by(|a, b| a.uuid.cmp(&b.uuid));
    Ok(Documento {
        formato: formato::FORMATO,
        app: crate::VERSION_APP.to_string(),
        equipo: EquipoDocumento {
            uuid: este.uuid,
            nombre: este.nombre,
            plataforma: este.plataforma,
        },
        generado_en: carpeta::fecha_iso_ahora(),
        reloj_ms: este.reloj_ms.unwrap_or(0),
        elementos,
        lapidas: lapidas_doc,
        huellas,
        equipos_quitados,
    })
}

/// Hash del contenido que no depende de la fecha de generación: sirve para no
/// reexportar el fichero propio si nada cambió.
pub fn hash_canonico(documento: &Documento) -> Result<String> {
    #[derive(Serialize)]
    struct Canonico<'a> {
        formato: u32,
        equipo: &'a EquipoDocumento,
        elementos: &'a [ElementoDocumento],
        lapidas: &'a [LapidaDocumento],
        huellas: &'a [formato::HuellaDocumento],
        equipos_quitados: &'a [formato::EquipoQuitadoDocumento],
    }
    let canonico = Canonico {
        formato: documento.formato,
        equipo: &documento.equipo,
        elementos: &documento.elementos,
        lapidas: &documento.lapidas,
        huellas: &documento.huellas,
        equipos_quitados: &documento.equipos_quitados,
    };
    let json = serde_json::to_string(&canonico)?;
    Ok(cifrado::sha256_hex(json.as_bytes()))
}

/// ¿El plan cambia algo del inventario local?
fn plan_cambia(plan: &fusion::PlanFusion, local: &[fusion::Candidato]) -> bool {
    if !plan.renombrados.is_empty() || !plan.conflictos.is_empty() {
        return true;
    }
    // Un mismo uuid puede tener a la vez un vivo y su lápida histórica (si
    // resucitó): se comparan por separado, nunca mezclados.
    let locales_vivos: BTreeMap<&str, &fusion::Candidato> = local
        .iter()
        .filter(|candidato| candidato.contenido.is_some())
        .map(|candidato| (candidato.uuid.as_str(), candidato))
        .collect();
    let locales_lapidas: BTreeMap<&str, &fusion::Candidato> = local
        .iter()
        .filter(|candidato| candidato.contenido.is_none())
        .map(|candidato| (candidato.uuid.as_str(), candidato))
        .collect();
    let mut vivos_plan: BTreeSet<&str> = BTreeSet::new();
    for vivo in &plan.vivos {
        vivos_plan.insert(vivo.uuid.as_str());
        match locales_vivos.get(vivo.uuid.as_str()) {
            Some(local_vivo) => {
                if local_vivo.version != vivo.version
                    || local_vivo.vector != vivo.vector
                    || local_vivo.contenido_json() != vivo.contenido_json()
                {
                    return true;
                }
            }
            None => return true,
        }
    }
    for uuid in locales_vivos.keys() {
        if !vivos_plan.contains(uuid) {
            return true;
        }
    }
    let mut lapidas_plan: BTreeSet<&str> = BTreeSet::new();
    for lapida in &plan.lapidas {
        lapidas_plan.insert(lapida.uuid.as_str());
        match locales_lapidas.get(lapida.uuid.as_str()) {
            Some(local_lapida) => {
                if local_lapida.version != lapida.version || local_lapida.vector != lapida.vector {
                    return true;
                }
            }
            None => return true,
        }
    }
    for uuid in locales_lapidas.keys() {
        if !lapidas_plan.contains(uuid) {
            return true;
        }
    }
    false
}

/// Nombres de los hosts locales que desaparecen con este plan.
fn hosts_a_borrar(plan: &fusion::PlanFusion, local: &[fusion::Candidato]) -> (Vec<String>, usize) {
    let vivos_plan: BTreeSet<&str> = plan
        .vivos
        .iter()
        .filter(|vivo| vivo.tipo == "host")
        .map(|vivo| vivo.uuid.as_str())
        .collect();
    let mut borrados = Vec::new();
    let mut total = 0usize;
    for candidato in local {
        if candidato.tipo == "host" && candidato.contenido.is_some() {
            total += 1;
            if !vivos_plan.contains(candidato.uuid.as_str()) {
                borrados.push(candidato.nombre.clone());
            }
        }
    }
    (borrados, total)
}

/// Una pasada completa de sincronización. Lee la carpeta, fusiona, aplica y
/// exporta. No toma el cerrojo: lo hace quien llama (la UI sin bloquear, la
/// CLI esperando hasta 10 s).
pub fn pasada(
    conexion: &Connection,
    rutas: &Rutas,
    config: &Config,
    claves: &[ClaveSincronizacion],
    opciones: &OpcionesPasada<'_>,
) -> Result<ResultadoPasada> {
    let Some(carpeta) = config.sincronizacion.ruta_carpeta(&rutas.hogar) else {
        return Ok(ResultadoPasada::nuevo(EstadoPasada::SinConfigurar));
    };
    if claves.is_empty() {
        return Ok(ResultadoPasada::nuevo(EstadoPasada::SinClave));
    }
    let mut resultado = ResultadoPasada::nuevo(EstadoPasada::AlDia);
    let este = equipos_bd::este(conexion)?;
    if este.estado == equipos_bd::ESTADO_QUITADO {
        return Ok(ResultadoPasada::nuevo(EstadoPasada::Quitado));
    }
    if !carpeta.is_dir() {
        resultado.estado = EstadoPasada::Error(format!(
            "la carpeta de sincronización {} no existe",
            carpeta.display()
        ));
        return Ok(resultado);
    }

    // 1. Leer los ficheros ajenos que hayan cambiado.
    let ficheros = match carpeta::listar(&carpeta) {
        Ok(ficheros) => ficheros,
        Err(error) => {
            resultado.estado = EstadoPasada::Error(error.to_string());
            return Ok(resultado);
        }
    };
    let mut leidos: Vec<FicheroLeido> = Vec::new();
    for fichero in &ficheros {
        if fichero.uuid == este.uuid {
            continue;
        }
        let equipo = equipos_bd::por_uuid(conexion, &fichero.uuid)?;
        if let Some(equipo) = &equipo {
            if equipo.estado == equipos_bd::ESTADO_QUITADO {
                continue;
            }
            if equipo.hash_ultimo_fichero.as_deref() == Some(fichero.hash.as_str()) {
                continue;
            }
        }
        match leer_fichero(&fichero.ruta, claves, &fichero.uuid) {
            Err(error) => {
                let mensaje = format!("leyendo {}: {error}", fichero.ruta.display());
                resultado.avisos.push(mensaje.clone());
                if equipo.is_some() {
                    resultado.estado = EstadoPasada::Error(mensaje);
                }
            }
            Ok(Ok(leido)) => {
                if equipo.is_none() {
                    equipos_bd::crear(
                        conexion,
                        &leido.uuid,
                        &leido.documento.equipo.nombre,
                        &leido.documento.equipo.plataforma,
                    )?;
                }
                leidos.push(leido);
            }
            Ok(Err(FalloLectura::SinClave)) => {
                if let Some(equipo) = &equipo {
                    equipos_bd::fijar_estado(conexion, equipo.id, equipos_bd::ESTADO_ILEGIBLE)?;
                    resultado.estado = EstadoPasada::EsperandoClave(equipo.nombre.clone());
                    resultado.avisos.push(format!(
                        "el fichero de «{}» no se abre con la clave de este equipo",
                        equipo.nombre
                    ));
                    return Ok(resultado);
                }
                resultado.avisos.push(format!(
                    "un fichero de la carpeta ({}) no se abre con esta clave",
                    fichero.ruta.display()
                ));
            }
            Ok(Err(FalloLectura::Danado(motivo))) => {
                if let Some(equipo) = &equipo {
                    equipos_bd::fijar_estado(conexion, equipo.id, equipos_bd::ESTADO_DANADO)?;
                }
                resultado.avisos.push(format!(
                    "el fichero de {} está dañado ({motivo}); se reintentará",
                    equipo
                        .map(|equipo| equipo.nombre)
                        .unwrap_or_else(|| fichero.uuid.clone())
                ));
            }
            Ok(Err(FalloLectura::FormatoNuevo(numero))) => {
                if let Some(equipo) = &equipo {
                    equipos_bd::fijar_estado(
                        conexion,
                        equipo.id,
                        equipos_bd::ESTADO_FORMATO_NUEVO,
                    )?;
                }
                let nombre = equipo
                    .map(|equipo| equipo.nombre)
                    .unwrap_or_else(|| fichero.uuid.clone());
                resultado.avisos.push(format!(
                    "el equipo {nombre} usa una versión más nueva de MAGI; actualiza"
                ));
                let _ = numero;
            }
            Ok(Err(FalloLectura::EquipoInvalido(motivo))) => {
                if let Some(equipo) = &equipo {
                    equipos_bd::fijar_estado(conexion, equipo.id, equipos_bd::ESTADO_DANADO)?;
                }
                resultado.avisos.push(motivo);
            }
        }
    }
    if let EstadoPasada::Error(_) = resultado.estado {
        return Ok(resultado);
    }

    // 2. ¿Este equipo ha sido quitado por otro?
    let documentos_owned: Vec<Documento> = leidos.iter().map(|l| l.documento.clone()).collect();
    let quitados = equipos::equipos_quitados(&documentos_owned);
    for (uuid, fecha) in &quitados {
        if uuid == &este.uuid {
            continue;
        }
        match equipos_bd::por_uuid(conexion, uuid)? {
            Some(equipo) if equipo.estado != equipos_bd::ESTADO_QUITADO => {
                equipos_bd::marcar_quitado(conexion, equipo.id, fecha)?;
            }
            Some(_) => {}
            None => {
                let id = equipos_bd::crear(conexion, uuid, "desconocido", "linux")?;
                equipos_bd::marcar_quitado(conexion, id, fecha)?;
            }
        }
    }
    // El fichero de un equipo que se acaba de descubrir quitado no se importa
    // en esta misma pasada: su estado ya no es al_dia y sus elementos no
    // cuentan.
    leidos.retain(|leido| !quitados.contains_key(&leido.uuid));
    if quitados.contains_key(&este.uuid) {
        equipos_bd::marcar_quitado(conexion, este.id, &crate::modelo::fecha_ahora())?;
        registro::anotar(
            conexion,
            registro::INVENTARIO_SINC_ERROR,
            None,
            None,
            "este equipo ha sido quitado de la sincronización; únete de nuevo como equipo nuevo",
            ResultadoRegistro::Ok,
        )?;
        return Ok(ResultadoPasada::nuevo(EstadoPasada::Quitado));
    }

    // 3. Fusionar.
    let documentos_owned: Vec<Documento> = leidos.iter().map(|l| l.documento.clone()).collect();
    let local = elementos::estado_local(conexion, &rutas.hogar)?;
    let mut plan = fusion::fusionar(&local, &documentos_owned)?;
    let mut cambia = plan_cambia(&plan, &local);

    // 4. Umbral de borrados masivos: copia y, si toca, preguntar sin aplicar.
    let (mut hosts_borrados, total_hosts) = hosts_a_borrar(&plan, &local);
    let umbral = requiere_confirmacion(
        hosts_borrados.len(),
        total_hosts,
        config.sincronizacion.umbral_borrado_hosts,
        config.sincronizacion.umbral_borrado_pct,
    );
    if umbral && opciones.politica_borrados == PoliticaBorrados::Preguntar && cambia {
        let copia = copias::copiar(conexion, rutas, config.sincronizacion.copias)?;
        return Ok(ResultadoPasada {
            estado: EstadoPasada::DecisionPendiente(BorradosPendientes {
                copia,
                hosts: std::mem::take(&mut hosts_borrados),
                total_hosts,
            }),
            ..ResultadoPasada::nuevo(EstadoPasada::AlDia)
        });
    }

    // 5. «Mantener los míos»: resucita los hosts que iban a borrarse con una
    //    edición nueva y vuelve a fusionar.
    if cambia
        && opciones.politica_borrados == PoliticaBorrados::Mantener
        && !hosts_borrados.is_empty()
    {
        versiones::al_importar(conexion, plan.reloj_ms)?;
        for nombre in &hosts_borrados {
            let id: Option<i64> = conexion
                .query_row(
                    "SELECT id FROM HOSTS WHERE nombre = ?1",
                    rusqlite::params![nombre],
                    |fila| fila.get(0),
                )
                .ok();
            if let Some(id) = id {
                versiones::subir(conexion, versiones::Tabla::Hosts, id)?;
            }
        }
        let local = elementos::estado_local(conexion, &rutas.hogar)?;
        plan = fusion::fusionar(&local, &documentos_owned)?;
        cambia = plan_cambia(&plan, &local);
    }

    // 6. Aplicar: copia previa si se va a tocar algo (inventario o huellas).
    let hay_huellas = documentos_owned
        .iter()
        .any(|documento| !documento.huellas.is_empty());
    let hay_cambios_huellas = hay_huellas && !leidos.is_empty();
    let mut resumen = ResumenImportacion {
        equipos: leidos.len(),
        ..ResumenImportacion::default()
    };
    if cambia || hay_cambios_huellas {
        copias::copiar(conexion, rutas, config.sincronizacion.copias)?;
    }
    if cambia {
        // Borrados con trabajo vivo: se aplazan y quedan como conflicto.
        let mut aplazados: BTreeSet<String> = BTreeSet::new();
        let vivos_plan: BTreeSet<&str> = plan.vivos.iter().map(|v| v.uuid.as_str()).collect();
        let mut conflictos_borrado = Vec::new();
        for candidato in &local {
            if candidato.contenido.is_none() || vivos_plan.contains(candidato.uuid.as_str()) {
                continue;
            }
            if candidato.tipo != "host" && candidato.tipo != "tunel" {
                continue;
            }
            if (opciones.trabajo_vivo)(&candidato.tipo, &candidato.uuid) {
                aplazados.insert(candidato.uuid.clone());
                conflictos_borrado.push(fusion::ConflictoFusion {
                    tipo: conflictos::TIPO_BORRADO.to_string(),
                    elemento_tipo: candidato.tipo.clone(),
                    uuid: candidato.uuid.clone(),
                    nombre: candidato.nombre.clone(),
                    version_perdedora_json: None,
                    version_ganadora: None,
                    equipo_ganador: None,
                });
            }
        }
        versiones::al_importar(conexion, plan.reloj_ms)?;
        // La aplicación persiste los conflictos de la fusión; aquí solo se
        // crean los borrados aplazados por trabajo vivo. Después se anotan en
        // REGISTRO los conflictos nuevos de esta pasada.
        let max_conflicto: i64 = conexion
            .query_row("SELECT COALESCE(MAX(id), 0) FROM CONFLICTOS", [], |fila| {
                fila.get(0)
            })
            .unwrap_or(0);
        let opciones_aplicar = aplicacion::OpcionesAplicacion {
            borrados_aplazados: aplazados,
        };
        resumen = {
            let aplicado = aplicacion::aplicar(conexion, &plan, &opciones_aplicar, &rutas.hogar)?;
            ResumenImportacion {
                equipos: leidos.len(),
                creados: aplicado.creados,
                actualizados: aplicado.actualizados,
                borrados: aplicado.borrados,
                conflictos: aplicado.conflictos,
                huellas: 0,
            }
        };
        for conflicto in &conflictos_borrado {
            let nuevo = conflictos::NuevoConflicto {
                tipo: conflicto.tipo.clone(),
                elemento_tipo: conflicto.elemento_tipo.clone(),
                uuid: conflicto.uuid.clone(),
                nombre: conflicto.nombre.clone(),
                version_perdedora_json: conflicto.version_perdedora_json.clone(),
                version_ganadora: conflicto.version_ganadora.clone(),
                equipo_ganador: conflicto.equipo_ganador.clone(),
            };
            conflictos::crear_si_nuevo(conexion, &nuevo)?;
        }
        {
            let mut sentencia = conexion.prepare(
                "SELECT tipo, elemento_tipo, nombre FROM CONFLICTOS WHERE id > ?1 ORDER BY id",
            )?;
            let filas = sentencia.query_map(rusqlite::params![max_conflicto], |fila| {
                Ok((
                    fila.get::<_, String>(0)?,
                    fila.get::<_, String>(1)?,
                    fila.get::<_, String>(2)?,
                ))
            })?;
            let nuevos: Vec<(String, String, String)> = filas.collect::<rusqlite::Result<_>>()?;
            resumen.conflictos = nuevos.len();
            for (tipo, elemento_tipo, nombre) in &nuevos {
                registro::anotar(
                    conexion,
                    registro::INVENTARIO_CONFLICTO,
                    None,
                    None,
                    &format!("conflicto {tipo} de {elemento_tipo} «{nombre}»"),
                    ResultadoRegistro::Ok,
                )?;
            }
        }
        registro::anotar(
            conexion,
            registro::INVENTARIO_IMPORTADO,
            None,
            None,
            &format!(
                "equipos: {}, creados: {}, actualizados: {}, borrados: {}, conflictos: {}",
                resumen.equipos,
                resumen.creados,
                resumen.actualizados,
                resumen.borrados,
                resumen.conflictos
            ),
            ResultadoRegistro::Ok,
        )?;
        if config.exportar_al_guardar {
            let _ = crate::sshconfig::exportar::exportar(conexion, &rutas.dir_ssh());
        }
    }

    // 7. Huellas: añadir solo si no hay ninguna; conflicto si hay otra.
    if documentos_owned
        .iter()
        .any(|documento| !documento.huellas.is_empty())
    {
        let mut todas = Vec::new();
        let mut mapa_equipos: BTreeMap<String, (String, String)> = BTreeMap::new();
        for documento in &documentos_owned {
            todas.extend(documento.huellas.iter().cloned());
            mapa_equipos.insert(
                documento.equipo.uuid.clone(),
                (
                    documento.equipo.nombre.clone(),
                    documento.generado_en.clone(),
                ),
            );
        }
        let resultado_huellas = huellas::importar(
            conexion,
            &rutas.fichero_known_hosts(),
            &todas,
            &mapa_equipos,
        )?;
        resumen.huellas = resultado_huellas.anadidas.len();
        for conflicto in &resultado_huellas.conflictos {
            let nuevo = conflictos::NuevoConflicto {
                tipo: conflicto.tipo.clone(),
                elemento_tipo: conflicto.elemento_tipo.clone(),
                uuid: conflicto.uuid.clone(),
                nombre: conflicto.nombre.clone(),
                version_perdedora_json: conflicto.version_perdedora_json.clone(),
                version_ganadora: conflicto.version_ganadora.clone(),
                equipo_ganador: conflicto.equipo_ganador.clone(),
            };
            if conflictos::crear_si_nuevo(conexion, &nuevo)?.is_some() {
                resumen.conflictos += 1;
                registro::anotar(
                    conexion,
                    registro::INVENTARIO_CONFLICTO,
                    None,
                    None,
                    &format!(
                        "conflicto {} de {} «{}»",
                        conflicto.tipo, conflicto.elemento_tipo, conflicto.nombre
                    ),
                    ResultadoRegistro::Ok,
                )?;
            }
        }
        resultado.avisos.extend(resultado_huellas.avisos);
    }

    // 8. Marcar los ficheros leídos como importados.
    for leido in &leidos {
        if let Some(equipo) = equipos_bd::por_uuid(conexion, &leido.uuid)? {
            equipos_bd::fijar_ultimo_fichero(
                conexion,
                equipo.id,
                &leido.hash,
                &leido.documento.generado_en,
                Some(&leido.documento.app),
            )?;
            equipos_bd::fijar_importacion(conexion, equipo.id, &leido.documento.generado_en)?;
        }
    }
    if cambia || hay_cambios_huellas {
        resultado.resumen = Some(resumen);
    }

    // 9. Exportar el fichero propio si su contenido cambió (o no existe).
    let documento = documento_propio(conexion, rutas)?;
    let hash = hash_canonico(&documento)?;
    let ruta_propia = carpeta.join(carpeta::nombre_fichero(&este.uuid));
    let hash_anterior = equipos_bd::este(conexion)?.hash_ultimo_fichero;
    if hash_anterior.as_deref() != Some(hash.as_str()) || !ruta_propia.exists() {
        let clave = &claves[0];
        let json = formato::serializar(&documento)?;
        let firma = clave.firmar(json.as_bytes())?;
        let plano = formato::componer(&json, &firma);
        let cifrado = clave.cifrar(&plano)?;
        if let Err(error) = carpeta::escribir(&carpeta, &este.uuid, &cifrado) {
            resultado.estado = EstadoPasada::Error(format!(
                "no se pudo escribir el fichero de este equipo: {error}"
            ));
            return Ok(resultado);
        }
        equipos_bd::fijar_hash_propio(conexion, este.id, &hash)?;
        resultado.exportado = true;
    }

    resultado.conflictos_pendientes = conflictos::contar_pendientes(conexion)?;
    Ok(resultado)
}

/// Texto de estado para la CLI y la vista: una línea por equipo. No descifra
/// nada ni lee la clave del llavero.
pub fn estado_texto(conexion: &Connection, config: &Config, hogar: &Path) -> Result<String> {
    let mut texto = String::new();
    match config.sincronizacion.ruta_carpeta(hogar) {
        Some(carpeta) => texto.push_str(&format!("carpeta: {}\n", carpeta.display())),
        None => {
            texto.push_str("carpeta: (sin configurar)\n");
            return Ok(texto);
        }
    }
    let este = equipos_bd::este(conexion)?;
    texto.push_str(&format!(
        "este equipo: {} ({}) · {}\n",
        este.nombre, este.uuid, este.plataforma
    ));
    let equipos = equipos_bd::listar(conexion)?;
    texto.push_str("equipos:\n");
    for equipo in equipos {
        texto.push_str(&format!(
            "  {:<24} {:<8} {:<8} {}  {}\n",
            equipo.nombre,
            equipo.plataforma,
            equipo.app_version.unwrap_or_else(|| "—".to_string()),
            equipo
                .ultimo_fichero_en
                .unwrap_or_else(|| "nunca".to_string()),
            equipo.estado
        ));
    }
    let pendientes = conflictos::listar_pendientes(conexion)?;
    texto.push_str(&format!("conflictos pendientes: {}\n", pendientes.len()));
    for conflicto in pendientes.iter().take(20) {
        texto.push_str(&format!(
            "  {} {} «{}» (ganó {})\n",
            conflicto.tipo,
            conflicto.elemento_tipo,
            conflicto.nombre,
            conflicto
                .equipo_ganador
                .clone()
                .unwrap_or_else(|| "—".to_string())
        ));
    }
    Ok(texto)
}
