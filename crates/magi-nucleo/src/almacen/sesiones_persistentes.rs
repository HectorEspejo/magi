//! CRUD de SESIONES_PERSISTENTES (Fase 16, §3.1): las fichas de las sesiones
//! que viven en el tmux propio de MAGI en el host.
//!
//! Las fichas viajan con el inventario como elemento propio
//! `sesion_persistente` (D328): toda escritura de un campo que viaja sube la
//! versión con el reloj híbrido, una ficha nueva nace con uuid, versión y
//! vector, y un borrado deja lápida. Solo los hosts de tipo `ssh` tienen
//! fichas.
//!
//! Lo escribe el cliente, y solo cuando la sesión ya existe en el host (D341):
//! el servidor lee esta tabla y nunca escribe en ella. Borrar una ficha nunca
//! termina la sesión de tmux (el almacén ni siquiera sabe del host), y
//! renombrarla solo cambia la ficha: renombrar la sesión en el host es del
//! servidor, y va antes.
//!
//! La lectura valida cada fila (la pudo escribir otro proceso o llegar por la
//! sincronización) y salta, con un aviso en el log, las que no se pueden
//! interpretar: una fila corrupta no puede dejar sin listar las demás. El
//! aviso nunca lleva el comando ni la nota, y sale una sola vez por fila
//! mientras siga ilegible (Fase 9b: la lista se lee a menudo y una fila
//! corrupta llenaría el log).

use std::collections::HashSet;
use std::sync::{Mutex, PoisonError};

use anyhow::{anyhow, Error, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::almacen::versiones::{self, Tabla};
use crate::escritorio::TipoHost;
use crate::modelo::fecha_ahora;
use crate::sesiones_persistentes::validadores::{
    validar_datos, validar_host, validar_nombre, ErrorFicha,
};
use crate::sesiones_persistentes::{DatosFicha, Ficha, OrigenFicha};

const SELECCION: &str = "SELECT s.id, s.uuid, s.host_id, s.nombre, s.carpeta, s.comando, \
                         s.comando_reanudar, s.nota, s.origen, s.creada_por, s.creada_en, \
                         s.actualizado_en, h.tipo \
                           FROM SESIONES_PERSISTENTES s \
                           JOIN HOSTS h ON h.id = s.host_id";

/// Una fila que no se puede interpretar: su id y por qué.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Ilegible {
    id: i64,
    motivo: String,
}

/// Una fila de la consulta: la ficha, o por qué no se puede interpretar.
type FilaLeida = std::result::Result<Ficha, Ilegible>;

/// Columnas de una fila, en el orden de SELECCION. Una fila que no pasa la
/// validación (o cuyas columnas no tienen el tipo esperado) no es un error de
/// la consulta: se devuelve el motivo para saltarla.
fn mapear(fila: &Row<'_>) -> rusqlite::Result<FilaLeida> {
    let id: i64 = fila.get(0)?;
    match leer_columnas(fila, id) {
        Err(
            error @ (rusqlite::Error::FromSqlConversionFailure(..)
            | rusqlite::Error::InvalidColumnType(..)),
        ) => Ok(Err(Ilegible {
            id,
            motivo: format!("la ficha {id} no se puede leer: {error}"),
        })),
        otro => otro,
    }
}

fn leer_columnas(fila: &Row<'_>, id: i64) -> rusqlite::Result<FilaLeida> {
    let uuid: Option<String> = fila.get(1)?;
    let host_id: i64 = fila.get(2)?;
    let origen: String = fila.get(8)?;
    let tipo_host: String = fila.get(12)?;
    let datos = DatosFicha {
        nombre: fila.get(3)?,
        carpeta: fila.get(4)?,
        comando: fila.get(5)?,
        comando_reanudar: fila.get(6)?,
        nota: fila.get(7)?,
        origen: OrigenFicha::desde_texto(&origen).unwrap_or(OrigenFicha::Manual),
        creada_por: fila.get(9)?,
    };
    let creada_en: String = fila.get(10)?;
    let actualizado_en: String = fila.get(11)?;

    let ilegible = |motivo: String| Ok(Err(Ilegible { id, motivo }));
    let Some(uuid) = uuid.filter(|uuid| !uuid.trim().is_empty()) else {
        return ilegible(format!("la ficha {id} no tiene uuid"));
    };
    if TipoHost::desde_texto(&tipo_host) != Some(TipoHost::Ssh) {
        return ilegible(format!("la ficha {id} cuelga de un host que no es ssh"));
    }
    if OrigenFicha::desde_texto(&origen).is_none() {
        return ilegible(format!("la ficha {id} tiene un origen desconocido"));
    }
    // La misma validación que al escribir, con los campos recortados: una
    // ficha que llegó de otro equipo con una carpeta, un comando o una nota
    // fuera de los límites no se usa.
    let datos = match validar_datos(&datos) {
        Ok(datos) => datos,
        Err(error) => return ilegible(format!("la ficha {id} no es válida: {error}")),
    };
    Ok(Ok(Ficha {
        id,
        uuid,
        host_id,
        nombre: datos.nombre,
        carpeta: datos.carpeta,
        comando: datos.comando,
        comando_reanudar: datos.comando_reanudar,
        nota: datos.nota,
        origen: datos.origen,
        creada_por: datos.creada_por,
        creada_en,
        actualizado_en,
    }))
}

/// Un aviso ya dado de una fila ilegible.
struct Avisada {
    /// Ruta de la base de datos, para no mezclar las de dos `magi.db`.
    base: String,
    id: i64,
    motivo: String,
}

/// El recuerdo de los avisos dados, con un tope de filas.
struct Avisadas {
    filas: Vec<Avisada>,
    maximo: usize,
}

impl Avisadas {
    const fn nuevo(maximo: usize) -> Self {
        Self {
            filas: Vec::new(),
            maximo,
        }
    }

    /// Anota las filas ilegibles de una lectura y devuelve las que todavía no
    /// se habían avisado (o cuyo motivo ha cambiado). Una fila que se vuelve a
    /// leer bien sale del recuerdo: si se rompe otra vez, se avisa otra vez.
    ///
    /// Pasado el tope no se recuerda nada más: las filas de sobra avisan en
    /// cada lectura (son pocas, y un recuerdo que expulsara a las más viejas
    /// haría avisar a todas en cada lectura), pero la memoria no crece sin
    /// límite.
    fn nuevas(
        &mut self,
        base: &str,
        legibles: impl Iterator<Item = i64>,
        ilegibles: Vec<Ilegible>,
    ) -> Vec<Ilegible> {
        if self.filas.iter().any(|avisada| avisada.base == base) {
            let ids: HashSet<i64> = legibles.collect();
            self.filas
                .retain(|avisada| avisada.base != base || !ids.contains(&avisada.id));
        }
        let mut nuevas = Vec::new();
        for ilegible in ilegibles {
            let ya = self
                .filas
                .iter_mut()
                .find(|avisada| avisada.base == base && avisada.id == ilegible.id);
            match ya {
                Some(avisada) if avisada.motivo == ilegible.motivo => {}
                Some(avisada) => {
                    avisada.motivo.clone_from(&ilegible.motivo);
                    nuevas.push(ilegible);
                }
                None => {
                    if self.filas.len() < self.maximo {
                        self.filas.push(Avisada {
                            base: base.to_string(),
                            id: ilegible.id,
                            motivo: ilegible.motivo.clone(),
                        });
                    }
                    nuevas.push(ilegible);
                }
            }
        }
        nuevas
    }
}

/// Tope de filas avisadas que se recuerdan en este proceso.
const MAXIMO_AVISADAS: usize = 256;

/// Las filas ilegibles de las que ya se avisó en este proceso. La lectura vive
/// en el núcleo y la usan procesos distintos (el cliente, la CLI), así que el
/// recuerdo es del propio módulo.
static AVISADAS: Mutex<Avisadas> = Mutex::new(Avisadas::nuevo(MAXIMO_AVISADAS));

/// Las filas ilegibles de una lectura de las que todavía no se ha avisado. Una
/// base en memoria no tiene ruta con la que distinguirla de otra, y no se
/// recuerda nada de ella: avisa siempre.
fn nuevas_ilegibles(
    conexion: &Connection,
    legibles: &[Ficha],
    ilegibles: Vec<Ilegible>,
) -> Vec<Ilegible> {
    let Some(base) = conexion.path().filter(|ruta| !ruta.is_empty()) else {
        return ilegibles;
    };
    AVISADAS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .nuevas(base, legibles.iter().map(|ficha| ficha.id), ilegibles)
}

/// Lee las fichas de una consulta, saltando (con un aviso, una sola vez por
/// fila) las ilegibles. Un error real de SQLite sí se propaga.
fn consultar(
    conexion: &Connection,
    resto: &str,
    parametros: &[&dyn rusqlite::ToSql],
) -> Result<Vec<Ficha>> {
    let mut sentencia = conexion.prepare(&format!("{SELECCION} {resto}"))?;
    let filas = sentencia.query_map(parametros, mapear)?;
    let mut fichas = Vec::new();
    let mut ilegibles = Vec::new();
    for fila in filas {
        match fila? {
            Ok(ficha) => fichas.push(ficha),
            Err(ilegible) => ilegibles.push(ilegible),
        }
    }
    for ilegible in nuevas_ilegibles(conexion, &fichas, ilegibles) {
        tracing::warn!(
            "ficha de sesión persistente ilegible, se salta: {}",
            ilegible.motivo
        );
    }
    Ok(fichas)
}

/// Todas las fichas, ordenadas por host y nombre (el orden de la lista).
pub fn listar(conexion: &Connection) -> Result<Vec<Ficha>> {
    consultar(conexion, "ORDER BY h.nombre, s.nombre", &[])
}

/// Fichas de un host, por nombre.
pub fn de_host(conexion: &Connection, host_id: i64) -> Result<Vec<Ficha>> {
    consultar(
        conexion,
        "WHERE s.host_id = ?1 ORDER BY s.nombre",
        &[&host_id],
    )
}

/// Una ficha por su id. `None` si no existe o no se puede leer.
pub fn obtener(conexion: &Connection, id: i64) -> Result<Option<Ficha>> {
    Ok(consultar(conexion, "WHERE s.id = ?1", &[&id])?
        .into_iter()
        .next())
}

/// Una ficha por su uuid (el que difunde el servidor en la lista y el que
/// llevan fijado los diálogos del cliente). `None` si ya no existe (la pudo
/// borrar otro equipo con la sincronización) o no se puede leer.
pub fn por_uuid(conexion: &Connection, uuid: &str) -> Result<Option<Ficha>> {
    Ok(consultar(conexion, "WHERE s.uuid = ?1", &[&uuid])?
        .into_iter()
        .next())
}

/// La ficha de un host con ese nombre exacto (tmux distingue mayúsculas y
/// casa prefijos en `-t`; aquí nunca).
pub fn por_nombre(conexion: &Connection, host_id: i64, nombre: &str) -> Result<Option<Ficha>> {
    Ok(consultar(
        conexion,
        "WHERE s.host_id = ?1 AND s.nombre = ?2",
        &[&host_id, &nombre],
    )?
    .into_iter()
    .next())
}

/// Nombres de todas las fichas de un host, también las que no se pueden leer
/// por otro motivo: siguen ocupando su nombre (`UNIQUE(host_id, nombre)`) y las
/// terminadas lo reservan para los nombres automáticos.
pub fn nombres_de_host(conexion: &Connection, host_id: i64) -> Result<Vec<String>> {
    let mut sentencia = conexion
        .prepare("SELECT nombre FROM SESIONES_PERSISTENTES WHERE host_id = ?1 ORDER BY nombre")?;
    let filas = sentencia.query_map(params![host_id], |fila| fila.get(0))?;
    Ok(filas.collect::<rusqlite::Result<Vec<String>>>()?)
}

/// Rechaza la ficha si el host no existe o no es `ssh` (T100). El error es un
/// `ErrorFicha` dentro del `anyhow::Error`, para que el diálogo lo reconozca.
pub(crate) fn comprobar_host_ssh(conexion: &Connection, host_id: i64) -> Result<()> {
    let tipo: Option<String> = conexion
        .query_row(
            "SELECT tipo FROM HOSTS WHERE id = ?1",
            params![host_id],
            |fila| fila.get(0),
        )
        .optional()?;
    let Some(tipo) = tipo else {
        return Err(Error::new(ErrorFicha::HostNoExiste { host_id }));
    };
    match TipoHost::desde_texto(&tipo) {
        Some(tipo) => validar_host(tipo).map_err(Error::new),
        None => Err(anyhow!(
            "el host {host_id} es de un tipo desconocido «{tipo}»"
        )),
    }
}

/// Crea la ficha de una sesión que ya existe en el host y devuelve su id.
///
/// Valida los datos, exige un host `ssh` y deja uuid, versión y vector de
/// este equipo. `creada_en` y `actualizado_en` son la hora de ahora.
pub fn crear(conexion: &Connection, host_id: i64, datos: &DatosFicha) -> Result<i64> {
    let datos = validar_datos(datos).map_err(Error::new)?;
    let transaccion = crate::almacen::transaccion(conexion)?;
    comprobar_host_ssh(&transaccion, host_id)?;
    let ahora = fecha_ahora();
    transaccion
        .execute(
            "INSERT INTO SESIONES_PERSISTENTES
                (host_id, nombre, carpeta, comando, comando_reanudar, nota, origen,
                 creada_por, creada_en, actualizado_en)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
            params![
                host_id,
                datos.nombre,
                datos.carpeta,
                datos.comando,
                datos.comando_reanudar,
                datos.nota,
                datos.origen.texto(),
                datos.creada_por,
                ahora,
            ],
        )
        .map_err(|error| nombre_en_uso_o(error, &datos.nombre))?;
    let id = transaccion.last_insert_rowid();
    // Una ficha nueva nace con uuid, versión y vector (Fase 10).
    versiones::registrar_nuevo(&transaccion, Tabla::SesionesPersistentes, id, None)?;
    transaccion.commit()?;
    Ok(id)
}

/// Edita nombre, carpeta, comando, comando de reanudación y nota. Solo sube la
/// versión (y `actualizado_en`) si algo cambió. `origen` y `creada_por` se
/// fijan al crear y no se editan. Renombrar aquí solo cambia la ficha: el
/// nombre de la sesión en el host lo cambia el servidor, antes.
///
/// Compara lo editado antes y después en vez de pasar por
/// `versiones::subir_si_cambio`: así la versión sube igual con independencia
/// de cómo serialice la sincronización los datos de una ficha.
pub fn actualizar(conexion: &Connection, id: i64, datos: &DatosFicha) -> Result<()> {
    let datos = validar_datos(datos).map_err(Error::new)?;
    let transaccion = crate::almacen::transaccion(conexion)?;
    let Some(antes) = obtener(&transaccion, id)? else {
        return Err(Error::new(ErrorFicha::FichaNoExiste { id }));
    };
    if antes.nombre == datos.nombre
        && antes.carpeta == datos.carpeta
        && antes.comando == datos.comando
        && antes.comando_reanudar == datos.comando_reanudar
        && antes.nota == datos.nota
    {
        return Ok(());
    }
    transaccion
        .execute(
            "UPDATE SESIONES_PERSISTENTES
                SET nombre = ?1, carpeta = ?2, comando = ?3, comando_reanudar = ?4,
                    nota = ?5, actualizado_en = ?6
              WHERE id = ?7",
            params![
                datos.nombre,
                datos.carpeta,
                datos.comando,
                datos.comando_reanudar,
                datos.nota,
                fecha_ahora(),
                id,
            ],
        )
        .map_err(|error| nombre_en_uso_o(error, &datos.nombre))?;
    versiones::subir(&transaccion, Tabla::SesionesPersistentes, id)?;
    transaccion.commit()?;
    Ok(())
}

/// Cambia solo el nombre de la ficha (versión nueva). El de la sesión en el
/// host lo cambia el servidor con `RenombrarTmux`; si la sesión ya terminó,
/// solo se renombra la ficha. Mismo nombre: no hace nada.
pub fn renombrar(conexion: &Connection, id: i64, nombre: &str) -> Result<()> {
    let nombre = nombre.trim();
    validar_nombre(nombre).map_err(Error::new)?;
    let transaccion = crate::almacen::transaccion(conexion)?;
    let Some(antes) = obtener(&transaccion, id)? else {
        return Err(Error::new(ErrorFicha::FichaNoExiste { id }));
    };
    if antes.nombre == nombre {
        return Ok(());
    }
    transaccion
        .execute(
            "UPDATE SESIONES_PERSISTENTES SET nombre = ?1, actualizado_en = ?2 WHERE id = ?3",
            params![nombre, fecha_ahora(), id],
        )
        .map_err(|error| nombre_en_uso_o(error, nombre))?;
    versiones::subir(&transaccion, Tabla::SesionesPersistentes, id)?;
    transaccion.commit()?;
    Ok(())
}

/// Borra la ficha dejando su lápida. Nunca termina la sesión del host: si
/// vivía, sigue viva y sin ficha. Una fila sin uuid (que no debería existir)
/// se borra sin lápida: no puede viajar.
pub fn borrar(conexion: &Connection, id: i64) -> Result<()> {
    let transaccion = crate::almacen::transaccion(conexion)?;
    let fila: Option<(Option<String>, Option<String>, String)> = transaccion
        .query_row(
            "SELECT uuid, vector, nombre FROM SESIONES_PERSISTENTES WHERE id = ?1",
            params![id],
            |fila| Ok((fila.get(0)?, fila.get(1)?, fila.get(2)?)),
        )
        .optional()?;
    let Some((uuid, vector, nombre)) = fila else {
        // No hay fila que borrar: ni lápida ni cambios.
        return Ok(());
    };
    if let Some(uuid) = uuid.filter(|uuid| !uuid.trim().is_empty()) {
        crate::almacen::dejar_lapida(
            &transaccion,
            Tabla::SesionesPersistentes,
            &uuid,
            &nombre,
            vector.as_deref().unwrap_or("{}"),
        )?;
    }
    transaccion.execute(
        "DELETE FROM SESIONES_PERSISTENTES WHERE id = ?1",
        params![id],
    )?;
    transaccion.commit()?;
    Ok(())
}

/// Convierte el choque de `UNIQUE(host_id, nombre)` en un error que el
/// diálogo reconoce; cualquier otro error pasa tal cual.
fn nombre_en_uso_o(error: rusqlite::Error, nombre: &str) -> Error {
    if crate::almacen::es_conflicto_unico(&error) {
        Error::new(ErrorFicha::NombreEnUso {
            nombre: nombre.to_string(),
        })
    } else {
        Error::new(error)
    }
}

// ------------------------------------------------------- lectura del vigilante

/// Lo que lee el vigilante del servidor en cada pasada (Fase 16, §4.5): las
/// fichas, los hosts que hay que comprobar y los que hacen falta para los
/// saltos «a través de».
///
/// La lectura valida cada fila antes de entregarla (la pudo escribir otro
/// proceso o llegar por la sincronización) y nunca falla por una sola fila
/// ilegible: la salta y deja su motivo en `ilegibles`, sin escribir nada en el
/// log (el vigilante lee cada minuto y avisa una sola vez por motivo). Los
/// motivos nunca llevan comandos, notas ni carpetas.
#[derive(Debug, Default)]
pub struct LecturaVigilante {
    /// Fichas legibles, en el orden de la lista (host y nombre).
    pub fichas: Vec<Ficha>,
    /// Hosts `ssh` legibles con alguna ficha o con la marca
    /// `sesiones_persistentes`: los que se comprueban, por id.
    pub vigilados: Vec<i64>,
    /// Todos los hosts legibles, por id (la cadena de saltos los necesita).
    pub hosts: std::collections::HashMap<i64, crate::modelo::Host>,
    /// Por qué se saltó cada fila ilegible.
    pub ilegibles: Vec<String>,
}

/// Lee las fichas de una consulta sin avisar en el log: devuelve las legibles
/// y el motivo de cada fila saltada. Un error real de SQLite sí se propaga.
fn consultar_con_motivos(conexion: &Connection) -> Result<(Vec<Ficha>, Vec<String>)> {
    let mut sentencia = conexion.prepare(&format!("{SELECCION} ORDER BY h.nombre, s.nombre"))?;
    let filas = sentencia.query_map([], mapear)?;
    let mut fichas = Vec::new();
    let mut motivos = Vec::new();
    for fila in filas {
        match fila? {
            Ok(ficha) => fichas.push(ficha),
            Err(ilegible) => motivos.push(ilegible.motivo),
        }
    }
    Ok((fichas, motivos))
}

/// Lee lo que necesita una pasada del vigilante: fichas válidas, hosts `ssh`
/// a comprobar y el inventario legible. El servidor solo lee: esta función no
/// escribe nada.
pub fn leer_para_vigilante(conexion: &Connection) -> Result<LecturaVigilante> {
    let (fichas, mut ilegibles) = consultar_con_motivos(conexion)?;

    // Cada host por separado: uno ilegible (un tipo que este binario no
    // entiende, unas opciones corruptas) no deja sin comprobar a los demás.
    let ids: Vec<i64> = {
        let mut sentencia = conexion.prepare("SELECT id FROM HOSTS ORDER BY id")?;
        let filas = sentencia.query_map([], |fila| fila.get::<_, i64>(0))?;
        filas.collect::<rusqlite::Result<Vec<i64>>>()?
    };
    let mut hosts = std::collections::HashMap::new();
    for id in ids {
        match crate::almacen::hosts::obtener(conexion, id) {
            Ok(host) => {
                hosts.insert(id, host);
            }
            Err(error) => ilegibles.push(format!("el host {id} no se puede leer: {error}")),
        }
    }

    let mut vigilados: Vec<i64> = hosts
        .values()
        .filter(|host| host.tipo == TipoHost::Ssh)
        .filter(|host| {
            host.sesiones_persistentes || fichas.iter().any(|ficha| ficha.host_id == host.id)
        })
        .map(|host| host.id)
        .collect();
    vigilados.sort_unstable();

    // Una ficha cuyo host no se pudo leer no se comprueba: se queda en la
    // lista, «sin comprobar».
    Ok(LecturaVigilante {
        fichas,
        vigilados,
        hosts,
        ilegibles,
    })
}

#[cfg(test)]
mod pruebas_vigilante {
    use super::*;
    use crate::almacen::Almacen;
    use crate::modelo::{DatosHost, Origen};

    fn host(almacen: &Almacen, nombre: &str, tipo: TipoHost) -> i64 {
        almacen
            .crear_host(
                &DatosHost {
                    nombre: nombre.to_string(),
                    direccion: "192.0.2.10".to_string(),
                    tipo,
                    usuario: Some("usuario".to_string()),
                    ..DatosHost::default()
                },
                Origen::Manual,
            )
            .expect("host")
    }

    fn datos(nombre: &str) -> DatosFicha {
        DatosFicha {
            nombre: nombre.to_string(),
            carpeta: None,
            comando: Some("claude".to_string()),
            comando_reanudar: None,
            nota: None,
            origen: OrigenFicha::Manual,
            creada_por: "portatil".to_string(),
        }
    }

    #[test]
    fn el_vigilante_solo_vigila_los_hosts_ssh_con_fichas_o_marcados() {
        let almacen = Almacen::abrir_en_memoria().unwrap();
        let con_ficha = host(&almacen, "con-ficha", TipoHost::Ssh);
        let marcado = host(&almacen, "marcado", TipoHost::Ssh);
        let otro = host(&almacen, "otro", TipoHost::Ssh);
        let escritorio = host(&almacen, "escritorio", TipoHost::Rdp);
        crear(almacen.conexion(), con_ficha, &datos("api")).unwrap();
        crate::almacen::hosts::fijar_sesiones_persistentes(almacen.conexion(), marcado, true)
            .unwrap();

        let lectura = leer_para_vigilante(almacen.conexion()).unwrap();
        assert_eq!(lectura.vigilados, vec![con_ficha, marcado]);
        assert!(!lectura.vigilados.contains(&otro));
        assert!(!lectura.vigilados.contains(&escritorio));
        assert_eq!(lectura.fichas.len(), 1);
        // El inventario entero está para resolver los saltos.
        assert_eq!(lectura.hosts.len(), 4);
        assert!(lectura.ilegibles.is_empty());
    }

    #[test]
    fn una_fila_ilegible_se_salta_con_su_motivo_y_sin_llevar_comandos() {
        let almacen = Almacen::abrir_en_memoria().unwrap();
        let sano = host(&almacen, "sano", TipoHost::Ssh);
        let roto = host(&almacen, "roto", TipoHost::Ssh);
        crear(almacen.conexion(), sano, &datos("api")).unwrap();
        let ficha_rota = crear(almacen.conexion(), roto, &datos("web")).unwrap();
        // Una ficha con un origen que este binario no conoce y un host de un
        // tipo desconocido: las filas las pudo escribir otro proceso.
        almacen
            .conexion()
            .execute(
                "UPDATE SESIONES_PERSISTENTES SET origen = 'raro' WHERE id = ?1",
                [ficha_rota],
            )
            .unwrap();
        let tipo_raro = host(&almacen, "tipo-raro", TipoHost::Ssh);
        almacen
            .conexion()
            .execute("UPDATE HOSTS SET tipo = 'raro' WHERE id = ?1", [tipo_raro])
            .unwrap();

        let lectura = leer_para_vigilante(almacen.conexion()).unwrap();
        assert_eq!(lectura.fichas.len(), 1, "solo la ficha sana");
        assert_eq!(lectura.fichas[0].nombre, "api");
        // El host sano se vigila; el de la ficha ilegible (sin marca ni otra
        // ficha) y el de tipo raro, no.
        assert!(!lectura.vigilados.contains(&roto));
        assert!(lectura.vigilados.contains(&sano));
        assert!(!lectura.vigilados.contains(&tipo_raro));
        assert!(!lectura.hosts.contains_key(&tipo_raro));
        assert_eq!(lectura.ilegibles.len(), 2, "{:?}", lectura.ilegibles);
        assert!(
            lectura
                .ilegibles
                .iter()
                .all(|motivo| !motivo.contains("claude")),
            "un motivo lleva el comando: {:?}",
            lectura.ilegibles
        );
    }
}

#[cfg(test)]
mod pruebas_avisos {
    use super::*;

    fn ilegible(id: i64, motivo: &str) -> Ilegible {
        Ilegible {
            id,
            motivo: motivo.to_string(),
        }
    }

    #[test]
    fn avisa_una_vez_por_fila_y_olvida_las_que_se_arreglan() {
        let mut avisadas = Avisadas::nuevo(8);
        let rotas = || vec![ilegible(1, "sin uuid"), ilegible(2, "sin uuid")];
        assert_eq!(avisadas.nuevas("a.db", [].into_iter(), rotas()).len(), 2);
        assert!(avisadas.nuevas("a.db", [].into_iter(), rotas()).is_empty());
        // La 1 se lee bien: se olvida. La 2 sigue avisada.
        assert!(avisadas
            .nuevas("a.db", [1].into_iter(), vec![ilegible(2, "sin uuid")])
            .is_empty());
        let otra_vez = avisadas.nuevas("a.db", [].into_iter(), rotas());
        assert_eq!(otra_vez, vec![ilegible(1, "sin uuid")]);
    }

    #[test]
    fn un_motivo_nuevo_o_otra_base_avisan_de_nuevo() {
        let mut avisadas = Avisadas::nuevo(8);
        avisadas.nuevas("a.db", [].into_iter(), vec![ilegible(1, "sin uuid")]);
        assert_eq!(
            avisadas
                .nuevas("a.db", [].into_iter(), vec![ilegible(1, "origen raro")])
                .len(),
            1
        );
        assert_eq!(
            avisadas
                .nuevas("b.db", [].into_iter(), vec![ilegible(1, "origen raro")])
                .len(),
            1,
            "la misma fila de otra base no está avisada"
        );
    }

    #[test]
    fn pasado_el_tope_no_recuerda_mas_ni_crece() {
        let mut avisadas = Avisadas::nuevo(3);
        let rotas: Vec<Ilegible> = (1..=5).map(|id| ilegible(id, "sin uuid")).collect();
        assert_eq!(
            avisadas.nuevas("a.db", [].into_iter(), rotas.clone()).len(),
            5
        );
        assert_eq!(avisadas.filas.len(), 3, "recuerda hasta el tope");
        // Las tres recordadas callan; las dos que no cupieron avisan siempre.
        let repetidas = avisadas.nuevas("a.db", [].into_iter(), rotas);
        assert_eq!(
            repetidas.iter().map(|fila| fila.id).collect::<Vec<_>>(),
            vec![4, 5]
        );
        assert_eq!(avisadas.filas.len(), 3);
        // Arreglar una deja sitio a la siguiente que se rompa.
        avisadas.nuevas("a.db", [1].into_iter(), Vec::new());
        assert_eq!(avisadas.filas.len(), 2);
        avisadas.nuevas("a.db", [].into_iter(), vec![ilegible(4, "sin uuid")]);
        assert_eq!(avisadas.filas.len(), 3);
    }
}
