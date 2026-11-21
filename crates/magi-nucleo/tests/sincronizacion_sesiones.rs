//! Pruebas de la sincronización de las sesiones persistentes (Fase 16, §3.1,
//! §3.2, §4.9, §7.5 y §7.6): el `formato` 4 solo cuando toca, el elemento
//! `sesion_persistente` y la marca `sesiones_persistentes` del host, la unión de
//! fichas con el mismo host y nombre (D337), las lápidas, las fichas huérfanas
//! y un lector con el máximo anterior.
//!
//! No usan el CRUD de fichas del almacén: insertan filas con SQL y los
//! ayudantes de versiones, como las pruebas de comprobaciones, para probar la
//! sincronización sola. Todo va contra almacenes en memoria o bases temporales
//! con una carpeta compartida de verdad; no se abre ninguna conexión.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::params;
use serde_json::json;

use magi_nucleo::almacen::lapidas;
use magi_nucleo::almacen::versiones::{self, formatear_version, Tabla};
use magi_nucleo::almacen::{conflictos, equipos as equipos_bd, Almacen};
use magi_nucleo::configuracion::SeccionSincronizacion;
use magi_nucleo::escritorio::TipoHost;
use magi_nucleo::llavero::{LlaveroEnMemoria, LlaveroSecreto, CUENTA_SINCRONIZACION};
use magi_nucleo::modelo::{DatosHost, Origen};
use magi_nucleo::rutas::{Equipo, Rutas};
use magi_nucleo::sincronizacion::aplicacion::{
    aplicar, unir_por_nombre, OpcionesAplicacion, ResumenAplicacion,
};
use magi_nucleo::sincronizacion::carpeta;
use magi_nucleo::sincronizacion::cifrado::ClaveSincronizacion;
use magi_nucleo::sincronizacion::documento_propio;
use magi_nucleo::sincronizacion::elementos::{estado_local, formato_a_escribir};
use magi_nucleo::sincronizacion::formato::{
    self, Documento, ElementoDocumento, EquipoDocumento, LapidaDocumento,
};
use magi_nucleo::sincronizacion::fusion::{fusionar, Candidato, PlanFusion};
use magi_nucleo::sincronizacion::{self, EstadoPasada, OpcionesPasada, PoliticaBorrados};

/// Hogar de pruebas: el JSON canónico compara rutas bajo este hogar.
const HOGAR: &str = "/home/pruebas-magi";

/// uuid del equipo ajeno que firma los documentos de prueba.
const EQUIPO_AJENO: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";

/// uuid del host remoto que usan los documentos de prueba.
const UUID_HOST_REMOTO: &str = "99999999-aaaa-4aaa-8aaa-999999999999";

/// Fichas con el uuid ordenado: `UUID_MENOR` gana siempre una unión.
const UUID_MENOR: &str = "11111111-1111-4111-8111-111111111111";
const UUID_MEDIO: &str = "55555555-5555-4555-8555-555555555555";
const UUID_MAYOR: &str = "ffffffff-ffff-4fff-8fff-ffffffffffff";

// --------------------------------------------------------------- utilidades

fn almacen() -> Almacen {
    Almacen::abrir_en_memoria().expect("almacén en memoria")
}

fn crear_host(almacen: &Almacen, nombre: &str) -> i64 {
    almacen
        .crear_host(
            &DatosHost {
                nombre: nombre.to_string(),
                direccion: "192.0.2.10".to_string(),
                usuario: Some("usuario".to_string()),
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .expect("host ssh")
}

fn crear_host_escritorio(almacen: &Almacen, nombre: &str) -> i64 {
    almacen
        .crear_host(
            &DatosHost {
                nombre: nombre.to_string(),
                direccion: "192.0.2.30".to_string(),
                puerto: 3389,
                tipo: TipoHost::Rdp,
                usuario: Some("operador".to_string()),
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .expect("host de escritorio")
}

/// uuid, versión y vector de una fila de una tabla que viaja.
fn fila_versionada(almacen: &Almacen, tabla: Tabla, id: i64) -> (String, String, String) {
    almacen
        .conexion()
        .query_row(
            &format!(
                "SELECT uuid, version, vector FROM {} WHERE id = ?1",
                tabla.nombre()
            ),
            params![id],
            |fila| Ok((fila.get(0)?, fila.get(1)?, fila.get(2)?)),
        )
        .expect("leyendo uuid, versión y vector")
}

fn uuid_host(almacen: &Almacen, id: i64) -> String {
    fila_versionada(almacen, Tabla::Hosts, id).0
}

fn ms_de(version: &str) -> i64 {
    versiones::ms_de_version(version).expect("versión con la forma esperada")
}

fn version_ajena(ms: i64) -> String {
    formatear_version(ms, EQUIPO_AJENO)
}

fn vector_ajeno(ms: i64) -> BTreeMap<String, i64> {
    [(EQUIPO_AJENO.to_string(), ms)].into_iter().collect()
}

/// Lo que cambia entre las fichas de prueba; el resto es igual.
#[derive(Clone, Copy, Default)]
struct Campos<'a> {
    carpeta: Option<&'a str>,
    comando: Option<&'a str>,
    comando_reanudar: Option<&'a str>,
    nota: Option<&'a str>,
}

/// Inserta una ficha con SQL y le da uuid, versión y vector con los ayudantes
/// de versiones (sin el CRUD del almacén). `uuid` fija el uuid.
fn insertar_ficha(
    almacen: &Almacen,
    host_id: i64,
    nombre: &str,
    campos: Campos<'_>,
    uuid: Option<&str>,
) -> i64 {
    let ahora = magi_nucleo::modelo::fecha_ahora();
    almacen
        .conexion()
        .execute(
            "INSERT INTO SESIONES_PERSISTENTES (
                host_id, nombre, carpeta, comando, comando_reanudar, nota, origen,
                creada_por, creada_en, actualizado_en
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'manual', 'sobremesa', ?7, ?7)",
            params![
                host_id,
                nombre,
                campos.carpeta,
                campos.comando,
                campos.comando_reanudar,
                campos.nota,
                ahora
            ],
        )
        .expect("insertando la ficha");
    let id = almacen.conexion().last_insert_rowid();
    versiones::registrar_nuevo(almacen.conexion(), Tabla::SesionesPersistentes, id, uuid)
        .expect("uuid, versión y vector de la ficha");
    id
}

/// Edita el comando de una ficha con SQL y sube su versión.
fn editar_comando(almacen: &Almacen, id: i64, comando: &str) {
    almacen
        .conexion()
        .execute(
            "UPDATE SESIONES_PERSISTENTES SET comando = ?1 WHERE id = ?2",
            params![comando, id],
        )
        .expect("editando la ficha");
    versiones::subir(almacen.conexion(), Tabla::SesionesPersistentes, id)
        .expect("subiendo la versión de la ficha");
}

/// Borra una ficha dejando su lápida, como lo hace el almacén: una versión
/// nueva sobre el vector anterior, antes del DELETE.
fn borrar_ficha(almacen: &Almacen, id: i64) {
    let (uuid, nombre, vector): (String, String, String) = almacen
        .conexion()
        .query_row(
            "SELECT uuid, nombre, vector FROM SESIONES_PERSISTENTES WHERE id = ?1",
            params![id],
            |fila| Ok((fila.get(0)?, fila.get(1)?, fila.get(2)?)),
        )
        .expect("leyendo la ficha que se borra");
    let (version, vector) =
        versiones::version_de_lapida(almacen.conexion(), &vector).expect("versión de la lápida");
    lapidas::guardar(
        almacen.conexion(),
        &lapidas::Lapida {
            uuid,
            tipo: "sesion_persistente".to_string(),
            nombre,
            version,
            vector,
            borrado_en: magi_nucleo::modelo::fecha_ahora(),
        },
    )
    .expect("guardando la lápida");
    almacen
        .conexion()
        .execute(
            "DELETE FROM SESIONES_PERSISTENTES WHERE id = ?1",
            params![id],
        )
        .expect("borrando la ficha");
}

/// Marca o desmarca un host y sube su versión (la marca viaja).
fn marcar_host(almacen: &Almacen, id: i64, marcado: bool) {
    almacen
        .conexion()
        .execute(
            "UPDATE HOSTS SET sesiones_persistentes = ?1 WHERE id = ?2",
            params![marcado as i64, id],
        )
        .expect("marcando el host");
    versiones::subir(almacen.conexion(), Tabla::Hosts, id).expect("subiendo la versión del host");
}

/// Una ficha tal como la ve la prueba: lo que viaja, más su versión.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FilaFicha {
    uuid: String,
    host_uuid: String,
    nombre: String,
    carpeta: Option<String>,
    comando: Option<String>,
    comando_reanudar: Option<String>,
    nota: Option<String>,
    origen: String,
    creada_por: String,
    version: String,
    vector: String,
}

fn fichas(conexion: &rusqlite::Connection) -> Vec<FilaFicha> {
    let mut sentencia = conexion
        .prepare(
            "SELECT s.uuid, h.uuid, s.nombre, s.carpeta, s.comando, s.comando_reanudar,
                    s.nota, s.origen, s.creada_por, s.version, s.vector
               FROM SESIONES_PERSISTENTES s JOIN HOSTS h ON h.id = s.host_id
              ORDER BY s.uuid",
        )
        .expect("preparando la lectura de fichas");
    let filas = sentencia
        .query_map([], |fila| {
            Ok(FilaFicha {
                uuid: fila.get(0)?,
                host_uuid: fila.get(1)?,
                nombre: fila.get(2)?,
                carpeta: fila.get(3)?,
                comando: fila.get(4)?,
                comando_reanudar: fila.get(5)?,
                nota: fila.get(6)?,
                origen: fila.get(7)?,
                creada_por: fila.get(8)?,
                version: fila.get(9)?,
                vector: fila.get(10)?,
            })
        })
        .expect("leyendo las fichas");
    filas
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("fichas legibles")
}

fn contar_fichas(conexion: &rusqlite::Connection) -> i64 {
    conexion
        .query_row("SELECT COUNT(*) FROM SESIONES_PERSISTENTES", [], |fila| {
            fila.get(0)
        })
        .expect("contando fichas")
}

fn host_marcado(conexion: &rusqlite::Connection, uuid: &str) -> bool {
    conexion
        .query_row(
            "SELECT sesiones_persistentes FROM HOSTS WHERE uuid = ?1",
            params![uuid],
            |fila| fila.get::<_, i64>(0),
        )
        .expect("leyendo la marca del host")
        != 0
}

// ---------------------------------------------------- documentos de otros equipos

/// Datos que viajan de un host de otro equipo (formato 4, con o sin marca).
fn datos_host_documento(nombre: &str, tipo: TipoHost, marcado: bool) -> serde_json::Value {
    serde_json::to_value(magi_nucleo::sincronizacion::elementos::DatosHost {
        nombre: nombre.to_string(),
        direccion: "192.0.2.77".to_string(),
        puerto: 22,
        tipo,
        escritorio_dominio: None,
        escritorio_opciones: json!({}),
        usuario: None,
        identidad_ref: None,
        salto: None,
        grupo: None,
        multiplexar: false,
        keepalive_seg: None,
        opciones_extra: String::new(),
        servicios: String::new(),
        origen: "manual".to_string(),
        etiquetas: Vec::new(),
        verificaciones: None,
        snippet_al_conectar: None,
        sesiones_persistentes: marcado,
    })
    .expect("datos de host a JSON")
}

fn elemento_host(
    uuid: &str,
    nombre: &str,
    tipo: TipoHost,
    marcado: bool,
    ms: i64,
) -> ElementoDocumento {
    ElementoDocumento {
        tipo: "host".to_string(),
        uuid: uuid.to_string(),
        version: version_ajena(ms),
        vector: vector_ajeno(ms),
        datos: datos_host_documento(nombre, tipo, marcado),
    }
}

fn elemento_ficha(
    uuid: &str,
    host: &str,
    nombre: &str,
    campos: Campos<'_>,
    ms: i64,
) -> ElementoDocumento {
    ElementoDocumento {
        tipo: "sesion_persistente".to_string(),
        uuid: uuid.to_string(),
        version: version_ajena(ms),
        vector: vector_ajeno(ms),
        datos: json!({
            "host": host,
            "nombre": nombre,
            "carpeta": campos.carpeta,
            "comando": campos.comando,
            "comando_reanudar": campos.comando_reanudar,
            "nota": campos.nota,
            "origen": "manual",
            "creada_por": "portatil",
        }),
    }
}

fn lapida_ficha(uuid: &str, nombre: &str, ms: i64) -> LapidaDocumento {
    LapidaDocumento {
        tipo: "sesion_persistente".to_string(),
        uuid: uuid.to_string(),
        nombre: nombre.to_string(),
        version: version_ajena(ms),
        vector: vector_ajeno(ms),
        borrado_en: "2026-11-16T10:00:00+01:00".to_string(),
    }
}

fn documento_con(
    equipo: &str,
    formato: u32,
    elementos: Vec<ElementoDocumento>,
    lapidas: Vec<LapidaDocumento>,
) -> Documento {
    Documento {
        formato,
        app: "1.5.0".to_string(),
        equipo: EquipoDocumento {
            uuid: equipo.to_string(),
            nombre: "equipo-ajeno".to_string(),
            plataforma: "linux".to_string(),
        },
        generado_en: "2026-11-16T10:00:00+01:00".to_string(),
        reloj_ms: 1_000,
        elementos,
        lapidas,
        huellas: Vec::new(),
        equipos_quitados: Vec::new(),
    }
}

/// Fusiona los documentos con el estado local y aplica el plan, como una
/// pasada de sincronización (sin cifrado ni carpeta).
fn fusionar_y_aplicar(
    almacen: &Almacen,
    documentos: &[Documento],
) -> (PlanFusion, ResumenAplicacion) {
    let local = estado_local(almacen.conexion(), Path::new(HOGAR)).expect("estado local");
    let plan = fusionar(&local, documentos).expect("fusión");
    let resumen = aplicar(
        almacen.conexion(),
        &plan,
        &OpcionesAplicacion::default(),
        Path::new(HOGAR),
    )
    .expect("aplicando el plan");
    (plan, resumen)
}

// ----------------------------------------------------- dos equipos de verdad

/// Un MAGI de prueba con su `magi.db`, sus rutas y su clave, que sincroniza con
/// una carpeta compartida de verdad.
struct Entorno {
    _temporal: tempfile::TempDir,
    rutas: Rutas,
    almacen: Almacen,
    carpeta: PathBuf,
    texto_clave: String,
}

impl Entorno {
    fn nuevo(carpeta: &Path, nombre: &str, texto_clave: &str) -> Self {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let raiz = temporal.path();
        let rutas = Rutas {
            datos: raiz.join("datos"),
            config: raiz.join("config"),
            estado: raiz.join("estado"),
            hogar: raiz.join("hogar"),
            runtime: raiz.join("runtime"),
        };
        std::fs::create_dir_all(rutas.dir_ssh()).expect("creando ~/.ssh");
        std::fs::write(rutas.fichero_known_hosts(), b"").expect("known_hosts vacío");
        let almacen = Almacen::abrir(
            &rutas.base_datos(),
            &Equipo::nuevo(nombre, "linux"),
            "1.5.0",
        )
        .expect("creando magi.db");
        let llavero = LlaveroEnMemoria::nuevo();
        llavero
            .guardar(CUENTA_SINCRONIZACION, texto_clave)
            .expect("guardando la clave de sincronización");
        Self {
            _temporal: temporal,
            rutas,
            almacen,
            carpeta: carpeta.to_path_buf(),
            texto_clave: texto_clave.to_string(),
        }
    }

    fn conexion(&self) -> &rusqlite::Connection {
        self.almacen.conexion()
    }

    fn uuid_este(&self) -> String {
        equipos_bd::este(self.conexion())
            .expect("leyendo este equipo")
            .uuid
    }

    /// Una pasada en modo escritor: lee, fusiona, aplica y exporta.
    fn pasada(&self) {
        let ajustes = SeccionSincronizacion {
            carpeta: Some(self.carpeta.to_string_lossy().to_string()),
            umbral_borrado_hosts: 1000,
            umbral_borrado_pct: 1000,
            ..SeccionSincronizacion::default()
        };
        let clave = ClaveSincronizacion::desde_texto(&self.texto_clave).expect("clave de prueba");
        let resultado = sincronizacion::pasada(
            self.conexion(),
            &self.rutas,
            &ajustes,
            "1.5.0",
            &[clave],
            &OpcionesPasada::sin_trabajo_vivo(PoliticaBorrados::Preguntar),
        )
        .expect("la pasada falla");
        assert_eq!(resultado.estado, EstadoPasada::AlDia, "{resultado:?}");
    }

    /// El número de formato del fichero propio tal como está en la carpeta.
    fn formato_del_fichero_propio(&self) -> u32 {
        let ruta = self
            .carpeta
            .join(carpeta::nombre_fichero(&self.uuid_este()));
        let bytes = std::fs::read(&ruta).expect("leyendo el fichero propio");
        let clave = ClaveSincronizacion::desde_texto(&self.texto_clave).expect("clave de prueba");
        let plano = clave.descifrar(&bytes).expect("descifrando el fichero");
        formato::separar(&plano)
            .expect("separando el fichero")
            .formato
    }
}

fn carpeta_compartida() -> (tempfile::TempDir, PathBuf) {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let carpeta = temporal.path().join("sync");
    std::fs::create_dir_all(&carpeta).expect("creando la carpeta compartida");
    (temporal, carpeta)
}

/// Dos equipos que ya comparten un host `web`: el primero lo crea y los dos
/// sincronizan. Devuelve la carpeta, los dos entornos y el uuid del host.
fn dos_equipos_con_el_mismo_host() -> (tempfile::TempDir, Entorno, Entorno, String) {
    let (temporal, carpeta) = carpeta_compartida();
    let clave = ClaveSincronizacion::generar().texto().to_string();
    let a = Entorno::nuevo(&carpeta, "sobremesa", &clave);
    let b = Entorno::nuevo(&carpeta, "portatil", &clave);
    let host_a = crear_host(&a.almacen, "web");
    let uuid = uuid_host(&a.almacen, host_a);
    a.pasada();
    b.pasada();
    let host_b: i64 = b
        .conexion()
        .query_row("SELECT id FROM HOSTS WHERE uuid = ?1", params![uuid], |f| {
            f.get(0)
        })
        .expect("el host llega al segundo equipo");
    assert_eq!(uuid_host(&b.almacen, host_b), uuid);
    (temporal, a, b, uuid)
}

fn host_id_por_uuid(entorno: &Entorno, uuid: &str) -> i64 {
    entorno
        .conexion()
        .query_row("SELECT id FROM HOSTS WHERE uuid = ?1", params![uuid], |f| {
            f.get(0)
        })
        .expect("host por uuid")
}

// ------------------------------------------------------- el formato 4 solo cuando toca

#[test]
fn el_formato_sube_a_4_solo_con_fichas_o_hosts_marcados() {
    let almacen = almacen();
    assert_eq!(
        formato_a_escribir(almacen.conexion()).expect("formato vacío"),
        formato::FORMATO_1
    );
    let host = crear_host(&almacen, "web");
    assert_eq!(
        formato_a_escribir(almacen.conexion()).expect("formato con solo SSH"),
        formato::FORMATO_1,
        "un inventario sin fichas ni hosts marcados no pasa al formato 4"
    );
    crear_host_escritorio(&almacen, "win-01");
    assert_eq!(
        formato_a_escribir(almacen.conexion()).expect("formato con escritorio"),
        formato::FORMATO_2
    );
    almacen
        .guardar_comprobacion(
            host,
            magi_nucleo::servicios::TipoObjetivo::Unidad,
            "nginx.service",
            "nginx -t",
        )
        .expect("comprobación propia");
    assert_eq!(
        formato_a_escribir(almacen.conexion()).expect("formato con comprobaciones"),
        formato::FORMATO_3,
        "una comprobación propia sigue dando el formato 3"
    );

    // Con una ficha, sí.
    let ficha = insertar_ficha(&almacen, host, "agente", Campos::default(), None);
    assert_eq!(
        formato_a_escribir(almacen.conexion()).expect("formato con una ficha"),
        formato::FORMATO_4
    );
    // Sin la ficha, vuelve al que tocaba.
    borrar_ficha(&almacen, ficha);
    assert_eq!(
        formato_a_escribir(almacen.conexion()).expect("formato sin ficha"),
        formato::FORMATO_3
    );

    // Un host marcado, sin ninguna ficha, también.
    marcar_host(&almacen, host, true);
    assert_eq!(
        formato_a_escribir(almacen.conexion()).expect("formato con un host marcado"),
        formato::FORMATO_4
    );
    marcar_host(&almacen, host, false);
    assert_eq!(
        formato_a_escribir(almacen.conexion()).expect("formato sin la marca"),
        formato::FORMATO_3
    );
}

#[test]
fn el_fichero_propio_solo_dice_formato_4_con_fichas_o_marca() {
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = ClaveSincronizacion::generar().texto().to_string();
    let equipo = Entorno::nuevo(&carpeta, "sobremesa", &clave);
    let host = crear_host(&equipo.almacen, "web");
    equipo.pasada();
    assert_eq!(
        equipo.formato_del_fichero_propio(),
        formato::FORMATO_1,
        "sin fichas ni marca, el fichero sigue siendo del formato 1"
    );

    insertar_ficha(&equipo.almacen, host, "agente", Campos::default(), None);
    equipo.pasada();
    assert_eq!(equipo.formato_del_fichero_propio(), formato::FORMATO_4);
}

#[test]
fn la_exportacion_lleva_la_ficha_sin_fechas_ni_estado() {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let raiz = temporal.path();
    let rutas = Rutas::nuevas(
        raiz.join("datos"),
        raiz.join("config"),
        raiz.join("estado"),
        raiz.join("hogar"),
        raiz.join("runtime"),
    );
    std::fs::create_dir_all(rutas.dir_ssh()).expect("creando ~/.ssh");
    std::fs::write(rutas.fichero_known_hosts(), b"").expect("known_hosts vacío");

    let almacen = almacen();
    let host = crear_host(&almacen, "web");
    let host_uuid = uuid_host(&almacen, host);
    let id = insertar_ficha(
        &almacen,
        host,
        "agente",
        Campos {
            carpeta: Some("~/proyectos/api"),
            comando: Some("claude"),
            comando_reanudar: Some("claude --continue"),
            nota: Some("el agente de la API"),
        },
        None,
    );
    let (uuid_ficha, version, _) = fila_versionada(&almacen, Tabla::SesionesPersistentes, id);

    let documento = documento_propio(almacen.conexion(), &rutas, "1.5.0").expect("documento");
    assert_eq!(documento.formato, formato::FORMATO_4);
    let elemento = documento
        .elementos
        .iter()
        .find(|elemento| elemento.tipo == "sesion_persistente")
        .expect("el elemento sesion_persistente viaja");
    assert_eq!(elemento.uuid, uuid_ficha);
    assert_eq!(elemento.version, version);
    assert!(!elemento.vector.is_empty(), "el elemento lleva vector");
    assert_eq!(
        elemento.datos,
        json!({
            "host": host_uuid,
            "nombre": "agente",
            "carpeta": "~/proyectos/api",
            "comando": "claude",
            "comando_reanudar": "claude --continue",
            "nota": "el agente de la API",
            "origen": "manual",
            "creada_por": "sobremesa",
        }),
        "viajan los campos de la ficha y nada más: ni fechas ni estado"
    );

    // Un campo vacío viaja como null, lo mismo que si falta.
    almacen
        .conexion()
        .execute(
            "UPDATE SESIONES_PERSISTENTES SET nota = '' WHERE id = ?1",
            params![id],
        )
        .expect("nota vacía");
    let documento = documento_propio(almacen.conexion(), &rutas, "1.5.0").expect("documento");
    let elemento = documento
        .elementos
        .iter()
        .find(|elemento| elemento.tipo == "sesion_persistente")
        .expect("la ficha viaja");
    assert!(elemento.datos["nota"].is_null(), "{}", elemento.datos);
}

// ------------------------------------------------------------- la marca del host

#[test]
fn la_marca_del_host_viaja_y_un_host_sin_marca_no_cambia_su_json() {
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = ClaveSincronizacion::generar().texto().to_string();
    let a = Entorno::nuevo(&carpeta, "sobremesa", &clave);
    let b = Entorno::nuevo(&carpeta, "portatil", &clave);
    let host = crear_host(&a.almacen, "web");
    let host_uuid = uuid_host(&a.almacen, host);

    // Sin marca: el JSON del host no trae el campo y el fichero es del formato 1.
    let documento = documento_propio(a.conexion(), &a.rutas, "1.5.0").expect("documento");
    assert_eq!(documento.formato, formato::FORMATO_1);
    let elemento = documento
        .elementos
        .iter()
        .find(|elemento| elemento.tipo == "host")
        .expect("el host viaja");
    assert!(
        elemento.datos.get("sesiones_persistentes").is_none(),
        "un host sin la marca no cambia su JSON canónico: {}",
        elemento.datos
    );
    let version_sin_marca = fila_versionada(&a.almacen, Tabla::Hosts, host).1;

    // Marcado: viaja, sube la versión y el fichero pasa al formato 4.
    marcar_host(&a.almacen, host, true);
    let version_marcado = fila_versionada(&a.almacen, Tabla::Hosts, host).1;
    assert!(ms_de(&version_marcado) > ms_de(&version_sin_marca));
    let documento = documento_propio(a.conexion(), &a.rutas, "1.5.0").expect("documento");
    assert_eq!(documento.formato, formato::FORMATO_4);
    let elemento = documento
        .elementos
        .iter()
        .find(|elemento| elemento.tipo == "host")
        .expect("el host viaja");
    assert_eq!(elemento.datos["sesiones_persistentes"], json!(true));

    // El otro equipo recibe la marca con la versión tal cual.
    a.pasada();
    b.pasada();
    assert!(host_marcado(b.conexion(), &host_uuid), "la marca llega");
    let (_, version_b, _): (String, String, String) = b
        .conexion()
        .query_row(
            "SELECT uuid, version, vector FROM HOSTS WHERE uuid = ?1",
            params![host_uuid],
            |fila| Ok((fila.get(0)?, fila.get(1)?, fila.get(2)?)),
        )
        .expect("host en el otro equipo");
    assert_eq!(version_b, version_marcado, "la versión se escribe tal cual");

    // Y desmarcarla también viaja.
    marcar_host(&a.almacen, host, false);
    a.pasada();
    b.pasada();
    assert!(!host_marcado(b.conexion(), &host_uuid), "la marca se quita");
}

#[test]
fn un_host_de_un_fichero_anterior_al_4_no_trae_la_marca_y_cuenta_como_desmarcado() {
    let almacen = almacen();
    // El host de un fichero del formato 1: su JSON no tiene el campo.
    let mut datos = datos_host_documento("web", TipoHost::Ssh, false);
    assert!(datos.get("sesiones_persistentes").is_none());
    datos
        .as_object_mut()
        .expect("objeto")
        .remove("sesiones_persistentes");
    let documento = documento_con(
        EQUIPO_AJENO,
        1,
        vec![ElementoDocumento {
            tipo: "host".to_string(),
            uuid: UUID_HOST_REMOTO.to_string(),
            version: version_ajena(500),
            vector: vector_ajeno(500),
            datos,
        }],
        Vec::new(),
    );
    let (_, resumen) = fusionar_y_aplicar(&almacen, &[documento]);
    assert_eq!(resumen.creados, 1);
    assert!(!host_marcado(almacen.conexion(), UUID_HOST_REMOTO));
}

// ------------------------------------------------------------- importar y exportar

#[test]
fn un_fichero_del_formato_4_crea_la_ficha_con_su_version() {
    let almacen = almacen();
    let documento = documento_con(
        EQUIPO_AJENO,
        4,
        vec![
            elemento_host(UUID_HOST_REMOTO, "web", TipoHost::Ssh, true, 400),
            elemento_ficha(
                UUID_MEDIO,
                UUID_HOST_REMOTO,
                "agente",
                Campos {
                    carpeta: Some("~/proyectos/api"),
                    comando: Some("claude"),
                    comando_reanudar: Some("claude --continue"),
                    nota: None,
                },
                500,
            ),
        ],
        Vec::new(),
    );
    // Pasa por el mismo camino que un fichero real: serializar y parsear.
    let json = formato::serializar(&documento).expect("serializando el documento");
    let documento = formato::parsear(&json).expect("el formato 4 se lee");
    assert_eq!(documento.formato, formato::FORMATO_4);
    let (plan, resumen) = fusionar_y_aplicar(&almacen, &[documento]);
    assert_eq!(resumen.creados, 2, "el host y la ficha");
    assert_eq!(resumen.conflictos, 0);
    assert!(plan.conflictos.is_empty(), "{:?}", plan.conflictos);
    assert!(plan.unidas.is_empty());

    let importadas = fichas(almacen.conexion());
    assert_eq!(importadas.len(), 1);
    let ficha = &importadas[0];
    assert_eq!(ficha.uuid, UUID_MEDIO);
    assert_eq!(ficha.host_uuid, UUID_HOST_REMOTO);
    assert_eq!(ficha.nombre, "agente");
    assert_eq!(ficha.carpeta.as_deref(), Some("~/proyectos/api"));
    assert_eq!(ficha.comando.as_deref(), Some("claude"));
    assert_eq!(ficha.comando_reanudar.as_deref(), Some("claude --continue"));
    assert_eq!(ficha.nota, None);
    assert_eq!(ficha.origen, "manual");
    assert_eq!(ficha.creada_por, "portatil");
    assert_eq!(ficha.version, version_ajena(500));
    assert_eq!(
        ficha.vector,
        serde_json::to_string(&vector_ajeno(500)).expect("vector")
    );
    assert!(host_marcado(almacen.conexion(), UUID_HOST_REMOTO));
    // El inventario importado vuelve a exportarse en el formato 4.
    assert_eq!(
        formato_a_escribir(almacen.conexion()).expect("formato"),
        formato::FORMATO_4
    );
}

#[test]
fn una_ficha_de_un_equipo_llega_al_otro_con_el_mismo_uuid_y_contenido() {
    let (_temporal, a, b, host_uuid) = dos_equipos_con_el_mismo_host();
    let host_a = host_id_por_uuid(&a, &host_uuid);
    insertar_ficha(
        &a.almacen,
        host_a,
        "agente",
        Campos {
            carpeta: Some("~/proyectos/api"),
            comando: Some("claude"),
            comando_reanudar: Some("claude --continue"),
            nota: Some("la API"),
        },
        None,
    );
    a.pasada();
    b.pasada();
    let en_a = fichas(a.conexion());
    let en_b = fichas(b.conexion());
    assert_eq!(en_a.len(), 1);
    assert_eq!(en_a, en_b, "los dos equipos tienen la misma ficha");
    assert_eq!(en_b[0].host_uuid, host_uuid);

    // Una edición viaja con su versión nueva.
    let id_a: i64 = a
        .conexion()
        .query_row("SELECT id FROM SESIONES_PERSISTENTES", [], |f| f.get(0))
        .expect("id de la ficha");
    editar_comando(&a.almacen, id_a, "claude --resume");
    a.pasada();
    b.pasada();
    assert_eq!(fichas(a.conexion()), fichas(b.conexion()));
    assert_eq!(
        fichas(b.conexion())[0].comando.as_deref(),
        Some("claude --resume")
    );
}

// ------------------------------------------------------------- unión (D337)

#[test]
fn el_mismo_nombre_creado_en_dos_equipos_acaba_en_una_sola_ficha_la_misma() {
    // Los dos órdenes de unión (quién sincroniza primero) y las dos formas de
    // repartir los uuid dan el mismo resultado: queda la de uuid menor.
    for (uuid_a, uuid_b) in [(UUID_MENOR, UUID_MAYOR), (UUID_MAYOR, UUID_MENOR)] {
        for a_primero in [true, false] {
            let (_temporal, a, b, host_uuid) = dos_equipos_con_el_mismo_host();
            // Sin conexión, cada uno crea la sesión «agente» con lo suyo.
            insertar_ficha(
                &a.almacen,
                host_id_por_uuid(&a, &host_uuid),
                "agente",
                Campos {
                    carpeta: Some("~/proyectos/api"),
                    ..Campos::default()
                },
                Some(uuid_a),
            );
            insertar_ficha(
                &b.almacen,
                host_id_por_uuid(&b, &host_uuid),
                "agente",
                Campos {
                    comando: Some("claude"),
                    comando_reanudar: Some("claude --continue"),
                    ..Campos::default()
                },
                Some(uuid_b),
            );
            // Dos rondas completas bastan para que todos lo vean todo.
            for _ in 0..2 {
                if a_primero {
                    a.pasada();
                    b.pasada();
                } else {
                    b.pasada();
                    a.pasada();
                }
            }
            let en_a = fichas(a.conexion());
            let en_b = fichas(b.conexion());
            let contexto = format!("a={uuid_a} b={uuid_b} a_primero={a_primero}");
            assert_eq!(en_a.len(), 1, "una sola ficha en A ({contexto}): {en_a:?}");
            assert_eq!(en_b.len(), 1, "una sola ficha en B ({contexto}): {en_b:?}");
            assert_eq!(en_a, en_b, "la misma ficha en los dos ({contexto})");
            let ficha = &en_a[0];
            assert_eq!(ficha.uuid, UUID_MENOR, "gana el uuid menor ({contexto})");
            assert_eq!(ficha.nombre, "agente");
            // Con lo que le faltaba de la otra: la carpeta de A y el comando de B.
            assert_eq!(ficha.carpeta.as_deref(), Some("~/proyectos/api"));
            assert_eq!(ficha.comando.as_deref(), Some("claude"));
            assert_eq!(ficha.comando_reanudar.as_deref(), Some("claude --continue"));
            // La otra queda con lápida en los dos equipos.
            for entorno in [&a, &b] {
                let lapida = lapidas::obtener(entorno.conexion(), UUID_MAYOR)
                    .expect("leyendo la lápida")
                    .unwrap_or_else(|| panic!("la perdedora deja lápida ({contexto})"));
                assert_eq!(lapida.tipo, "sesion_persistente");
                assert_eq!(lapida.nombre, "agente");
            }
            // Y no se ha renombrado nada a «agente (2)».
            for entorno in [&a, &b] {
                let renombradas: i64 = entorno
                    .conexion()
                    .query_row(
                        "SELECT COUNT(*) FROM SESIONES_PERSISTENTES WHERE nombre LIKE '%(2)%'",
                        [],
                        |fila| fila.get(0),
                    )
                    .expect("contando renombradas");
                assert_eq!(renombradas, 0);
            }
            // Los dos acaban con el fichero estable: otra ronda no cambia nada.
            let antes = (fichas(a.conexion()), fichas(b.conexion()));
            a.pasada();
            b.pasada();
            assert_eq!((fichas(a.conexion()), fichas(b.conexion())), antes);
        }
    }
}

#[test]
fn si_los_dos_unen_a_la_vez_con_ficheros_viejos_acaban_igual() {
    // Peor caso: cada equipo une leyendo el fichero que el otro escribió antes
    // de la unión, así que los dos editan a la ganadora por su cuenta (con
    // versiones distintas y el mismo contenido) y los dos dejan su lápida.
    let (_temporal, a, b, host_uuid) = dos_equipos_con_el_mismo_host();
    insertar_ficha(
        &a.almacen,
        host_id_por_uuid(&a, &host_uuid),
        "agente",
        Campos {
            carpeta: Some("~/proyectos/api"),
            ..Campos::default()
        },
        Some(UUID_MENOR),
    );
    insertar_ficha(
        &b.almacen,
        host_id_por_uuid(&b, &host_uuid),
        "agente",
        Campos {
            comando: Some("claude"),
            ..Campos::default()
        },
        Some(UUID_MAYOR),
    );
    let documento_de = |entorno: &Entorno| {
        documento_propio(entorno.conexion(), &entorno.rutas, "1.5.0").expect("documento propio")
    };
    let de_a = documento_de(&a);
    let de_b = documento_de(&b);
    // Los dos unen a la vez, cada uno con el fichero viejo del otro.
    fusionar_y_aplicar(&a.almacen, &[de_b]);
    fusionar_y_aplicar(&b.almacen, &[de_a]);
    assert_eq!(contar_fichas(a.conexion()), 1);
    assert_eq!(contar_fichas(b.conexion()), 1);

    // Ahora cada uno lee el fichero nuevo del otro, ya con la unión hecha.
    let de_a = documento_de(&a);
    let de_b = documento_de(&b);
    fusionar_y_aplicar(&a.almacen, &[de_b]);
    fusionar_y_aplicar(&b.almacen, &[de_a]);
    let de_a = documento_de(&a);
    let de_b = documento_de(&b);
    fusionar_y_aplicar(&a.almacen, &[de_b]);
    fusionar_y_aplicar(&b.almacen, &[de_a]);

    let en_a = fichas(a.conexion());
    let en_b = fichas(b.conexion());
    assert_eq!(en_a.len(), 1, "{en_a:?}");
    assert_eq!(en_a, en_b, "la misma ficha, con la misma versión");
    assert_eq!(en_a[0].uuid, UUID_MENOR);
    assert_eq!(en_a[0].carpeta.as_deref(), Some("~/proyectos/api"));
    assert_eq!(en_a[0].comando.as_deref(), Some("claude"));
    for entorno in [&a, &b] {
        assert!(lapidas::obtener(entorno.conexion(), UUID_MAYOR)
            .expect("lápida")
            .is_some());
    }
}

#[test]
fn tres_equipos_con_el_mismo_nombre_acaban_con_una_sola_ficha_en_cualquier_orden() {
    for orden in [[0usize, 1, 2], [2, 1, 0], [1, 2, 0]] {
        let (_temporal, carpeta) = carpeta_compartida();
        let clave = ClaveSincronizacion::generar().texto().to_string();
        let equipos = [
            Entorno::nuevo(&carpeta, "sobremesa", &clave),
            Entorno::nuevo(&carpeta, "portatil", &clave),
            Entorno::nuevo(&carpeta, "estudio", &clave),
        ];
        let host = crear_host(&equipos[0].almacen, "web");
        let host_uuid = uuid_host(&equipos[0].almacen, host);
        for equipo in &equipos {
            equipo.pasada();
        }
        // Cada equipo crea «agente» sin conexión, con su uuid y lo suyo.
        let datos = [
            (
                UUID_MAYOR,
                Campos {
                    carpeta: Some("/a"),
                    ..Campos::default()
                },
            ),
            (
                UUID_MEDIO,
                Campos {
                    comando: Some("claude"),
                    ..Campos::default()
                },
            ),
            (
                UUID_MENOR,
                Campos {
                    nota: Some("la del estudio"),
                    ..Campos::default()
                },
            ),
        ];
        for (equipo, (uuid, campos)) in equipos.iter().zip(datos) {
            insertar_ficha(
                &equipo.almacen,
                host_id_por_uuid(equipo, &host_uuid),
                "agente",
                campos,
                Some(uuid),
            );
        }
        for _ in 0..4 {
            for &indice in &orden {
                equipos[indice].pasada();
            }
        }
        let referencia = fichas(equipos[0].conexion());
        assert_eq!(referencia.len(), 1, "orden {orden:?}: {referencia:?}");
        assert_eq!(referencia[0].uuid, UUID_MENOR);
        assert_eq!(referencia[0].carpeta.as_deref(), Some("/a"));
        assert_eq!(referencia[0].comando.as_deref(), Some("claude"));
        assert_eq!(referencia[0].nota.as_deref(), Some("la del estudio"));
        for equipo in &equipos[1..] {
            assert_eq!(fichas(equipo.conexion()), referencia, "orden {orden:?}");
        }
        for equipo in &equipos {
            for perdedora in [UUID_MAYOR, UUID_MEDIO] {
                assert!(
                    lapidas::obtener(equipo.conexion(), perdedora)
                        .expect("lápida")
                        .is_some(),
                    "orden {orden:?}: lápida de {perdedora}"
                );
            }
        }
    }
}

#[test]
fn la_union_deja_un_conflicto_de_nombre_y_nunca_pide_recuperar_la_perdedora() {
    let (_temporal, a, b, host_uuid) = dos_equipos_con_el_mismo_host();
    insertar_ficha(
        &a.almacen,
        host_id_por_uuid(&a, &host_uuid),
        "agente",
        Campos::default(),
        Some(UUID_MENOR),
    );
    insertar_ficha(
        &b.almacen,
        host_id_por_uuid(&b, &host_uuid),
        "agente",
        Campos::default(),
        Some(UUID_MAYOR),
    );
    a.pasada();
    b.pasada();
    // B es quien tenía la perdedora: guarda el conflicto «nombre».
    let pendientes = conflictos::listar_pendientes(b.conexion()).expect("conflictos de B");
    let conflicto = pendientes
        .iter()
        .find(|conflicto| {
            conflicto.tipo == "nombre" && conflicto.elemento_tipo == "sesion_persistente"
        })
        .unwrap_or_else(|| panic!("B guarda un conflicto de nombre: {pendientes:?}"));
    assert_eq!(conflicto.uuid, UUID_MAYOR);
    assert_eq!(conflicto.nombre, "agente");
    assert!(
        conflicto.version_perdedora_json.is_none(),
        "no hay versión que recuperar: duplicaría el nombre"
    );
    // Recuperarlo es un error claro y no toca nada.
    let error = magi_nucleo::sincronizacion::aplicacion::recuperar_version(
        b.conexion(),
        conflicto,
        Path::new(HOGAR),
    )
    .expect_err("no se recupera una ficha unida")
    .to_string();
    assert!(error.contains("versión perdedora"), "{error}");
    assert_eq!(contar_fichas(b.conexion()), 1);
}

#[test]
fn la_fusion_une_igual_con_cualquier_orden_de_documentos_y_de_origen() {
    let equipo_x = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
    let equipo_y = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";
    let documento_x = |ms| {
        documento_con(
            equipo_x,
            4,
            vec![
                elemento_host(UUID_HOST_REMOTO, "web", TipoHost::Ssh, false, 100),
                elemento_ficha(
                    UUID_MAYOR,
                    UUID_HOST_REMOTO,
                    "agente",
                    Campos {
                        comando: Some("claude"),
                        nota: Some("nota de X"),
                        ..Campos::default()
                    },
                    ms,
                ),
            ],
            Vec::new(),
        )
    };
    let documento_y = |ms| {
        documento_con(
            equipo_y,
            4,
            vec![
                elemento_host(UUID_HOST_REMOTO, "web", TipoHost::Ssh, false, 100),
                elemento_ficha(
                    UUID_MENOR,
                    UUID_HOST_REMOTO,
                    "agente",
                    Campos {
                        carpeta: Some("/srv/api"),
                        nota: Some("nota de Y"),
                        ..Campos::default()
                    },
                    ms,
                ),
                elemento_ficha(
                    UUID_MEDIO,
                    UUID_HOST_REMOTO,
                    "agente",
                    Campos {
                        comando_reanudar: Some("claude --continue"),
                        ..Campos::default()
                    },
                    ms + 1,
                ),
            ],
            Vec::new(),
        )
    };

    let resumen_de = |plan: &PlanFusion| {
        let mut vivas: Vec<(String, String)> = plan
            .vivos
            .iter()
            .filter(|vivo| vivo.tipo == "sesion_persistente")
            .map(|vivo| (vivo.uuid.clone(), vivo.contenido_json().unwrap_or_default()))
            .collect();
        vivas.sort();
        let unidas: Vec<(String, String, bool)> = plan
            .unidas
            .iter()
            .map(|union| {
                (
                    union.ganadora.clone(),
                    union.perdedora.clone(),
                    union.ganadora_editada,
                )
            })
            .collect();
        (vivas, unidas, plan.conflictos.clone())
    };

    let referencia = fusionar(&[], &[documento_x(200), documento_y(300)]).expect("fusión");
    let (vivas, unidas, conflictos_plan) = resumen_de(&referencia);
    assert_eq!(vivas.len(), 1, "las tres fichas son una sola: {vivas:?}");
    assert_eq!(vivas[0].0, UUID_MENOR);
    let contenido: serde_json::Value =
        serde_json::from_str(&vivas[0].1).expect("contenido de la ganadora");
    assert_eq!(contenido["nombre"], "agente", "no se renombra");
    assert_eq!(contenido["carpeta"], "/srv/api", "lo suyo no se pisa");
    assert_eq!(contenido["nota"], "nota de Y", "lo suyo no se pisa");
    assert_eq!(
        contenido["comando"], "claude",
        "lo que falta, de la primera que lo tenga"
    );
    assert_eq!(contenido["comando_reanudar"], "claude --continue");
    assert_eq!(
        unidas,
        vec![
            (UUID_MENOR.to_string(), UUID_MEDIO.to_string(), true),
            (UUID_MENOR.to_string(), UUID_MAYOR.to_string(), true),
        ]
    );
    let de_nombre: Vec<_> = conflictos_plan
        .iter()
        .filter(|conflicto| conflicto.tipo == "nombre")
        .collect();
    assert_eq!(
        de_nombre.len(),
        2,
        "un conflicto por perdedora: {conflictos_plan:?}"
    );
    assert!(referencia.renombrados.is_empty());

    // Al revés de documentos: lo mismo.
    let al_reves = fusionar(&[], &[documento_y(300), documento_x(200)]).expect("fusión");
    assert_eq!(resumen_de(&al_reves), resumen_de(&referencia));

    // Con una de las fichas como estado local (en vez de en un fichero): lo
    // mismo. Se prepara con un almacén que ya tiene el host y la ficha mayor.
    let almacen = almacen();
    fusionar_y_aplicar(&almacen, &[documento_x(200)]);
    assert_eq!(contar_fichas(almacen.conexion()), 1);
    let local = estado_local(almacen.conexion(), Path::new(HOGAR)).expect("estado local");
    let con_local = fusionar(&local, &[documento_y(300)]).expect("fusión con local");
    assert_eq!(resumen_de(&con_local), resumen_de(&referencia));
}

#[test]
fn una_union_sin_aportaciones_no_edita_a_la_ganadora() {
    let documento = |equipo: &str, uuid: &str, ms| {
        documento_con(
            equipo,
            4,
            vec![
                elemento_host(UUID_HOST_REMOTO, "web", TipoHost::Ssh, false, 100),
                elemento_ficha(
                    uuid,
                    UUID_HOST_REMOTO,
                    "agente",
                    Campos {
                        carpeta: Some("~/proyectos"),
                        comando: Some("claude"),
                        ..Campos::default()
                    },
                    ms,
                ),
            ],
            Vec::new(),
        )
    };
    let plan = fusionar(
        &[],
        &[
            documento("cccccccc-cccc-4ccc-8ccc-cccccccccccc", UUID_MENOR, 200),
            documento("dddddddd-dddd-4ddd-8ddd-dddddddddddd", UUID_MAYOR, 300),
        ],
    )
    .expect("fusión");
    assert_eq!(plan.unidas.len(), 1);
    assert!(
        !plan.unidas[0].ganadora_editada,
        "la ganadora ya lo tenía todo: no hay edición nueva"
    );
    let ganadora = plan
        .vivos
        .iter()
        .find(|vivo| vivo.tipo == "sesion_persistente")
        .expect("la ganadora");
    assert_eq!(ganadora.uuid, UUID_MENOR);
    assert_eq!(
        ganadora.version,
        version_ajena(200),
        "la versión de la ganadora no cambia al aplicar sin aportaciones"
    );
}

#[test]
fn el_mismo_nombre_en_hosts_distintos_no_se_une() {
    let otro_host = "88888888-8888-4888-8888-888888888888";
    let documento = documento_con(
        EQUIPO_AJENO,
        4,
        vec![
            elemento_host(UUID_HOST_REMOTO, "web", TipoHost::Ssh, false, 100),
            elemento_host(otro_host, "db", TipoHost::Ssh, false, 100),
            elemento_ficha(
                UUID_MENOR,
                UUID_HOST_REMOTO,
                "agente",
                Campos::default(),
                200,
            ),
            elemento_ficha(UUID_MAYOR, otro_host, "agente", Campos::default(), 200),
        ],
        Vec::new(),
    );
    let almacen = almacen();
    let (plan, resumen) = fusionar_y_aplicar(&almacen, &[documento]);
    assert!(plan.unidas.is_empty());
    assert!(plan.renombrados.is_empty(), "{:?}", plan.renombrados);
    assert!(plan.conflictos.is_empty(), "{:?}", plan.conflictos);
    assert_eq!(resumen.creados, 4);
    assert_eq!(contar_fichas(almacen.conexion()), 2);
}

#[test]
fn unir_por_nombre_adopta_el_uuid_del_fichero_para_la_misma_ficha() {
    let almacen = almacen();
    let host = crear_host(&almacen, "web");
    let host_uuid = uuid_host(&almacen, host);
    insertar_ficha(&almacen, host, "agente", Campos::default(), None);
    // Una ficha del mismo host con otro nombre no se toca.
    insertar_ficha(&almacen, host, "otra", Campos::default(), None);

    let documento = documento_con(
        EQUIPO_AJENO,
        4,
        vec![elemento_ficha(
            UUID_MEDIO,
            &host_uuid,
            "agente",
            Campos::default(),
            500,
        )],
        Vec::new(),
    );
    let unidos = unir_por_nombre(almacen.conexion(), &[documento], Path::new(HOGAR))
        .expect("unión por nombre");
    assert_eq!(unidos, 1, "la ficha local adopta el uuid del fichero");
    let uuid: String = almacen
        .conexion()
        .query_row(
            "SELECT uuid FROM SESIONES_PERSISTENTES WHERE nombre = 'agente'",
            [],
            |fila| fila.get(0),
        )
        .expect("uuid de la ficha");
    assert_eq!(uuid, UUID_MEDIO);
}

// ------------------------------------------------------------------- lápidas

#[test]
fn borrar_una_ficha_deja_lapida_y_se_propaga_al_otro_equipo() {
    let (_temporal, a, b, host_uuid) = dos_equipos_con_el_mismo_host();
    let host_a = host_id_por_uuid(&a, &host_uuid);
    let id = insertar_ficha(&a.almacen, host_a, "agente", Campos::default(), None);
    let (uuid, _, _) = fila_versionada(&a.almacen, Tabla::SesionesPersistentes, id);
    a.pasada();
    b.pasada();
    assert_eq!(contar_fichas(b.conexion()), 1);

    // A la borra: deja lápida y el fichero deja de llevar la ficha.
    borrar_ficha(&a.almacen, id);
    let lapida = lapidas::obtener(a.conexion(), &uuid)
        .expect("leyendo la lápida")
        .expect("borrar una ficha deja lápida");
    assert_eq!(lapida.tipo, "sesion_persistente");
    assert_eq!(lapida.nombre, "agente");
    a.pasada();
    b.pasada();

    assert_eq!(contar_fichas(b.conexion()), 0, "la lápida se propaga");
    let lapida_b = lapidas::obtener(b.conexion(), &uuid)
        .expect("leyendo la lápida en B")
        .expect("B también guarda la lápida");
    assert_eq!(lapida_b.tipo, "sesion_persistente");
    // El formato vuelve al que tocaba: ya no hay fichas.
    assert_eq!(a.formato_del_fichero_propio(), formato::FORMATO_1);
}

#[test]
fn una_lapida_antigua_no_borra_una_edicion_posterior_de_la_ficha() {
    let almacen = almacen();
    let host = crear_host(&almacen, "web");
    let host_uuid = uuid_host(&almacen, host);
    let id = insertar_ficha(&almacen, host, "agente", Campos::default(), None);
    let (uuid, version_local, _) = fila_versionada(&almacen, Tabla::SesionesPersistentes, id);
    let _ = host_uuid;
    // Una lápida de hace más milisegundos que la edición local pierde.
    let documento = documento_con(
        EQUIPO_AJENO,
        4,
        Vec::new(),
        vec![lapida_ficha(&uuid, "agente", ms_de(&version_local) - 1_000)],
    );
    let (_, resumen) = fusionar_y_aplicar(&almacen, &[documento]);
    assert_eq!(resumen.borrados, 0);
    assert_eq!(contar_fichas(almacen.conexion()), 1);
}

#[test]
fn una_lapida_del_formato_4_borra_la_ficha_y_deja_su_lapida() {
    let almacen = almacen();
    let host = crear_host(&almacen, "web");
    let id = insertar_ficha(&almacen, host, "agente", Campos::default(), None);
    let (uuid, version_local, _) = fila_versionada(&almacen, Tabla::SesionesPersistentes, id);
    let documento = documento_con(
        EQUIPO_AJENO,
        4,
        Vec::new(),
        vec![lapida_ficha(&uuid, "agente", ms_de(&version_local) + 1_000)],
    );
    let (_, resumen) = fusionar_y_aplicar(&almacen, &[documento]);
    assert_eq!(resumen.borrados, 1);
    assert_eq!(contar_fichas(almacen.conexion()), 0);
    let lapida = lapidas::obtener(almacen.conexion(), &uuid)
        .expect("leyendo la lápida")
        .expect("el borrado deja lápida");
    assert_eq!(lapida.tipo, "sesion_persistente");
    assert_eq!(lapida.nombre, "agente");
}

#[test]
fn borrar_un_host_deja_lapida_de_sus_fichas() {
    let almacen = almacen();
    let host = crear_host(&almacen, "web");
    let id = insertar_ficha(&almacen, host, "agente", Campos::default(), None);
    let (uuid, _, _) = fila_versionada(&almacen, Tabla::SesionesPersistentes, id);
    almacen.borrar_host(host).expect("borrando el host");
    assert_eq!(contar_fichas(almacen.conexion()), 0, "caen en cascada");
    let lapida = lapidas::obtener(almacen.conexion(), &uuid)
        .expect("leyendo la lápida")
        .expect("el host borra sus fichas con lápida");
    assert_eq!(lapida.tipo, "sesion_persistente");
    assert_eq!(lapida.nombre, "agente");
}

#[test]
fn un_host_borrado_en_otro_equipo_borra_sus_fichas_con_lapida_aqui() {
    let (_temporal, a, b, host_uuid) = dos_equipos_con_el_mismo_host();
    let host_a = host_id_por_uuid(&a, &host_uuid);
    let id = insertar_ficha(&a.almacen, host_a, "agente", Campos::default(), None);
    let (uuid_ficha, _, _) = fila_versionada(&a.almacen, Tabla::SesionesPersistentes, id);
    a.pasada();
    b.pasada();
    assert_eq!(contar_fichas(b.conexion()), 1);

    a.almacen.borrar_host(host_a).expect("borrando el host");
    a.pasada();
    b.pasada();
    assert_eq!(contar_fichas(b.conexion()), 0);
    let lapida = lapidas::obtener(b.conexion(), &uuid_ficha)
        .expect("leyendo la lápida")
        .expect("la ficha del host borrado deja lápida");
    assert_eq!(lapida.tipo, "sesion_persistente");
}

// ------------------------------------------------------------------- huérfanas

#[test]
fn una_ficha_de_un_host_que_no_es_ssh_es_huerfana() {
    let almacen = almacen();
    let documento = documento_con(
        EQUIPO_AJENO,
        4,
        vec![
            elemento_host(UUID_HOST_REMOTO, "win-01", TipoHost::Rdp, false, 100),
            elemento_ficha(
                UUID_MEDIO,
                UUID_HOST_REMOTO,
                "agente",
                Campos::default(),
                200,
            ),
        ],
        Vec::new(),
    );
    let (plan, resumen) = fusionar_y_aplicar(&almacen, &[documento]);
    assert!(plan
        .conflictos
        .iter()
        .any(|conflicto| conflicto.tipo == "huerfano"
            && conflicto.elemento_tipo == "sesion_persistente"));
    assert_eq!(resumen.creados, 1, "solo el host");
    assert_eq!(contar_fichas(almacen.conexion()), 0);
    let pendientes = conflictos::listar_pendientes(almacen.conexion()).expect("conflictos");
    assert!(pendientes
        .iter()
        .any(|conflicto| conflicto.tipo == "huerfano"
            && conflicto.elemento_tipo == "sesion_persistente"));
}

#[test]
fn una_ficha_de_un_host_que_no_existe_es_huerfana() {
    let almacen = almacen();
    let documento = documento_con(
        EQUIPO_AJENO,
        4,
        vec![elemento_ficha(
            UUID_MEDIO,
            "77777777-7777-4777-8777-777777777777",
            "agente",
            Campos::default(),
            200,
        )],
        Vec::new(),
    );
    let (plan, resumen) = fusionar_y_aplicar(&almacen, &[documento]);
    assert!(plan
        .conflictos
        .iter()
        .any(|conflicto| conflicto.tipo == "huerfano"));
    assert_eq!(resumen.creados, 0);
    assert_eq!(contar_fichas(almacen.conexion()), 0);
}

#[test]
fn una_ficha_con_datos_no_validos_no_se_aplica_y_deja_conflicto() {
    let almacen = almacen();
    let mut mala = elemento_ficha(
        UUID_MEDIO,
        UUID_HOST_REMOTO,
        "nombre con espacios",
        Campos::default(),
        200,
    );
    mala.datos["origen"] = json!("manual");
    let mut otro_origen = elemento_ficha(
        UUID_MAYOR,
        UUID_HOST_REMOTO,
        "buena",
        Campos::default(),
        200,
    );
    otro_origen.datos["origen"] = json!("inventado");
    let documento = documento_con(
        EQUIPO_AJENO,
        4,
        vec![
            elemento_host(UUID_HOST_REMOTO, "web", TipoHost::Ssh, false, 100),
            mala,
            otro_origen,
            elemento_ficha(
                UUID_MENOR,
                UUID_HOST_REMOTO,
                "valida",
                Campos::default(),
                200,
            ),
        ],
        Vec::new(),
    );
    let (_, resumen) = fusionar_y_aplicar(&almacen, &[documento]);
    assert_eq!(resumen.creados, 2, "el host y la ficha válida");
    let importadas = fichas(almacen.conexion());
    assert_eq!(importadas.len(), 1);
    assert_eq!(importadas[0].nombre, "valida");
    let pendientes = conflictos::listar_pendientes(almacen.conexion()).expect("conflictos");
    assert_eq!(
        pendientes
            .iter()
            .filter(|conflicto| conflicto.tipo == "elemento"
                && conflicto.elemento_tipo == "sesion_persistente")
            .count(),
        2,
        "una por cada ficha que no se puede aplicar: {pendientes:?}"
    );
}

#[test]
fn una_ficha_que_no_valida_no_se_une_ni_se_lleva_por_delante_a_una_buena() {
    // La mala tiene el uuid menor y el mismo host y nombre que la buena: no
    // puede ganar la unión, porque la aplicación la descartará.
    let mut mala = elemento_ficha(
        UUID_MENOR,
        UUID_HOST_REMOTO,
        "agente",
        Campos::default(),
        200,
    );
    mala.datos["origen"] = json!("inventado");
    let documento = documento_con(
        EQUIPO_AJENO,
        4,
        vec![
            elemento_host(UUID_HOST_REMOTO, "web", TipoHost::Ssh, false, 100),
            mala,
            elemento_ficha(
                UUID_MAYOR,
                UUID_HOST_REMOTO,
                "agente",
                Campos {
                    comando: Some("claude"),
                    ..Campos::default()
                },
                300,
            ),
        ],
        Vec::new(),
    );
    let almacen = almacen();
    let (plan, resumen) = fusionar_y_aplicar(&almacen, &[documento]);
    assert!(plan.unidas.is_empty(), "{:?}", plan.unidas);
    assert_eq!(resumen.creados, 2, "el host y la ficha buena");
    let importadas = fichas(almacen.conexion());
    assert_eq!(importadas.len(), 1);
    assert_eq!(importadas[0].uuid, UUID_MAYOR);
    assert_eq!(importadas[0].comando.as_deref(), Some("claude"));
    let pendientes = conflictos::listar_pendientes(almacen.conexion()).expect("conflictos");
    assert!(pendientes
        .iter()
        .any(|conflicto| conflicto.tipo == "elemento"
            && conflicto.elemento_tipo == "sesion_persistente"
            && conflicto.uuid == UUID_MENOR));
    assert!(
        lapidas::obtener(almacen.conexion(), UUID_MAYOR)
            .expect("lápida")
            .is_none(),
        "la buena no se borra"
    );
}

#[test]
fn los_datos_de_una_ficha_del_fichero_se_validan_antes_de_aplicarse() {
    use magi_nucleo::sincronizacion::elementos::DatosSesionPersistente;
    let buena = || DatosSesionPersistente {
        host: UUID_HOST_REMOTO.to_string(),
        nombre: "agente".to_string(),
        carpeta: Some("~/proyectos/api".to_string()),
        comando: Some("claude".to_string()),
        comando_reanudar: Some("claude --continue".to_string()),
        nota: Some("la API".to_string()),
        origen: "descubierta".to_string(),
        creada_por: "sobremesa".to_string(),
    };
    assert_eq!(buena().validar(), Ok(()));

    // Los límites exactos valen y uno más no.
    let mut datos = buena();
    datos.nombre = "a".repeat(64);
    assert_eq!(datos.validar(), Ok(()));
    datos.nombre = "a".repeat(65);
    assert!(datos.validar().is_err());
    for nombre in ["", "con espacio", "con.punto", "ñandú", "a/b", "=agente"] {
        let mut datos = buena();
        datos.nombre = nombre.to_string();
        assert!(
            datos.validar().is_err(),
            "«{nombre}» no es un nombre válido"
        );
    }
    let mut datos = buena();
    datos.carpeta = Some(format!("/{}", "x".repeat(1023)));
    assert_eq!(datos.validar(), Ok(()));
    datos.carpeta = Some(format!("/{}", "x".repeat(1024)));
    assert!(datos.validar().is_err());
    datos.carpeta = Some("relativa/sin/raiz".to_string());
    assert!(datos.validar().is_err(), "la carpeta es absoluta o ~/…");
    datos.carpeta = Some("~/proyectos".to_string());
    assert_eq!(datos.validar(), Ok(()));
    datos.carpeta = Some("~/a\nb".to_string());
    assert!(
        datos.validar().is_err(),
        "sin saltos de línea en la carpeta"
    );
    let mut datos = buena();
    datos.comando = Some("x".repeat(4096));
    assert_eq!(datos.validar(), Ok(()));
    datos.comando = Some("x".repeat(4097));
    assert!(datos.validar().is_err());
    datos.comando = Some("a\0b".to_string());
    assert!(datos.validar().is_err(), "sin bytes nulos en el comando");
    let mut datos = buena();
    datos.comando_reanudar = Some("a\0b".to_string());
    assert!(datos.validar().is_err());
    let mut datos = buena();
    datos.nota = Some("ñ".repeat(500));
    assert_eq!(datos.validar(), Ok(()), "500 caracteres, no 500 bytes");
    datos.nota = Some("ñ".repeat(501));
    assert!(datos.validar().is_err());
    // Un comando con saltos de línea es texto libre, como un snippet.
    let mut datos = buena();
    datos.comando = Some("cd /srv\nclaude".to_string());
    assert_eq!(datos.validar(), Ok(()));
    for origen in ["manual", "host", "descubierta"] {
        let mut datos = buena();
        datos.origen = origen.to_string();
        assert_eq!(datos.validar(), Ok(()));
    }
    let mut datos = buena();
    datos.origen = "otro".to_string();
    assert!(datos.validar().is_err());
    let mut datos = buena();
    datos.host = "  ".to_string();
    assert!(datos.validar().is_err());
    let mut datos = buena();
    datos.creada_por = String::new();
    assert!(datos.validar().is_err());
    // El equipo vale igual que en el CRUD: recortado y no vacío.
    datos.creada_por = "   ".to_string();
    assert!(datos.validar().is_err(), "solo espacios no es un equipo");
    datos.creada_por = "  sobremesa  ".to_string();
    assert_eq!(datos.validar(), Ok(()));
}

/// La importación guarda `creada_por` como lo guarda el CRUD (`validar_datos`):
/// recortado. Un fichero con la etiqueta con espacios (de un escritor que no
/// pasó por el CRUD) no deja la ficha distinta de la que habría creado este
/// equipo.
#[test]
fn la_etiqueta_del_equipo_de_una_ficha_importada_se_guarda_recortada() {
    let almacen = almacen();
    let host = crear_host(&almacen, "web");
    let uuid_del_host = uuid_host(&almacen, host);
    let mut ficha = elemento_ficha(
        UUID_MENOR,
        &uuid_del_host,
        "agente",
        Campos::default(),
        5_000,
    );
    ficha.datos["creada_por"] = json!("  portatil  ");
    let documento = documento_con(EQUIPO_AJENO, 4, vec![ficha], Vec::new());
    let (_, resumen) = fusionar_y_aplicar(&almacen, &[documento]);
    assert_eq!(resumen.creados, 1);
    let todas = fichas(almacen.conexion());
    assert_eq!(todas.len(), 1);
    assert_eq!(todas[0].creada_por, "portatil");
    // Y la lee el almacén tal cual, sin avisos de ficha ilegible.
    let leida = almacen.fichas_de_host(host).expect("fichas del host");
    assert_eq!(leida.len(), 1);
    assert_eq!(leida[0].creada_por, "portatil");
}

/// Una etiqueta de equipo en blanco no se aplica: conflicto de elemento, como
/// el resto de datos que el CRUD rechazaría.
#[test]
fn una_ficha_con_la_etiqueta_del_equipo_en_blanco_no_se_aplica() {
    let almacen = almacen();
    let host = crear_host(&almacen, "web");
    let uuid_del_host = uuid_host(&almacen, host);
    let mut ficha = elemento_ficha(
        UUID_MENOR,
        &uuid_del_host,
        "agente",
        Campos::default(),
        5_000,
    );
    ficha.datos["creada_por"] = json!("   ");
    let documento = documento_con(EQUIPO_AJENO, 4, vec![ficha], Vec::new());
    let (_, resumen) = fusionar_y_aplicar(&almacen, &[documento]);
    assert_eq!(resumen.creados, 0);
    assert_eq!(contar_fichas(almacen.conexion()), 0);
    let pendientes = conflictos::listar_pendientes(almacen.conexion()).expect("conflictos");
    assert!(
        pendientes
            .iter()
            .any(|conflicto| conflicto.tipo == "elemento"
                && conflicto.elemento_tipo == "sesion_persistente"),
        "{pendientes:?}"
    );
}

/// El CRUD no deja marcar un host que no es `ssh`; la importación tampoco: el
/// host se aplica con todo lo demás y se ignora la marca, que solo podría venir
/// de un escritor con errores. En un host `ssh` la marca viaja como siempre.
#[test]
fn la_importacion_ignora_la_marca_de_un_host_que_no_es_ssh() {
    let almacen = almacen();
    let documento = documento_con(
        EQUIPO_AJENO,
        4,
        vec![
            elemento_host(UUID_HOST_REMOTO, "win-01", TipoHost::Rdp, true, 100),
            elemento_host(
                "88888888-aaaa-4aaa-8aaa-888888888888",
                "web-02",
                TipoHost::Ssh,
                true,
                100,
            ),
        ],
        Vec::new(),
    );
    let (_, resumen) = fusionar_y_aplicar(&almacen, &[documento]);
    assert_eq!(resumen.creados, 2, "los dos hosts se aplican");
    let tipo_rdp: String = almacen
        .conexion()
        .query_row(
            "SELECT tipo FROM HOSTS WHERE uuid = ?1",
            params![UUID_HOST_REMOTO],
            |fila| fila.get(0),
        )
        .expect("el host rdp está");
    assert_eq!(tipo_rdp, "rdp");
    assert!(
        !host_marcado(almacen.conexion(), UUID_HOST_REMOTO),
        "un host rdp nunca queda marcado"
    );
    assert!(
        host_marcado(almacen.conexion(), "88888888-aaaa-4aaa-8aaa-888888888888"),
        "el ssh sí"
    );
}

/// La misma regla al actualizar un host que ya existe aquí.
#[test]
fn una_actualizacion_con_la_marca_en_un_host_que_no_es_ssh_tampoco_la_enciende() {
    let almacen = almacen();
    let rdp = crear_host_escritorio(&almacen, "win-01");
    let uuid = uuid_host(&almacen, rdp);
    let (_, version_local, _) = fila_versionada(&almacen, Tabla::Hosts, rdp);
    let ms = ms_de(&version_local) + 10_000;
    let documento = documento_con(
        EQUIPO_AJENO,
        4,
        vec![elemento_host(&uuid, "win-01", TipoHost::Rdp, true, ms)],
        Vec::new(),
    );
    let (_, resumen) = fusionar_y_aplicar(&almacen, &[documento]);
    assert_eq!(resumen.actualizados, 1);
    assert!(!host_marcado(almacen.conexion(), &uuid));
}

#[test]
fn recuperar_la_version_perdedora_de_una_ficha_la_escribe_como_edicion_nueva() {
    use magi_nucleo::sincronizacion::aplicacion::recuperar_version;
    let almacen = almacen();
    let host = crear_host(&almacen, "web");
    let host_uuid = uuid_host(&almacen, host);
    let id = insertar_ficha(
        &almacen,
        host,
        "agente",
        Campos {
            comando: Some("el-mío"),
            ..Campos::default()
        },
        None,
    );
    let (uuid, version_local, _) = fila_versionada(&almacen, Tabla::SesionesPersistentes, id);

    // Otro equipo la edita a la vez (vectores concurrentes) y gana por ser
    // más reciente: la edición local queda como conflicto de elemento.
    let ms = ms_de(&version_local) + 5_000;
    let documento = documento_con(
        EQUIPO_AJENO,
        4,
        vec![elemento_ficha(
            &uuid,
            &host_uuid,
            "agente",
            Campos {
                comando: Some("el-ajeno"),
                ..Campos::default()
            },
            ms,
        )],
        Vec::new(),
    );
    let (_, resumen) = fusionar_y_aplicar(&almacen, &[documento]);
    assert_eq!(resumen.actualizados, 1);
    assert_eq!(resumen.conflictos, 1);
    assert_eq!(
        fichas(almacen.conexion())[0].comando.as_deref(),
        Some("el-ajeno")
    );
    let pendientes = conflictos::listar_pendientes(almacen.conexion()).expect("conflictos");
    let conflicto = pendientes
        .iter()
        .find(|conflicto| conflicto.tipo == "elemento" && conflicto.uuid == uuid)
        .expect("la edición local perdida queda como conflicto");
    assert_eq!(conflicto.elemento_tipo, "sesion_persistente");

    // «r»: la versión local vuelve como edición nueva de este equipo.
    recuperar_version(almacen.conexion(), conflicto, Path::new(HOGAR)).expect("recuperando");
    let tras = fichas(almacen.conexion());
    assert_eq!(tras.len(), 1);
    assert_eq!(tras[0].comando.as_deref(), Some("el-mío"));
    assert!(
        ms_de(&tras[0].version) > ms,
        "la versión recuperada domina a la ganadora: {}",
        tras[0].version
    );
    let vector: BTreeMap<String, i64> = serde_json::from_str(&tras[0].vector).expect("vector");
    assert!(
        vector.contains_key(EQUIPO_AJENO),
        "hereda el vector ganador"
    );
}

#[test]
fn recuperar_una_ficha_cuyo_nombre_ya_esta_ocupado_da_un_error_claro() {
    use magi_nucleo::sincronizacion::aplicacion::recuperar_version;
    let almacen = almacen();
    let host = crear_host(&almacen, "web");
    let id = insertar_ficha(&almacen, host, "agente", Campos::default(), None);
    let (uuid, version_local, _) = fila_versionada(&almacen, Tabla::SesionesPersistentes, id);
    // La ficha local pierde ante una lápida concurrente y se borra.
    let documento = documento_con(
        EQUIPO_AJENO,
        4,
        Vec::new(),
        vec![lapida_ficha(&uuid, "agente", ms_de(&version_local) + 5_000)],
    );
    let (_, resumen) = fusionar_y_aplicar(&almacen, &[documento]);
    assert_eq!(resumen.borrados, 1);
    let pendientes = conflictos::listar_pendientes(almacen.conexion()).expect("conflictos");
    let conflicto = pendientes
        .iter()
        .find(|conflicto| conflicto.tipo == "elemento" && conflicto.uuid == uuid)
        .expect("la ficha perdida ante la lápida deja conflicto")
        .clone();

    // Mientras tanto se crea otra ficha con el mismo host y nombre.
    insertar_ficha(&almacen, host, "agente", Campos::default(), None);
    let error = recuperar_version(almacen.conexion(), &conflicto, Path::new(HOGAR))
        .expect_err("el nombre está ocupado")
        .to_string();
    assert!(error.contains("ya hay otra"), "{error}");
    assert_eq!(contar_fichas(almacen.conexion()), 1);

    // Libre el nombre, la ficha vuelve (con otra versión, ya sin la lápida
    // por encima).
    almacen
        .conexion()
        .execute("DELETE FROM SESIONES_PERSISTENTES", [])
        .expect("liberando el nombre");
    recuperar_version(almacen.conexion(), &conflicto, Path::new(HOGAR)).expect("recuperando");
    let tras = fichas(almacen.conexion());
    assert_eq!(tras.len(), 1);
    assert_eq!(tras[0].uuid, uuid);
    assert_eq!(tras[0].nombre, "agente");
}

// ------------------------------------------------------- documentación del formato

/// `docs/formato-sincronizacion.md` es la referencia del formato (la app
/// Android y cualquier otro lector la usan): tiene que describir lo que el
/// núcleo escribe de verdad.
#[test]
fn la_documentacion_describe_el_formato_4_y_los_campos_que_viajan() {
    let doc = include_str!("../../../docs/formato-sincronizacion.md");
    assert!(doc.contains("**Formato:** 4"), "la cabecera dice formato 4");
    assert!(doc.contains("`sesion_persistente`"));
    assert!(doc.contains("### 7.8 Sesión persistente (formato 4)"));
    assert!(doc.contains("máximo conocido (4)"), "§13 con el máximo 4");
    assert!(
        doc.contains("Formato 3 → formato 4"),
        "§14 con la viñeta 3→4"
    );
    assert!(doc.contains("Fichas de sesión persistente (formato 4): unión"));
    assert!(doc.contains("## 18. Ejemplo de sesión persistente"));
    assert!(doc.contains("## 19. Cómo abrir un fichero fuera de MAGI"));

    // Todos los campos que viajan de una ficha están en su tabla, y los que no
    // viajan (fechas, estado) no.
    let (_, seccion) = doc
        .split_once("### 7.8 Sesión persistente (formato 4)")
        .expect("sección 7.8");
    let (seccion, _) = seccion.split_once("## 8. Lápidas").expect("fin de la 7.8");
    let datos = magi_nucleo::sincronizacion::elementos::DatosSesionPersistente {
        host: UUID_HOST_REMOTO.to_string(),
        nombre: "agente".to_string(),
        carpeta: None,
        comando: None,
        comando_reanudar: None,
        nota: None,
        origen: "manual".to_string(),
        creada_por: "sobremesa".to_string(),
    };
    let json = serde_json::to_value(datos).expect("datos a JSON");
    for campo in json.as_object().expect("objeto").keys() {
        assert!(
            seccion.contains(&format!("| `{campo}` |")),
            "el campo «{campo}» de la ficha no está en la tabla de §7.8"
        );
    }
    assert!(!seccion.contains("| `creada_en` |"));
    assert!(!seccion.contains("| `actualizado_en` |"));

    // El campo del host y su regla.
    assert!(doc.contains("| `sesiones_persistentes` | booleano |"));
}

// ------------------------------------------------------- lector con máximo anterior

/// Máximo que conocía MAGI 1.4 (y la app Android antes de la Fase 16).
const MAXIMO_ANTERIOR: u32 = 3;

#[test]
fn un_lector_con_el_maximo_anterior_ve_el_4_como_formato_nuevo_y_solo_cuando_toca() {
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = ClaveSincronizacion::generar().texto().to_string();
    let equipo = Entorno::nuevo(&carpeta, "sobremesa", &clave);
    let host = crear_host(&equipo.almacen, "web");

    // Sin fichas ni marca: lo lee un MAGI anterior (formato ≤ 3).
    equipo.pasada();
    let sin_novedades = equipo.formato_del_fichero_propio();
    assert!(
        sin_novedades <= MAXIMO_ANTERIOR,
        "un MAGI anterior lee un inventario sin fichas: formato {sin_novedades}"
    );

    // Con una ficha: un MAGI anterior lo ve como formato nuevo y no importa
    // nada a medias; el actual lo lee.
    insertar_ficha(&equipo.almacen, host, "agente", Campos::default(), None);
    equipo.pasada();
    let con_ficha = equipo.formato_del_fichero_propio();
    assert_eq!(con_ficha, formato::FORMATO_4);
    assert!(
        con_ficha > MAXIMO_ANTERIOR,
        "un MAGI con máximo {MAXIMO_ANTERIOR} lo rechaza como formato_nuevo"
    );
    assert!(con_ficha <= formato::FORMATO_MAXIMO, "el actual sí lo lee");

    // Con una marca de host y sin fichas, lo mismo.
    let ficha_id: i64 = equipo
        .conexion()
        .query_row("SELECT id FROM SESIONES_PERSISTENTES", [], |f| f.get(0))
        .expect("id de la ficha");
    borrar_ficha(&equipo.almacen, ficha_id);
    marcar_host(&equipo.almacen, host, true);
    equipo.pasada();
    assert_eq!(equipo.formato_del_fichero_propio(), formato::FORMATO_4);
}

#[test]
fn el_formato_maximo_conocido_es_el_4_y_un_formato_5_se_rechaza() {
    assert_eq!(formato::FORMATO_MAXIMO, 4);
    let documento = documento_con(EQUIPO_AJENO, 4, Vec::new(), Vec::new());
    let json = formato::serializar(&documento).expect("serializando");
    assert_eq!(formato::parsear(&json).expect("el 4 se lee").formato, 4);
    let documento = documento_con(EQUIPO_AJENO, 5, Vec::new(), Vec::new());
    let json = formato::serializar(&documento).expect("serializando");
    let error = formato::parsear(&json)
        .expect_err("un formato más nuevo no se lee")
        .to_string();
    assert!(error.contains("formato 5"), "{error}");
}

// -------------------------------------------------- tipos que usa la fusión pura

#[test]
fn las_fichas_de_la_fusion_tienen_como_nombre_natural_el_de_la_sesion() {
    // `nombre_de_contenido` y el conflicto usan «nombre»: es el de tmux.
    let contenido = json!({"host": UUID_HOST_REMOTO, "nombre": "agente"});
    assert_eq!(
        magi_nucleo::sincronizacion::fusion::nombre_de_contenido("sesion_persistente", &contenido),
        "agente"
    );
    let candidato = Candidato {
        tipo: "sesion_persistente".to_string(),
        uuid: UUID_MEDIO.to_string(),
        nombre: "agente".to_string(),
        version: version_ajena(1),
        vector: vector_ajeno(1),
        contenido: Some(contenido),
        borrado_en: None,
    };
    assert!(!candidato.es_lapida());
}

// ------------------------------------------------ revisión adversarial (pista C)

/// Un host local con ficha cuya versión en el fichero llega con datos de host
/// que no se pueden leer (el host no se aplica y se conserva) no deja a la
/// ficha apartada con el nombre temporal de la importación.
#[test]
fn una_ficha_de_un_host_que_no_se_puede_leer_no_pierde_su_nombre() {
    let almacen = almacen();
    let host = crear_host(&almacen, "web");
    let uuid_del_host = uuid_host(&almacen, host);
    let id = insertar_ficha(
        &almacen,
        host,
        "agente",
        Campos {
            carpeta: Some("~/viejo"),
            ..Campos::default()
        },
        Some(UUID_MENOR),
    );
    let (_, version_local, _) = fila_versionada(&almacen, Tabla::SesionesPersistentes, id);
    let ms_nuevo = ms_de(&version_local) + 10_000;

    // El host llega con un puerto que no es un número: no se puede leer.
    let mut host_roto = elemento_host(&uuid_del_host, "web", TipoHost::Ssh, false, ms_nuevo);
    host_roto.datos["puerto"] = json!("no-es-un-numero");
    let ficha_nueva = elemento_ficha(
        UUID_MENOR,
        &uuid_del_host,
        "agente",
        Campos {
            carpeta: Some("~/nuevo"),
            ..Campos::default()
        },
        ms_nuevo,
    );
    let documento = documento_con(EQUIPO_AJENO, 4, vec![host_roto, ficha_nueva], Vec::new());
    let _ = fusionar_y_aplicar(&almacen, &[documento]);

    let todas = fichas(almacen.conexion());
    assert_eq!(todas.len(), 1, "la ficha sigue: {todas:?}");
    assert_eq!(
        todas[0].nombre, "agente",
        "la ficha no se queda con el nombre temporal: {todas:?}"
    );
}

/// Una ficha local que el fichero renombra a un nombre que ya ocupa otra fila
/// local fuera de la fusión (aquí, una sin uuid, que no viaja) no se escribe:
/// deja un conflicto de nombre y la ficha, que se había apartado con el nombre
/// temporal, conserva el suyo.
#[test]
fn una_ficha_cuyo_nombre_nuevo_esta_ocupado_por_una_fila_ajena_conserva_el_suyo() {
    let almacen = almacen();
    let host = crear_host(&almacen, "web");
    let uuid_del_host = uuid_host(&almacen, host);
    let id = insertar_ficha(
        &almacen,
        host,
        "agente",
        Campos::default(),
        Some(UUID_MENOR),
    );
    // Una fila sin uuid: no participa en la sincronización, pero ocupa su nombre.
    almacen
        .conexion()
        .execute(
            "INSERT INTO SESIONES_PERSISTENTES (
                host_id, nombre, origen, creada_por, creada_en, actualizado_en
             ) VALUES (?1, 'nuevo', 'manual', 'sobremesa', ?2, ?2)",
            params![host, magi_nucleo::modelo::fecha_ahora()],
        )
        .expect("fila sin uuid");
    let (_, version_local, _) = fila_versionada(&almacen, Tabla::SesionesPersistentes, id);
    let ms_nuevo = ms_de(&version_local) + 10_000;

    let ficha_renombrada = elemento_ficha(
        UUID_MENOR,
        &uuid_del_host,
        "nuevo",
        Campos::default(),
        ms_nuevo,
    );
    let documento = documento_con(EQUIPO_AJENO, 4, vec![ficha_renombrada], Vec::new());
    fusionar_y_aplicar(&almacen, &[documento]);
    let pendientes = conflictos::listar_pendientes(almacen.conexion()).expect("conflictos");
    assert!(
        pendientes.iter().any(|conflicto| conflicto.tipo == "nombre"
            && conflicto.elemento_tipo == "sesion_persistente"
            && conflicto.uuid == UUID_MENOR),
        "conflicto de nombre: {pendientes:?}"
    );

    let mut nombres: Vec<String> = almacen
        .fichas_de_host(host)
        .expect("fichas del host")
        .into_iter()
        .map(|ficha| ficha.nombre)
        .collect();
    nombres.sort();
    assert_eq!(nombres, ["agente"], "la sin uuid sigue sin poder leerse");
    assert!(
        almacen
            .nombres_de_fichas_de_host(host)
            .expect("nombres")
            .iter()
            .all(|nombre| !nombre.starts_with("__magi_sync__")),
        "ninguna se queda con el nombre temporal"
    );
}

/// Integración con el CRUD real del almacén (pista B): lo que escriben
/// `crear_ficha`, `actualizar_ficha`, `renombrar_ficha`, `borrar_ficha` y
/// `fijar_sesiones_persistentes` sube de versión, pasa la validación de la
/// importación y llega al otro equipo; el borrado deja lápida y se propaga.
#[test]
fn el_crud_del_almacen_y_la_sincronizacion_se_entienden() {
    use magi_nucleo::sesiones_persistentes::{DatosFicha, OrigenFicha};

    let (_temporal, a, b, host_uuid) = dos_equipos_con_el_mismo_host();
    let host_a = host_id_por_uuid(&a, &host_uuid);

    // Crear con el CRUD: lo que viaja valida en el otro equipo.
    let id = a
        .almacen
        .crear_ficha(
            host_a,
            &DatosFicha {
                nombre: "agente-api".to_string(),
                carpeta: Some("~/proyectos/api".to_string()),
                comando: Some("claude".to_string()),
                comando_reanudar: Some("claude --continue".to_string()),
                nota: Some("la de la API".to_string()),
                origen: OrigenFicha::Manual,
                creada_por: "sobremesa".to_string(),
            },
        )
        .expect("creando la ficha con el CRUD");
    a.pasada();
    b.pasada();
    let en_b = fichas(b.conexion());
    assert_eq!(en_b.len(), 1, "la ficha llega: {en_b:?}");
    assert_eq!(en_b[0].nombre, "agente-api");
    assert_eq!(en_b[0].carpeta.as_deref(), Some("~/proyectos/api"));
    assert_eq!(
        en_b[0].comando_reanudar.as_deref(),
        Some("claude --continue")
    );
    assert_eq!(en_b[0].creada_por, "sobremesa");
    assert_eq!(fichas(a.conexion()), en_b, "misma ficha y misma versión");
    assert!(
        conflictos::listar_pendientes(b.conexion())
            .expect("conflictos")
            .is_empty(),
        "una ficha creada con el CRUD no deja conflictos"
    );

    // Editar y renombrar con el CRUD suben la versión y viajan.
    let (_, version_antes, _) = fila_versionada(&a.almacen, Tabla::SesionesPersistentes, id);
    a.almacen
        .renombrar_ficha(id, "agente-web")
        .expect("renombrando");
    a.almacen
        .actualizar_ficha(
            id,
            &DatosFicha {
                nombre: "agente-web".to_string(),
                carpeta: Some("~/proyectos/web".to_string()),
                comando: Some("claude".to_string()),
                comando_reanudar: None,
                nota: None,
                origen: OrigenFicha::Manual,
                creada_por: "sobremesa".to_string(),
            },
        )
        .expect("editando");
    let (_, version_despues, _) = fila_versionada(&a.almacen, Tabla::SesionesPersistentes, id);
    assert_ne!(version_antes, version_despues, "editar sube la versión");
    a.pasada();
    b.pasada();
    let en_b = fichas(b.conexion());
    assert_eq!(en_b.len(), 1);
    assert_eq!(en_b[0].nombre, "agente-web");
    assert_eq!(en_b[0].carpeta.as_deref(), Some("~/proyectos/web"));
    assert_eq!(en_b[0].comando_reanudar, None);
    assert_eq!(en_b[0].nota, None);
    assert_eq!(fichas(a.conexion()), en_b);

    // La marca del host con el CRUD pasa el fichero a formato 4 y viaja.
    a.almacen
        .fijar_sesiones_persistentes(host_a, true)
        .expect("marcando el host");
    a.pasada();
    b.pasada();
    assert!(host_marcado(b.conexion(), &host_uuid), "la marca llega");

    // Borrar con el CRUD deja lápida y se propaga; no queda nada que terminar
    // en el host (el almacén no sabe de tmux).
    let uuid_ficha = en_b[0].uuid.clone();
    a.almacen.borrar_ficha(id).expect("borrando la ficha");
    let lapida = lapidas::obtener(a.conexion(), &uuid_ficha)
        .expect("leyendo la lápida")
        .expect("el borrado deja lápida");
    assert_eq!(lapida.tipo, "sesion_persistente");
    assert_eq!(lapida.nombre, "agente-web");
    a.pasada();
    b.pasada();
    assert_eq!(contar_fichas(a.conexion()), 0);
    assert_eq!(
        contar_fichas(b.conexion()),
        0,
        "el borrado llega al otro equipo"
    );
    assert!(lapidas::obtener(b.conexion(), &uuid_ficha)
        .expect("lápida en b")
        .is_some());

    // Y con la ficha borrada y la marca puesta, el formato sigue en 4; al
    // quitar la marca baja.
    assert_eq!(a.formato_del_fichero_propio(), formato::FORMATO_4);
    a.almacen
        .fijar_sesiones_persistentes(host_a, false)
        .expect("desmarcando el host");
    a.pasada();
    assert!(
        a.formato_del_fichero_propio() < formato::FORMATO_4,
        "sin fichas ni marcas vuelve a un formato anterior"
    );
}

/// La unión con fichas creadas por el CRUD real (uuid al azar): los dos
/// equipos acaban con la de uuid menor, sea cual sea.
#[test]
fn dos_fichas_creadas_con_el_crud_con_el_mismo_nombre_se_unen() {
    use magi_nucleo::sesiones_persistentes::{DatosFicha, OrigenFicha};

    for _ in 0..4 {
        let (_temporal, a, b, host_uuid) = dos_equipos_con_el_mismo_host();
        let datos = |carpeta: Option<&str>, comando: Option<&str>, equipo: &str| DatosFicha {
            nombre: "agente".to_string(),
            carpeta: carpeta.map(str::to_string),
            comando: comando.map(str::to_string),
            comando_reanudar: None,
            nota: None,
            origen: OrigenFicha::Manual,
            creada_por: equipo.to_string(),
        };
        let id_a = a
            .almacen
            .crear_ficha(
                host_id_por_uuid(&a, &host_uuid),
                &datos(Some("~/proyectos/api"), None, "sobremesa"),
            )
            .expect("ficha en a");
        let id_b = b
            .almacen
            .crear_ficha(
                host_id_por_uuid(&b, &host_uuid),
                &datos(None, Some("claude"), "portatil"),
            )
            .expect("ficha en b");
        let uuid_a = fila_versionada(&a.almacen, Tabla::SesionesPersistentes, id_a).0;
        let uuid_b = fila_versionada(&b.almacen, Tabla::SesionesPersistentes, id_b).0;
        let menor = uuid_a.clone().min(uuid_b.clone());
        for _ in 0..3 {
            a.pasada();
            b.pasada();
        }
        let en_a = fichas(a.conexion());
        let en_b = fichas(b.conexion());
        assert_eq!(en_a.len(), 1, "{en_a:?}");
        assert_eq!(en_a, en_b);
        assert_eq!(en_a[0].uuid, menor, "queda la de uuid menor");
        assert_eq!(en_a[0].carpeta.as_deref(), Some("~/proyectos/api"));
        assert_eq!(en_a[0].comando.as_deref(), Some("claude"));
    }
}
