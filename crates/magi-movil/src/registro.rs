//! Registro del móvil (informe §5.1, §6.10 y §7.10): las entradas del propio
//! móvil por páginas, con filtros por host, tipo y texto, y el detalle de una
//! entrada con las comprobaciones de su deliberación.
//!
//! Las consultas salen de `REGISTRO` (la tabla que escribe el hilo escritor) y
//! de `DELIBERACIONES` para el detalle de las anotaciones `deliberacion_*`. La
//! paginación va por cursor de id (de la más nueva a la más vieja, como pide
//! §7.10), así que una entrada nueva no desplaza las páginas que ya se leyeron.
//! Una deliberación sin `ejecucion_resultado` se marca `sin_resultado`
//! («MAGI se cerró durante la ejecución»); no se escribe nada en la base de
//! datos para decirlo.

use std::collections::HashMap;

use anyhow::{Context, Result};
use magi_nucleo::almacen::movil as movil_bd;
use magi_nucleo::almacen::versiones::Tabla;
use magi_nucleo::almacen::{deliberaciones as delib_bd, Almacen};
use magi_nucleo::deliberacion::{
    self, EjecucionResultado, RegistroDeliberacion, ResultadoDeliberacion, Veredicto,
};
use magi_nucleo::modelo::ResultadoRegistro;

use crate::api::{
    DetalleDeliberacion, DetalleRegistro, EntradaRegistro, ErrorNucleo, FilaHostDeliberacion,
    FiltroRegistro, ResultadoDeliberacionMovil, ResultadoEjecucionMovil, ResultadoRegistroMovil,
    TipoRegistroFiltro, VeredictoMovil,
};
use crate::estado::{con_almacen, EstadoNucleo};

/// Entradas por página (informe §7.10): el tope que admite la API aunque la
/// app pida más.
pub const POR_PAGINA: u32 = 50;

/// Una página de entradas, de la más nueva a la más vieja. `desde` es el id de
/// la última entrada de la página anterior; None empieza por la más nueva.
/// `cuantas` se acota a [`POR_PAGINA`] (0 pide la página entera).
pub fn entradas(
    estado: &EstadoNucleo,
    filtro: &FiltroRegistro,
    desde: Option<u64>,
    cuantas: u32,
) -> Result<Vec<EntradaRegistro>, ErrorNucleo> {
    let filtro = filtro.clone();
    let resultado = con_almacen(estado, move |almacen| {
        entradas_en(almacen, &filtro, desde, cuantas)
    })?;
    resultado.map_err(|error| ErrorNucleo::General {
        detalle: format!("leyendo el registro: {error:#}"),
    })
}

/// Detalle de una entrada; en las `deliberacion_*`, con la tabla de
/// comprobaciones leída de DELIBERACIONES (informe §7.10).
pub fn detalle(estado: &EstadoNucleo, id: u64) -> Result<DetalleRegistro, ErrorNucleo> {
    let resultado = con_almacen(estado, move |almacen| detalle_en(almacen, id))?;
    let detalle = resultado.map_err(|error| ErrorNucleo::General {
        detalle: format!("leyendo la entrada del registro: {error:#}"),
    })?;
    detalle.ok_or_else(|| ErrorNucleo::NoEncontrado {
        detalle: format!("la entrada {id} del registro"),
    })
}

// ------------------------------------------------------------------ consultas

/// Página del registro para un almacén ya abierto (la usan las pruebas).
fn entradas_en(
    almacen: &Almacen,
    filtro: &FiltroRegistro,
    desde: Option<u64>,
    cuantas: u32,
) -> Result<Vec<EntradaRegistro>> {
    let conexion = almacen.conexion();
    let host_id = match filtro.host_uuid.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(uuid) => match movil_bd::id_por_uuid(conexion, Tabla::Hosts, uuid)? {
            Some(id) => Some(id),
            // Un host que ya no está no puede tener entradas a su nombre.
            None => return Ok(Vec::new()),
        },
    };
    let limite = if cuantas == 0 {
        POR_PAGINA
    } else {
        cuantas.min(POR_PAGINA)
    };
    let mut condiciones = Vec::new();
    let tipos = tipos_de(&filtro.tipos);
    if !tipos.is_empty() {
        let lista: Vec<String> = tipos.iter().map(|tipo| literal(tipo)).collect();
        condiciones.push(format!("r.tipo IN ({})", lista.join(", ")));
    }
    let texto = filtro.texto.trim();
    if !texto.is_empty() {
        let patron = patron_like(texto);
        condiciones.push(format!(
            "(r.tipo LIKE {patron} ESCAPE '\\' OR r.detalle LIKE {patron} ESCAPE '\\' \
             OR h.nombre LIKE {patron} ESCAPE '\\')"
        ));
    }
    if let Some(host_id) = host_id {
        condiciones.push(format!("r.host_id = {host_id}"));
    }
    if let Some(desde) = desde {
        condiciones.push(format!(
            "r.id < {}",
            i64::try_from(desde).unwrap_or(i64::MAX)
        ));
    }
    let filtro_sql = if condiciones.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", condiciones.join(" AND "))
    };
    let sql = format!("{SELECCION}{filtro_sql} ORDER BY r.id DESC LIMIT {limite}");
    let uuids = movil_bd::uuid_por_id(conexion, Tabla::Hosts)?;
    let mut sentencia = conexion.prepare(&sql)?;
    let filas = sentencia.query_map([], |fila| {
        Ok(a_entrada(
            fila.get(0)?,
            fila.get(1)?,
            fila.get(2)?,
            fila.get(3)?,
            fila.get(4)?,
            fila.get(5)?,
            fila.get(6)?,
            &uuids,
        ))
    })?;
    let mut lista = Vec::new();
    for fila in filas {
        lista.push(fila?);
    }
    Ok(lista)
}

/// Detalle de una entrada para un almacén ya abierto (la usan las pruebas).
fn detalle_en(almacen: &Almacen, id: u64) -> Result<Option<DetalleRegistro>> {
    let conexion = almacen.conexion();
    let id = i64::try_from(id).unwrap_or(i64::MAX);
    let uuids = movil_bd::uuid_por_id(conexion, Tabla::Hosts)?;
    let sql = format!("{SELECCION} WHERE r.id = {id}");
    let mut sentencia = conexion.prepare(&sql)?;
    let mut filas = sentencia.query_map([], |fila| {
        Ok(a_entrada(
            fila.get(0)?,
            fila.get(1)?,
            fila.get(2)?,
            fila.get(3)?,
            fila.get(4)?,
            fila.get(5)?,
            fila.get(6)?,
            &uuids,
        ))
    })?;
    let Some(fila) = filas.next() else {
        return Ok(None);
    };
    let entrada = fila?;
    let deliberacion = if entrada.tipo.starts_with("deliberacion_") {
        leer_deliberacion(almacen, &entrada, &uuids)?
    } else {
        None
    };
    Ok(Some(DetalleRegistro {
        entrada,
        deliberacion,
    }))
}

/// Fila de DELIBERACIONES de una anotación `deliberacion_*`, si sigue estando.
fn leer_deliberacion(
    almacen: &Almacen,
    entrada: &EntradaRegistro,
    uuids: &HashMap<i64, String>,
) -> Result<Option<DetalleDeliberacion>> {
    let conexion = almacen.conexion();
    let Some(id) = deliberacion::id_en_detalle(&entrada.detalle) else {
        return Ok(None);
    };
    let mut sentencia =
        conexion.prepare(&format!("SELECT id FROM DELIBERACIONES WHERE id = {id}"))?;
    let mut filas = sentencia.query_map([], |fila| fila.get::<_, i64>(0))?;
    if filas.next().transpose()?.is_none() {
        return Ok(None);
    }
    let registro = delib_bd::obtener(conexion, id).context("leyendo la deliberación")?;
    Ok(Some(a_detalle_deliberacion(&registro, uuids)))
}

const SELECCION: &str = "
    SELECT r.id, r.fecha, r.tipo, r.host_id, h.nombre, r.detalle, r.resultado
      FROM REGISTRO r
      LEFT JOIN HOSTS h ON h.id = r.host_id";

#[allow(clippy::too_many_arguments)]
fn a_entrada(
    id: i64,
    fecha: String,
    tipo: String,
    host_id: Option<i64>,
    host_nombre: Option<String>,
    detalle: String,
    resultado: String,
    uuids: &HashMap<i64, String>,
) -> EntradaRegistro {
    EntradaRegistro {
        id: u64::try_from(id).unwrap_or(0),
        fecha: epoca(&fecha),
        tipo,
        host_uuid: host_id.and_then(|host_id| uuids.get(&host_id).cloned()),
        host_nombre,
        resultado: match ResultadoRegistro::desde_texto(&resultado) {
            ResultadoRegistro::Ok => ResultadoRegistroMovil::Ok,
            ResultadoRegistro::Error => ResultadoRegistroMovil::Error,
        },
        detalle,
    }
}

fn a_detalle_deliberacion(
    registro: &RegistroDeliberacion,
    uuids: &HashMap<i64, String>,
) -> DetalleDeliberacion {
    let (aprobadas, activas) = registro.consenso();
    let filas = registro
        .comprobaciones
        .iter()
        .map(|comprobaciones| FilaHostDeliberacion {
            host_uuid: uuids.get(&comprobaciones.host_id).cloned(),
            host_nombre: comprobaciones.host.clone(),
            salud: veredicto_movil(&comprobaciones.salud),
            backup: veredicto_movil(&comprobaciones.backup),
            tests: veredicto_movil(&comprobaciones.tests),
        })
        .collect();
    let ejecucion_resultado = registro.ejecucion_resultado.map(resultado_ejecucion_movil);
    DetalleDeliberacion {
        id: u64::try_from(registro.id).unwrap_or(0),
        fecha: epoca(&registro.fecha),
        accion: registro.accion.clone(),
        resultado: match registro.resultado {
            ResultadoDeliberacion::Aprobada => ResultadoDeliberacionMovil::Aprobada,
            ResultadoDeliberacion::Forzada => ResultadoDeliberacionMovil::Forzada,
            ResultadoDeliberacion::Cancelada => ResultadoDeliberacionMovil::Cancelada,
        },
        bloqueada: registro.bloqueada,
        motivo: registro.motivo.clone(),
        usuario: registro.usuario.clone(),
        ejecucion_resultado,
        // Una cancelada no se ejecutó: no echa en falta el resultado.
        sin_resultado: !matches!(registro.resultado, ResultadoDeliberacion::Cancelada)
            && ejecucion_resultado.is_none(),
        filas,
        aprobadas: aprobadas as u32,
        activas: activas as u32,
    }
}

fn veredicto_movil(veredicto: &Veredicto) -> VeredictoMovil {
    match veredicto {
        Veredicto::NoActiva => VeredictoMovil::NoActiva,
        Veredicto::Pendiente => VeredictoMovil::Pendiente,
        Veredicto::Aprueba { detalle, ms } => VeredictoMovil::Aprueba {
            detalle: detalle.clone(),
            ms: *ms,
        },
        Veredicto::Rechaza { detalle, ms } => VeredictoMovil::Rechaza {
            detalle: detalle.clone(),
            ms: *ms,
        },
    }
}

fn resultado_ejecucion_movil(resultado: EjecucionResultado) -> ResultadoEjecucionMovil {
    match resultado {
        EjecucionResultado::Ok => ResultadoEjecucionMovil::Ok,
        EjecucionResultado::Parcial => ResultadoEjecucionMovil::Parcial,
        EjecucionResultado::Error => ResultadoEjecucionMovil::Error,
        EjecucionResultado::Cancelada => ResultadoEjecucionMovil::Cancelada,
    }
}

/// Tipos de evento de cada filtro rápido de §7.10.
fn tipos_de(filtros: &[TipoRegistroFiltro]) -> Vec<&'static str> {
    use magi_nucleo::registro as evento;
    let mut tipos = Vec::new();
    for filtro in filtros {
        let grupo: &[&'static str] = match filtro {
            TipoRegistroFiltro::Conexiones => &[
                evento::CONEXION_ABIERTA,
                evento::CONEXION_FALLIDA,
                evento::SESION_CERRADA,
                evento::SESION_RECONECTADA,
            ],
            TipoRegistroFiltro::Huellas => &[
                evento::HUELLA_ACEPTADA,
                evento::HUELLA_SUSTITUIDA,
                evento::HUELLA_SINCRONIZADA,
            ],
            TipoRegistroFiltro::Tuneles => &[
                evento::TUNEL_ABIERTO,
                evento::TUNEL_CERRADO,
                evento::TUNEL_FALLIDO,
            ],
            TipoRegistroFiltro::Sondeos => &[evento::SONDEO_FALLIDO],
            TipoRegistroFiltro::Snippets => &[evento::SNIPPET_EJECUTADO],
            TipoRegistroFiltro::Deliberaciones => &[
                evento::DELIBERACION_APROBADA,
                evento::DELIBERACION_FORZADA,
                evento::DELIBERACION_CANCELADA,
            ],
            TipoRegistroFiltro::Servicios => &[evento::SERVICIO_ACCION],
            TipoRegistroFiltro::Claves => &[evento::CLAVE_GENERADA, evento::CLAVE_INSTALADA],
            TipoRegistroFiltro::Sincronizacion => &[evento::INVENTARIO_IMPORTADO],
        };
        tipos.extend_from_slice(grupo);
    }
    tipos
}

/// Literal de texto seguro para una consulta (comillas simples dobladas).
///
/// La puerta móvil no depende de `rusqlite`; en lugar de parámetros se compone
/// la consulta con literales ya escapados: los tipos son constantes del núcleo
/// y el texto del filtro se cita doblando las comillas.
fn literal(texto: &str) -> String {
    format!("'{}'", texto.replace('\'', "''"))
}

/// Patrón LIKE entre comodines, con `%_` y la barra escapados (literal).
fn patron_like(texto: &str) -> String {
    let escapado = texto
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    literal(&format!("%{escapado}%"))
}

/// Fecha ISO de `REGISTRO` en segundos desde la época.
fn epoca(fecha: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(fecha)
        .map(|fecha| fecha.timestamp())
        .unwrap_or(0)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    use magi_nucleo::almacen::hosts as hosts_bd;
    use magi_nucleo::deliberacion::{
        ComprobacionesHost, NuevaDeliberacion, ResultadoDeliberacion, Veredicto,
    };
    use magi_nucleo::modelo::{DatosHost, Origen};
    use magi_nucleo::registro as registro_eventos;

    fn almacen() -> Almacen {
        Almacen::abrir_en_memoria().expect("almacén en memoria")
    }

    /// Crea un host con uuid (los que viajan lo tienen al crearse) y devuelve
    /// (id, uuid).
    fn host(almacen: &Almacen, nombre: &str) -> (i64, String) {
        let id = hosts_bd::crear(
            almacen.conexion(),
            &DatosHost {
                nombre: nombre.to_string(),
                direccion: "192.0.2.10".to_string(),
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .expect("creando el host");
        let uuid = movil_bd::uuid_por_id(almacen.conexion(), Tabla::Hosts)
            .expect("uuids")
            .remove(&id)
            .expect("el host tiene uuid");
        (id, uuid)
    }

    fn anotar(almacen: &Almacen, tipo: &str, host_id: i64, detalle: &str) {
        registro_eventos::anotar(
            almacen.conexion(),
            tipo,
            Some(host_id),
            None,
            detalle,
            ResultadoRegistro::Ok,
        )
        .expect("anotando en el registro");
    }

    fn filtro(
        tipos: Vec<TipoRegistroFiltro>,
        texto: &str,
        host_uuid: Option<String>,
    ) -> FiltroRegistro {
        FiltroRegistro {
            host_uuid,
            tipos,
            texto: texto.to_string(),
        }
    }

    #[test]
    fn se_pagina_de_50_en_50_por_id_y_una_entrada_nueva_no_desplaza() {
        let almacen = almacen();
        let (host_id, _) = host(&almacen, "web-01");
        for numero in 1..=55 {
            anotar(
                &almacen,
                "conexion_abierta",
                host_id,
                &format!("entrada {numero}"),
            );
        }
        let primera =
            entradas_en(&almacen, &filtro(Vec::new(), "", None), None, 50).expect("página 1");
        assert_eq!(primera.len(), 50);
        assert_eq!(primera[0].detalle, "entrada 55");
        assert_eq!(primera[49].detalle, "entrada 6");
        // Entra una más por delante: el cursor de id no la repite.
        anotar(&almacen, "conexion_abierta", host_id, "entrada 56");
        let segunda = entradas_en(
            &almacen,
            &filtro(Vec::new(), "", None),
            Some(primera[49].id),
            50,
        )
        .expect("página 2");
        assert_eq!(segunda.len(), 5);
        assert_eq!(segunda[0].detalle, "entrada 5");
        assert!(segunda.iter().all(|entrada| entrada.id < primera[49].id));
    }

    #[test]
    fn el_filtro_por_tipos_texto_y_host_se_aplica_en_sql() {
        let almacen = almacen();
        let (web, web_uuid) = host(&almacen, "web-01");
        let (db, db_uuid) = host(&almacen, "db-01");
        anotar(&almacen, "conexion_abierta", web, "sesión abierta");
        anotar(&almacen, "servicio_accion", db, "reiniciar nginx.service");
        anotar(&almacen, "snippet_ejecutado", web, "reiniciar nginx");
        anotar(
            &almacen,
            "deliberacion_aprobada",
            web,
            "deliberación aprobada",
        );

        let conexiones = entradas_en(
            &almacen,
            &filtro(vec![TipoRegistroFiltro::Conexiones], "", None),
            None,
            50,
        )
        .expect("filtro por tipo");
        assert_eq!(conexiones.len(), 1);
        assert_eq!(conexiones[0].tipo, "conexion_abierta");

        let servicios = entradas_en(
            &almacen,
            &filtro(vec![TipoRegistroFiltro::Servicios], "", None),
            None,
            50,
        )
        .expect("filtro por tipo");
        assert_eq!(servicios.len(), 1);
        assert_eq!(servicios[0].host_nombre.as_deref(), Some("db-01"));

        let texto = entradas_en(&almacen, &filtro(Vec::new(), "nginx", None), None, 50)
            .expect("filtro por texto");
        assert_eq!(texto.len(), 2);

        let por_host = entradas_en(
            &almacen,
            &filtro(Vec::new(), "", Some(db_uuid.clone())),
            None,
            50,
        )
        .expect("filtro por host");
        assert_eq!(por_host.len(), 1);
        assert_eq!(por_host[0].host_uuid.as_deref(), Some(db_uuid.as_str()));

        let host_borrado = entradas_en(
            &almacen,
            &filtro(Vec::new(), "", Some("no-existe".to_string())),
            None,
            50,
        )
        .expect("host desconocido");
        assert!(host_borrado.is_empty());
        // El uuid del host de la primera entrada se resuelve en la fila.
        assert_eq!(conexiones[0].host_uuid.as_deref(), Some(web_uuid.as_str()));
    }

    #[test]
    fn el_limite_de_pagina_nunca_pasa_de_50() {
        let almacen = almacen();
        let (host_id, _) = host(&almacen, "web-01");
        for numero in 1..=60 {
            anotar(
                &almacen,
                "conexion_abierta",
                host_id,
                &format!("entrada {numero}"),
            );
        }
        let pagina =
            entradas_en(&almacen, &filtro(Vec::new(), "", None), None, 500).expect("página");
        assert_eq!(pagina.len(), 50);
        let cero = entradas_en(&almacen, &filtro(Vec::new(), "", None), None, 0).expect("página");
        assert_eq!(cero.len(), 50);
    }

    fn deliberacion(almacen: &Almacen, host_id: i64, resultado: ResultadoDeliberacion) -> i64 {
        let comprobaciones = ComprobacionesHost {
            host_id,
            host: "web-01".to_string(),
            salud: Veredicto::Aprueba {
                detalle: "hace 2 h".to_string(),
                ms: 12,
            },
            backup: Veredicto::NoActiva,
            tests: Veredicto::Rechaza {
                detalle: "solo en el escritorio".to_string(),
                ms: 3,
            },
        };
        delib_bd::crear(
            almacen.conexion(),
            &NuevaDeliberacion {
                snippet_id: None,
                accion: "reiniciar nginx → web-01".to_string(),
                hosts: vec![(host_id, "web-01".to_string())],
                comprobaciones: vec![comprobaciones],
                resultado,
                bloqueada: resultado != ResultadoDeliberacion::Aprobada,
                motivo: (resultado == ResultadoDeliberacion::Forzada)
                    .then(|| "me lo pide el cliente".to_string()),
                usuario: "móvil (prueba)".to_string(),
            },
        )
        .expect("creando la deliberación")
    }

    fn anotar_deliberacion(almacen: &Almacen, host_id: i64, tipo: &str, id: i64) {
        let registro = delib_bd::obtener(almacen.conexion(), id).expect("deliberación");
        let detalle = deliberacion::detalle_registro(&registro, None);
        anotar(almacen, tipo, host_id, &detalle);
    }

    #[test]
    fn el_detalle_de_una_deliberacion_ensena_sus_comprobaciones_y_si_falta_resultado() {
        let almacen = almacen();
        let (host_id, host_uuid) = host(&almacen, "web-01");
        let id = deliberacion(&almacen, host_id, ResultadoDeliberacion::Aprobada);
        anotar_deliberacion(&almacen, host_id, "deliberacion_aprobada", id);
        let entrada = entradas_en(&almacen, &filtro(Vec::new(), "", None), None, 50)
            .expect("página")
            .remove(0);
        let detalle = detalle_en(&almacen, entrada.id)
            .expect("detalle")
            .expect("la entrada existe");
        let deliberacion = detalle.deliberacion.expect("lleva la deliberación");
        assert_eq!(deliberacion.accion, "reiniciar nginx → web-01");
        assert_eq!(deliberacion.usuario, "móvil (prueba)");
        assert_eq!(deliberacion.resultado, ResultadoDeliberacionMovil::Aprobada);
        assert_eq!(deliberacion.filas.len(), 1);
        assert_eq!(
            deliberacion.filas[0].host_uuid.as_deref(),
            Some(host_uuid.as_str())
        );
        assert_eq!(deliberacion.filas[0].host_nombre, "web-01");
        assert!(matches!(
            deliberacion.filas[0].salud,
            VeredictoMovil::Aprueba { .. }
        ));
        assert_eq!(deliberacion.filas[0].backup, VeredictoMovil::NoActiva);
        assert_eq!((deliberacion.aprobadas, deliberacion.activas), (1, 2));
        // Sin ejecucion_resultado y aprobada: MAGI se cerró durante la ejecución.
        assert!(deliberacion.sin_resultado);
        assert_eq!(deliberacion.ejecucion_resultado, None);

        // Al fijarse el resultado deja de faltar.
        delib_bd::fijar_resultado_ejecucion(almacen.conexion(), id, EjecucionResultado::Parcial)
            .expect("fijando el resultado");
        let detalle = detalle_en(&almacen, entrada.id)
            .expect("detalle")
            .expect("la entrada existe");
        let deliberacion = detalle.deliberacion.expect("lleva la deliberación");
        assert!(!deliberacion.sin_resultado);
        assert_eq!(
            deliberacion.ejecucion_resultado,
            Some(ResultadoEjecucionMovil::Parcial)
        );
    }

    #[test]
    fn una_deliberacion_cancelada_no_echan_en_falta_el_resultado() {
        let almacen = almacen();
        let (host_id, _) = host(&almacen, "web-01");
        let id = deliberacion(&almacen, host_id, ResultadoDeliberacion::Cancelada);
        anotar_deliberacion(&almacen, host_id, "deliberacion_cancelada", id);
        let entrada = entradas_en(&almacen, &filtro(Vec::new(), "", None), None, 50)
            .expect("página")
            .remove(0);
        let detalle = detalle_en(&almacen, entrada.id)
            .expect("detalle")
            .expect("la entrada existe");
        let deliberacion = detalle.deliberacion.expect("lleva la deliberación");
        assert_eq!(
            deliberacion.resultado,
            ResultadoDeliberacionMovil::Cancelada
        );
        assert!(!deliberacion.sin_resultado);
    }

    #[test]
    fn una_entrada_normal_no_lleva_deliberacion_y_un_id_sin_entrada_no_existe() {
        let almacen = almacen();
        let (host_id, _) = host(&almacen, "web-01");
        anotar(&almacen, "conexion_abierta", host_id, "sesión abierta");
        let entrada = entradas_en(&almacen, &filtro(Vec::new(), "", None), None, 50)
            .expect("página")
            .remove(0);
        let detalle = detalle_en(&almacen, entrada.id)
            .expect("detalle")
            .expect("la entrada existe");
        assert!(detalle.deliberacion.is_none());
        assert!(detalle_en(&almacen, 9_999).expect("detalle").is_none());
    }

    #[test]
    fn el_texto_del_filtro_no_interpreta_comodines() {
        let almacen = almacen();
        let (host_id, _) = host(&almacen, "web-01");
        anotar(&almacen, "conexion_abierta", host_id, "100% seguro");
        anotar(&almacen, "conexion_abierta", host_id, "100x seguro");
        let pagina =
            entradas_en(&almacen, &filtro(Vec::new(), "100%", None), None, 50).expect("filtro");
        assert_eq!(pagina.len(), 1);
        assert_eq!(pagina[0].detalle, "100% seguro");
        let barra =
            entradas_en(&almacen, &filtro(Vec::new(), "a_b", None), None, 50).expect("filtro");
        assert!(barra.is_empty());
    }

    #[test]
    fn los_detalles_rfc3339_se_pasan_a_epoca() {
        assert_eq!(epoca("2026-11-09T12:00:00+01:00"), 1_794_222_000);
        assert_eq!(epoca("no es una fecha"), 0);
    }
}
