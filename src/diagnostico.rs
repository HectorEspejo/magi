//! `magi diagnostico`: comprueba lo que MAGI necesita en este sistema.
//!
//! Todo lo que se observa queda en `Datos` y la interpretación es pura
//! (`comprobar`): así las pruebas simulan un sistema sin portapapeles, una
//! base de datos de una versión futura o un locale ASCII sin depender de la
//! máquina que las ejecuta.
//!
//! El subcomando no escribe ficheros, no arranca el servidor, no lee secretos
//! del llavero y no abre conexiones de red.

use std::collections::BTreeMap;
use std::io::{BufRead as _, IsTerminal as _, Read as _, Write};
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use nix::unistd::{access, AccessFlags};
use rusqlite::{Connection, OpenFlags};

use crate::almacen::migraciones::VERSION_BD;
use crate::config::{Config, Rutas};
use crate::protocolo::{MensajeCliente, MensajeServidor, VERSION_PROTOCOLO};
use crate::servidor;
use crate::tema;

/// Ancho de la columna con el nombre de la comprobación.
const ANCHO_NOMBRE: usize = 15;

/// Longitud del socket a partir de la cual el servidor no arranca (R41).
const SOCKET_MAXIMO: usize = 108;

/// Longitud del socket a partir de la cual conviene acortar `XDG_RUNTIME_DIR`.
const SOCKET_AVISO: usize = 100;

// ------------------------------------------------------------------- datos

/// Estado de un directorio de MAGI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EstadoDirectorio {
    pub ruta: PathBuf,
    pub existe: bool,
    pub es_directorio: bool,
    /// Bits de modo (`0o700`) si se pudieron leer.
    pub permisos: Option<u32>,
    /// Si se puede escribir en él o crearlo (permiso sobre el padre).
    pub escribible: bool,
}

/// Cómo se pudo leer `config.toml`.
#[derive(Debug, Clone)]
pub enum ResultadoConfig {
    /// No existe; al arrancar se creará con los valores por defecto.
    Ausente,
    /// Se leyó y se entiende.
    Valida(Box<Config>),
    /// No se puede usar: sintaxis rota o error de lectura.
    Invalida(String),
}

impl ResultadoConfig {
    /// La configuración que usaría MAGI al arrancar (`Config::default` si no
    /// hay una válida).
    pub fn config(&self) -> Config {
        match self {
            Self::Valida(config) => config.as_ref().clone(),
            Self::Ausente | Self::Invalida(_) => Config::default(),
        }
    }
}

/// Lo que se pudo averiguar de `magi.db` sin modificarla.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultadoBaseDatos {
    NoExiste,
    Version(i64),
    Futura(i64),
    Error(String),
}

/// Respuesta del servidor de sesiones por el socket local.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServidorObservado {
    /// No hay socket: no está en marcha y se arrancará solo al abrir Magi.
    Apagado,
    EnMarcha {
        version_app: String,
        protocolo: u32,
        sesiones: u32,
        transferencias: u32,
        tuneles: u32,
        ejecuciones: u32,
    },
    OtraVersion {
        version_app: Option<String>,
        protocolo: u32,
    },
    /// El socket existe pero no contesta como un servidor de MAGI.
    SinRespuesta,
}

/// Lo que se pudo averiguar del agente SSH.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgenteObservado {
    SinVariable,
    SinClaves(String),
    Claves(usize),
}

/// Un equipo de la sincronización del inventario, leído de EQUIPOS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquipoObservado {
    pub nombre: String,
    pub plataforma: String,
    /// `este`, `al_dia`, `ilegible`, `danado`, `formato_nuevo` o `quitado`.
    pub estado: String,
    pub ultimo_fichero_en: Option<String>,
}

/// Lo que se pudo averiguar de los equipos y conflictos de la sincronización.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultadoEquipos {
    /// `magi.db` no existe o es anterior a la Fase 10: todavía no hay equipos.
    SinDatos,
    /// No se pudo leer EQUIPOS (base de datos futura o error de lectura).
    NoLegible(String),
    Equipos(Vec<EquipoObservado>),
}

/// Lo que se observó de la sincronización del inventario, sin leer nunca la
/// clave del llavero ni descifrar ficheros.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SincronizacionObservada {
    /// Sin carpeta en config.toml: la sincronización está desactivada.
    #[default]
    Desactivada,
    Configurada {
        carpeta: EstadoDirectorio,
        equipos: ResultadoEquipos,
        /// Conflictos pendientes, si se pudieron contar.
        conflictos_pendientes: Option<i64>,
        /// Copias en conflicto de la herramienta de sincronización (Syncthing,
        /// Nextcloud…) que hay en la carpeta.
        copias_conflicto: usize,
    },
}

/// Hechos observados del sistema. Se construye con `recoger` en producción y
/// a mano en las pruebas: la interpretación (`comprobar`) no toca el sistema.
#[derive(Debug, Clone)]
pub struct Datos {
    pub version_app: String,
    pub protocolo: u32,
    pub commit: String,
    pub rutas: Rutas,
    pub directorios: Directorios,
    pub config: ResultadoConfig,
    pub base_datos: ResultadoBaseDatos,
    /// Sincronización del inventario: carpeta, equipos y conflictos.
    pub sincronizacion: SincronizacionObservada,
    pub servidor: ServidorObservado,
    pub agente: AgenteObservado,
    /// Programas que importan y si están disponibles (en el `PATH` y
    /// ejecutables). En las pruebas se rellena a mano.
    pub binarios: BTreeMap<String, bool>,
    /// Variables de entorno del proceso, para no leerlas en la interpretación.
    pub variables: BTreeMap<String, String>,
    /// `~/.ssh/config` incluye el `magi_config`.
    pub incluye_magi_config: bool,
    /// Modo degradado ASCII (locale no UTF-8, `MAGI_ASCII` o configuración).
    pub ascii: bool,
}

/// Directorios de MAGI con su estado.
#[derive(Debug, Clone)]
pub struct Directorios {
    pub datos: EstadoDirectorio,
    pub configuracion: EstadoDirectorio,
    pub estado: EstadoDirectorio,
    pub runtime: EstadoDirectorio,
}

impl Datos {
    /// ¿Está el programa en el `PATH` (según lo observado)?
    pub fn disponible(&self, programa: &str) -> bool {
        if programa.contains('/') {
            return es_ejecutable(Path::new(programa));
        }
        self.binarios.get(programa).copied().unwrap_or(false)
    }

    /// Ruta del socket del servidor de sesiones.
    pub fn ruta_socket(&self) -> PathBuf {
        servidor::ruta_socket(&self.rutas)
    }
}

// ------------------------------------------------------------------ informe

/// Resultado de una comprobación.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Estado {
    Correcto,
    Aviso,
    Error,
}

/// Una línea del diagnóstico.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comprobacion {
    pub nombre: &'static str,
    pub estado: Estado,
    /// Texto principal de la línea.
    pub detalle: String,
    /// Instrucción de arreglo; se imprime tras una flecha.
    pub arreglo: Option<String>,
    /// Línea extra explicativa, alineada debajo.
    pub nota: Option<String>,
}

impl Comprobacion {
    fn correcto(nombre: &'static str, detalle: String) -> Self {
        Self {
            nombre,
            estado: Estado::Correcto,
            detalle,
            arreglo: None,
            nota: None,
        }
    }

    fn aviso(nombre: &'static str, detalle: String, arreglo: Option<String>) -> Self {
        Self {
            nombre,
            estado: Estado::Aviso,
            detalle,
            arreglo,
            nota: None,
        }
    }

    fn error(nombre: &'static str, detalle: String, arreglo: Option<String>) -> Self {
        Self {
            nombre,
            estado: Estado::Error,
            detalle,
            arreglo,
            nota: None,
        }
    }

    fn con_nota(mut self, nota: Option<String>) -> Self {
        self.nota = nota;
        self
    }
}

/// Informe completo de `magi diagnostico`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Informe {
    pub comprobaciones: Vec<Comprobacion>,
    pub avisos: usize,
    pub errores: usize,
}

impl Informe {
    /// Código de salida: 0 sin errores (los avisos no cuentan) y 1 con alguno.
    pub fn codigo_salida(&self) -> i32 {
        if self.errores > 0 {
            1
        } else {
            0
        }
    }

    /// Resumen final: «2 avisos, 0 errores».
    pub fn resumen(&self) -> String {
        format!(
            "{} {}, {} {}",
            self.avisos,
            if self.avisos == 1 { "aviso" } else { "avisos" },
            self.errores,
            if self.errores == 1 {
                "error"
            } else {
                "errores"
            }
        )
    }
}

/// Opciones de presentación.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opciones {
    pub color: bool,
    pub ascii: bool,
}

/// Decide la presentación: sin color si `NO_COLOR` está definido o la salida
/// no es una terminal; en ASCII si el entorno lo pide.
pub fn opciones(datos: &Datos, es_terminal: bool) -> Opciones {
    Opciones {
        color: es_terminal && !datos.variables.contains_key("NO_COLOR"),
        ascii: datos.ascii,
    }
}

// ---------------------------------------------------------------- recogida

/// Observa el sistema sin modificar nada.
pub fn recoger(rutas: Rutas) -> Datos {
    let variables = variables_de_entorno();
    let config = leer_config(&rutas.fichero_config());
    let valores = config.config();
    let ascii = tema::ascii_por_entorno(&valores);
    let binarios = programas_necesarios(&valores, &variables)
        .into_iter()
        .map(|programa| {
            let disponible = disponible_en_path(&programa);
            (programa, disponible)
        })
        .collect();
    let incluye_magi_config = crate::sshconfig::exportar::comprobar_include(
        &rutas.fichero_ssh_config(),
        &rutas.fichero_magi_config(),
    );
    let servidor = observar_servidor(&rutas);
    let agente = observar_agente(&variables);
    let base_datos = observar_base_datos(&rutas.base_datos());
    let sincronizacion = observar_sincronizacion(&rutas, &valores, &base_datos);
    let directorios = Directorios {
        datos: observar_directorio(&rutas.datos),
        configuracion: observar_directorio(&rutas.config),
        estado: observar_directorio(&rutas.estado),
        runtime: observar_directorio(&rutas.dir_runtime()),
    };
    Datos {
        version_app: crate::VERSION_APP.to_string(),
        protocolo: VERSION_PROTOCOLO,
        commit: crate::COMMIT.to_string(),
        rutas,
        directorios,
        config,
        base_datos,
        sincronizacion,
        servidor,
        agente,
        binarios,
        variables,
        incluye_magi_config,
        ascii,
    }
}

fn variables_de_entorno() -> BTreeMap<String, String> {
    std::env::vars_os()
        .filter_map(|(clave, valor)| Some((clave.into_string().ok()?, valor.into_string().ok()?)))
        .collect()
}

/// Lee `config.toml` sin crearlo ni corregirlo.
fn leer_config(ruta: &Path) -> ResultadoConfig {
    match std::fs::read_to_string(ruta) {
        Ok(texto) => match toml::from_str::<Config>(&texto) {
            Ok(config) => ResultadoConfig::Valida(Box::new(config)),
            Err(error) => ResultadoConfig::Invalida(error.to_string()),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => ResultadoConfig::Ausente,
        Err(error) => ResultadoConfig::Invalida(format!("no se puede leer: {error}")),
    }
}

/// Programas cuya disponibilidad necesita el diagnóstico: las herramientas
/// fijas, el llavero del sistema y los comandos de la configuración y el
/// entorno (editor, paginador y terminal).
fn programas_necesarios(config: &Config, variables: &BTreeMap<String, String>) -> Vec<String> {
    let mut programas: Vec<String> = Vec::new();
    for fijo in ["wl-copy", "pbcopy", "nvim", "vi", "less", "more", "ssh-add"] {
        anadir_programa(&mut programas, fijo);
    }
    anadir_programa(
        &mut programas,
        if cfg!(target_os = "macos") {
            "security"
        } else {
            "secret-tool"
        },
    );
    let mut comandos = vec![
        config.terminal.comando.as_str(),
        config.archivos.editor.as_str(),
        config.archivos.pager.as_str(),
    ];
    if let Some(valor) = variables.get("VISUAL") {
        comandos.push(valor);
    }
    if let Some(valor) = variables.get("EDITOR") {
        comandos.push(valor);
    }
    if let Some(valor) = variables.get("PAGER") {
        comandos.push(valor);
    }
    for comando in comandos {
        if let Some((programa, _)) = crate::visor::comando(comando) {
            anadir_programa(&mut programas, &programa);
        }
    }
    programas
}

fn anadir_programa(programas: &mut Vec<String>, programa: &str) {
    if !programas.iter().any(|existente| existente == programa) {
        programas.push(programa.to_string());
    }
}

/// ¿Está el programa en el `PATH` y es ejecutable?
fn disponible_en_path(programa: &str) -> bool {
    if programa.contains('/') {
        return es_ejecutable(Path::new(programa));
    }
    let Some(camino) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&camino).any(|directorio| es_ejecutable(&directorio.join(programa)))
}

fn es_ejecutable(ruta: &Path) -> bool {
    std::fs::metadata(ruta)
        .map(|metadatos| metadatos.is_file() && metadatos.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Estado de un directorio: existencia, tipo, permisos y si se puede escribir.
fn observar_directorio(ruta: &Path) -> EstadoDirectorio {
    match std::fs::metadata(ruta) {
        Ok(metadatos) => EstadoDirectorio {
            ruta: ruta.to_path_buf(),
            existe: true,
            es_directorio: metadatos.is_dir(),
            permisos: Some(metadatos.permissions().mode() & 0o777),
            escribible: metadatos.is_dir() && access(ruta, AccessFlags::W_OK).is_ok(),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let escribible = se_puede_crear(ruta);
            EstadoDirectorio {
                ruta: ruta.to_path_buf(),
                existe: false,
                es_directorio: false,
                permisos: None,
                escribible,
            }
        }
        Err(_) => EstadoDirectorio {
            ruta: ruta.to_path_buf(),
            existe: true,
            es_directorio: true,
            permisos: None,
            escribible: false,
        },
    }
}

/// ¿Se puede crear `ruta`? Basta con que el ancestro existente más cercano
/// sea un directorio en el que se pueda escribir: MAGI crea los directorios
/// que falten con `create_dir_all`.
fn se_puede_crear(ruta: &Path) -> bool {
    ruta.ancestors()
        .skip(1)
        .find_map(|ancestro| {
            std::fs::metadata(ancestro)
                .ok()
                .map(|metadatos| (ancestro, metadatos))
        })
        .is_some_and(|(ancestro, metadatos)| {
            metadatos.is_dir() && access(ancestro, AccessFlags::W_OK).is_ok()
        })
}

/// Ruta de un fichero lateral de SQLite (`magi.db-wal`, `-shm`, `-journal`).
fn ruta_lateral(ruta: &Path, sufijo: &str) -> PathBuf {
    let mut texto = ruta.as_os_str().to_os_string();
    texto.push(sufijo);
    PathBuf::from(texto)
}

/// ¿La cabecera SQLite dice que la base de datos está en modo WAL? Bytes 18 y
/// 19 de la cabecera son las versiones de escritura/lectura; 2 = WAL.
fn cabecera_en_wal(ruta: &Path) -> std::io::Result<bool> {
    let mut cabecera = [0u8; 20];
    let mut fichero = std::fs::File::open(ruta)?;
    let leidos = fichero.read(&mut cabecera)?;
    Ok(leidos >= 20 && (cabecera[18] == 2 || cabecera[19] == 2))
}

/// ¿Abrir la base de datos en su sitio podría escribir ficheros laterales?
/// Con un WAL pendiente o un journal caliente SQLite crea o toca `-wal`,
/// `-shm` o `-journal` incluso en una conexión de solo lectura.
fn tiene_laterales(ruta: &Path) -> bool {
    ["-wal", "-shm", "-journal"]
        .iter()
        .any(|sufijo| ruta_lateral(ruta, sufijo).exists())
        || cabecera_en_wal(ruta).unwrap_or(false)
}

/// Abre la base de datos en solo lectura y lee su `user_version`.
fn leer_version(ruta: &Path, solo_lectura: bool) -> ResultadoBaseDatos {
    let conexion = if solo_lectura {
        Connection::open_with_flags(ruta, OpenFlags::SQLITE_OPEN_READ_ONLY)
    } else {
        Connection::open(ruta)
    };
    let conexion = match conexion {
        Ok(conexion) => conexion,
        Err(error) => return ResultadoBaseDatos::Error(format!("no se puede abrir: {error}")),
    };
    match conexion.query_row("PRAGMA user_version", [], |fila| fila.get::<_, i64>(0)) {
        Ok(version) if version > VERSION_BD => ResultadoBaseDatos::Futura(version),
        Ok(version) => ResultadoBaseDatos::Version(version),
        Err(error) => ResultadoBaseDatos::Error(error.to_string()),
    }
}

/// Lee la versión de `magi.db` sin escribir nada en su directorio: si hay
/// ficheros laterales (WAL, shm o journal) se abre una copia en un directorio
/// temporal, porque abrir el original crearía o modificaría los laterales
/// (Fase 9: el diagnóstico no escribe ficheros).
fn observar_base_datos(ruta: &Path) -> ResultadoBaseDatos {
    if !ruta.exists() {
        return ResultadoBaseDatos::NoExiste;
    }
    match con_fichero_legible(ruta, |ruta, es_original| {
        Ok(leer_version(ruta, es_original))
    }) {
        Ok(resultado) => resultado,
        Err(mensaje) => ResultadoBaseDatos::Error(mensaje),
    }
}

/// Ejecuta una lectura sobre `magi.db` sin crear ni tocar ficheros laterales:
/// directamente si no los hay, o sobre una copia temporal si los hay (WAL
/// pendiente o journal caliente). `leer` recibe la ruta y si es el original
/// (que se abre en solo lectura) o la copia (que se puede abrir normal para
/// que SQLite aplique el WAL).
fn con_fichero_legible<T>(
    ruta: &Path,
    leer: impl FnOnce(&Path, bool) -> std::result::Result<T, String>,
) -> std::result::Result<T, String> {
    if !tiene_laterales(ruta) {
        return leer(ruta, true);
    }
    let marca = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duracion| duracion.as_nanos())
        .unwrap_or(0);
    let temporal =
        std::env::temp_dir().join(format!("magi-diagnostico-{}-{marca}", std::process::id()));
    if let Err(error) = std::fs::create_dir(&temporal) {
        return Err(format!(
            "no se puede crear un temporal para comprobar la base de datos: {error}"
        ));
    }
    let resultado = (|| -> std::result::Result<T, String> {
        let copia = temporal.join("magi.db");
        if let Err(error) = std::fs::copy(ruta, &copia) {
            return Err(format!(
                "no se puede copiar la base de datos para comprobarla: {error}"
            ));
        }
        for sufijo in ["-wal", "-shm", "-journal"] {
            let origen = ruta_lateral(ruta, sufijo);
            if origen.exists() {
                let destino = ruta_lateral(&copia, sufijo);
                if let Err(error) = std::fs::copy(&origen, &destino) {
                    return Err(format!(
                        "no se puede copiar {sufijo} para comprobar la base de datos: {error}"
                    ));
                }
            }
        }
        leer(&copia, false)
    })();
    let _ = std::fs::remove_dir_all(&temporal);
    resultado
}

/// Observa la sincronización del inventario sin leer la clave del llavero ni
/// descifrar ningún fichero: carpeta, equipos (leyendo EQUIPOS), conflictos
/// pendientes y copias en conflicto de la herramienta de sincronización.
fn observar_sincronizacion(
    rutas: &Rutas,
    config: &Config,
    base_datos: &ResultadoBaseDatos,
) -> SincronizacionObservada {
    let Some(carpeta) = config.sincronizacion.ruta_carpeta(&rutas.hogar) else {
        return SincronizacionObservada::Desactivada;
    };
    let estado = observar_directorio(&carpeta);
    let utilizable = estado.existe && estado.es_directorio;
    let (equipos, conflictos_pendientes) = if !utilizable {
        (ResultadoEquipos::SinDatos, None)
    } else {
        match base_datos {
            ResultadoBaseDatos::Version(version) if *version >= VERSION_BD => {
                match leer_equipos_y_conflictos(&rutas.base_datos()) {
                    Ok((equipos, pendientes)) => {
                        (ResultadoEquipos::Equipos(equipos), Some(pendientes))
                    }
                    Err(mensaje) => (ResultadoEquipos::NoLegible(mensaje), None),
                }
            }
            // Sin base de datos o con una anterior a la Fase 10: aún no hay
            // equipos; MAGI la migrará al abrirse.
            ResultadoBaseDatos::NoExiste | ResultadoBaseDatos::Version(_) => {
                (ResultadoEquipos::SinDatos, None)
            }
            ResultadoBaseDatos::Futura(_) => (
                ResultadoEquipos::NoLegible("magi.db es de una versión más nueva".to_string()),
                None,
            ),
            ResultadoBaseDatos::Error(_) => (
                ResultadoEquipos::NoLegible("no se pudo abrir magi.db".to_string()),
                None,
            ),
        }
    };
    let copias_conflicto = if utilizable {
        contar_copias_conflicto(&carpeta)
    } else {
        0
    };
    SincronizacionObservada::Configurada {
        carpeta: estado,
        equipos,
        conflictos_pendientes,
        copias_conflicto,
    }
}

/// Lee los equipos y los conflictos pendientes de `magi.db` sin crear ficheros
/// laterales (si hay WAL o journal, sobre una copia temporal).
fn leer_equipos_y_conflictos(
    ruta: &Path,
) -> std::result::Result<(Vec<EquipoObservado>, i64), String> {
    con_fichero_legible(ruta, |ruta, es_original| {
        let conexion = if es_original {
            Connection::open_with_flags(ruta, OpenFlags::SQLITE_OPEN_READ_ONLY)
        } else {
            Connection::open(ruta)
        }
        .map_err(|error| format!("no se puede abrir magi.db: {error}"))?;
        let mut sentencia = conexion
            .prepare(
                "SELECT nombre, plataforma, estado, ultimo_fichero_en
                   FROM EQUIPOS ORDER BY es_este DESC, nombre",
            )
            .map_err(|error| format!("no se puede leer EQUIPOS: {error}"))?;
        let filas = sentencia
            .query_map([], |fila| {
                Ok(EquipoObservado {
                    nombre: fila.get(0)?,
                    plataforma: fila.get(1)?,
                    estado: fila.get(2)?,
                    ultimo_fichero_en: fila.get(3)?,
                })
            })
            .map_err(|error| format!("no se puede leer EQUIPOS: {error}"))?;
        let equipos = filas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| format!("no se puede leer EQUIPOS: {error}"))?;
        let pendientes: i64 = conexion
            .query_row(
                "SELECT COUNT(*) FROM CONFLICTOS WHERE resuelto_en IS NULL",
                [],
                |fila| fila.get(0),
            )
            .map_err(|error| format!("no se puede leer CONFLICTOS: {error}"))?;
        Ok((equipos, pendientes))
    })
}

/// Copias en conflicto de las herramientas de sincronización de ficheros en la
/// carpeta compartida (Syncthing, Nextcloud, iCloud Drive…).
fn contar_copias_conflicto(carpeta: &Path) -> usize {
    let Ok(entradas) = std::fs::read_dir(carpeta) else {
        return 0;
    };
    entradas
        .filter_map(|entrada| entrada.ok())
        .filter(|entrada| entrada.path().is_file())
        .filter(|entrada| {
            entrada.file_name().to_str().is_some_and(|nombre| {
                nombre.starts_with("magi-")
                    && (nombre.contains("sync-conflict") || nombre.contains("conflicted copy"))
            })
        })
        .count()
}

/// Consulta el servidor por el socket local: nunca lo arranca.
fn observar_servidor(rutas: &Rutas) -> ServidorObservado {
    use std::os::unix::net::UnixStream;

    let ruta = servidor::ruta_socket(rutas);
    let mut flujo = match UnixStream::connect(&ruta) {
        Ok(flujo) => flujo,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return ServidorObservado::Apagado
        }
        Err(_) => return ServidorObservado::SinRespuesta,
    };
    let plazo = Some(Duration::from_secs(2));
    let _ = flujo.set_read_timeout(plazo);
    let _ = flujo.set_write_timeout(plazo);
    let saludo = MensajeCliente::Hola {
        version: VERSION_PROTOCOLO,
        pid: std::process::id(),
    };
    let Ok(linea) = crate::protocolo::codificar(&saludo) else {
        return ServidorObservado::SinRespuesta;
    };
    let escrito = flujo
        .write_all(linea.as_bytes())
        .and_then(|()| flujo.write_all(b"\n"))
        .and_then(|()| flujo.flush());
    if escrito.is_err() {
        return ServidorObservado::SinRespuesta;
    }
    let mut lector = std::io::BufReader::new(flujo);
    let mut respuesta = String::new();
    if lector.read_line(&mut respuesta).is_err() || respuesta.trim().is_empty() {
        return ServidorObservado::SinRespuesta;
    }
    let respuesta = respuesta.trim_end();
    let Ok(valor) = serde_json::from_str::<serde_json::Value>(respuesta) else {
        return ServidorObservado::SinRespuesta;
    };
    match valor.get("tipo").and_then(serde_json::Value::as_str) {
        Some("Bienvenida") => {
            let Ok(MensajeServidor::Bienvenida {
                version,
                version_app,
                sesiones,
                transferencias,
                tuneles,
                ejecuciones,
                ..
            }) = crate::protocolo::decodificar::<MensajeServidor>(respuesta)
            else {
                return ServidorObservado::SinRespuesta;
            };
            if version != VERSION_PROTOCOLO {
                return ServidorObservado::OtraVersion {
                    version_app: Some(version_app),
                    protocolo: version,
                };
            }
            ServidorObservado::EnMarcha {
                version_app,
                protocolo: version,
                // Mismo criterio que EstadoServidor::trabajo y servidor
                // estado: solo lo que está vivo de verdad.
                sesiones: sesiones.len() as u32,
                transferencias: transferencias
                    .iter()
                    .filter(|transferencia| !transferencia.estado.terminada())
                    .count() as u32,
                tuneles: tuneles
                    .iter()
                    .filter(|tunel| tunel.estado.en_marcha())
                    .count() as u32,
                ejecuciones: ejecuciones
                    .iter()
                    .filter(|ejecucion| {
                        ejecucion.estado == crate::protocolo::EstadoEjecucion::EnCurso
                    })
                    .count() as u32,
            }
        }
        Some("VersionIncompatible") => {
            let version = valor
                .get("version")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as u32;
            let version_app = valor
                .get("version_app")
                .and_then(serde_json::Value::as_str)
                .filter(|texto| !texto.is_empty())
                .map(str::to_string);
            ServidorObservado::OtraVersion {
                version_app,
                protocolo: version,
            }
        }
        _ => ServidorObservado::SinRespuesta,
    }
}

/// Ejecuta un programa con plazo duro. Devuelve estado y stdout si termina;
/// None si no arranca o si vence el plazo (el hijo se mata y se recoge).
fn ejecutar_con_plazo(
    programa: &str,
    argumentos: &[&str],
    plazo: Duration,
) -> Option<(ExitStatus, Vec<u8>)> {
    let mut hijo = Command::new(programa)
        .args(argumentos)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut salida = hijo.stdout.take()?;
    let lector = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = salida.read_to_end(&mut buffer);
        buffer
    });
    let inicio = Instant::now();
    loop {
        match hijo.try_wait() {
            Ok(Some(estado)) => {
                let stdout = lector.join().unwrap_or_default();
                return Some((estado, stdout));
            }
            Ok(None) if inicio.elapsed() >= plazo => {
                let _ = hijo.kill();
                let _ = hijo.wait();
                let _ = lector.join();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => {
                let _ = hijo.kill();
                let _ = hijo.wait();
                let _ = lector.join();
                return None;
            }
        }
    }
}

/// Cuenta las claves del agente con `ssh-add -l`: no lee ninguna clave. Con
/// plazo: un agente que acepta la conexión y no responde no debe colgar el
/// diagnóstico.
fn observar_agente(variables: &BTreeMap<String, String>) -> AgenteObservado {
    let Some(socket) = variables.get("SSH_AUTH_SOCK") else {
        return AgenteObservado::SinVariable;
    };
    if socket.trim().is_empty() {
        return AgenteObservado::SinVariable;
    }
    let Some((estado, stdout)) = ejecutar_con_plazo("ssh-add", &["-l"], Duration::from_secs(2))
    else {
        return AgenteObservado::SinClaves(
            "el agente SSH no ha respondido en 2 s (¿está colgado?)".to_string(),
        );
    };
    if estado.success() {
        let texto = String::from_utf8_lossy(&stdout);
        let claves = texto
            .lines()
            .filter(|linea| !linea.trim().is_empty())
            .count();
        if claves == 0 {
            AgenteObservado::SinClaves("el agente no tiene claves cargadas".to_string())
        } else {
            AgenteObservado::Claves(claves)
        }
    } else {
        match estado.code() {
            Some(1) => AgenteObservado::SinClaves("el agente no tiene claves cargadas".to_string()),
            Some(2) => {
                AgenteObservado::SinClaves("no se pudo conectar con el agente SSH".to_string())
            }
            _ => AgenteObservado::SinClaves(format!("ssh-add salió con {estado}")),
        }
    }
}

// ------------------------------------------------------------- comprobación

/// Calcula el informe a partir de los hechos observados: no toca el sistema.
pub fn comprobar(datos: &Datos) -> Informe {
    let config = datos.config.config();
    let comprobaciones = vec![
        comprobar_datos(datos),
        comprobar_configuracion(datos),
        comprobar_estado(datos),
        comprobar_runtime(datos),
        comprobar_servidor(datos),
        comprobar_agente(datos),
        comprobar_llavero(datos),
        comprobar_portapapeles(datos),
        comprobar_editor(datos, &config),
        comprobar_paginador(datos, &config),
        comprobar_terminal(datos, &config),
        comprobar_ssh_config(datos),
        comprobar_sincronizacion(datos),
    ];
    let avisos = comprobaciones
        .iter()
        .filter(|comprobacion| comprobacion.estado == Estado::Aviso)
        .count();
    let errores = comprobaciones
        .iter()
        .filter(|comprobacion| comprobacion.estado == Estado::Error)
        .count();
    Informe {
        comprobaciones,
        avisos,
        errores,
    }
}

/// Ruta con el `~` del hogar para que la salida se lea como en el informe.
fn ruta_legible(ruta: &Path, hogar: &Path) -> String {
    match ruta.strip_prefix(hogar) {
        Ok(relativa) => {
            let relativa = relativa.display().to_string();
            if relativa.is_empty() {
                "~".to_string()
            } else {
                format!("~/{relativa}")
            }
        }
        Err(_) => ruta.display().to_string(),
    }
}

/// Junta dos partes de una misma comprobación; el arreglo es el de la parte
/// que peor está.
fn combinar(
    primera: (Estado, String, Option<String>),
    segunda: (Estado, String, Option<String>),
) -> (Estado, String, Option<String>) {
    let estado = primera.0.max(segunda.0);
    let detalle = format!("{} · {}", primera.1, segunda.1);
    let arreglo = if segunda.0 == estado {
        segunda.2.or(primera.2)
    } else {
        primera.2.or(segunda.2)
    };
    (estado, detalle, arreglo)
}

/// Estado de un directorio como parte de una comprobación: permisos 700 y
/// escritura. Si no existe, dice si se podrá crear.
fn estado_directorio(estado: &EstadoDirectorio, hogar: &Path) -> (Estado, String, Option<String>) {
    let ruta = ruta_legible(&estado.ruta, hogar);
    if !estado.existe {
        if estado.escribible {
            return (
                Estado::Correcto,
                format!("{ruta} no existe; se creará al arrancar"),
                None,
            );
        }
        let padre = estado
            .ruta
            .parent()
            .map(|padre| ruta_legible(padre, hogar))
            .unwrap_or_else(|| "el directorio padre".to_string());
        return (
            Estado::Error,
            format!("{ruta} no existe y no se puede crear"),
            Some(format!("revisa los permisos de {padre}")),
        );
    }
    if !estado.es_directorio {
        return (
            Estado::Error,
            format!("{ruta} no es un directorio"),
            Some(format!("mueve o borra {ruta}")),
        );
    }
    let Some(permisos) = estado.permisos else {
        return (
            Estado::Error,
            format!("{ruta}: no se pueden leer sus permisos"),
            Some(format!("revisa los permisos de {ruta}")),
        );
    };
    if !estado.escribible {
        return (
            Estado::Error,
            format!("{ruta} ({permisos:03o}): no se puede escribir"),
            Some(format!("chmod u+w {ruta}")),
        );
    }
    if permisos & 0o077 != 0 {
        return (
            Estado::Aviso,
            format!("{ruta} ({permisos:03o}): permisos más abiertos de lo recomendado (700)"),
            Some(format!("chmod 700 {ruta}")),
        );
    }
    (Estado::Correcto, format!("{ruta} ({permisos:03o})"), None)
}

fn comprobar_datos(datos: &Datos) -> Comprobacion {
    let (estado, detalle, arreglo) =
        estado_directorio(&datos.directorios.datos, &datos.rutas.hogar);
    if !datos.directorios.datos.existe {
        return Comprobacion {
            nombre: "datos",
            estado,
            detalle: format!("{detalle} · magi.db se creará al arrancar"),
            arreglo,
            nota: None,
        };
    }
    let (estado_bd, detalle_bd, arreglo_bd) = match &datos.base_datos {
        ResultadoBaseDatos::NoExiste => (
            Estado::Correcto,
            "magi.db se creará al arrancar".to_string(),
            None,
        ),
        ResultadoBaseDatos::Version(version) => {
            (Estado::Correcto, format!("magi.db versión {version}"), None)
        }
        ResultadoBaseDatos::Futura(version) => (
            Estado::Error,
            format!("magi.db es de la versión {version}; este MAGI conoce la {VERSION_BD}"),
            Some("actualiza MAGI a la última versión y no borres magi.db".to_string()),
        ),
        ResultadoBaseDatos::Error(motivo) => (
            Estado::Error,
            format!("magi.db no se puede abrir en solo lectura: {motivo}"),
            Some("revisa los permisos de magi.db".to_string()),
        ),
    };
    let (estado, detalle, arreglo) = combinar(
        (estado, detalle, arreglo),
        (estado_bd, detalle_bd, arreglo_bd),
    );
    Comprobacion {
        nombre: "datos",
        estado,
        detalle,
        arreglo,
        nota: None,
    }
}

fn comprobar_configuracion(datos: &Datos) -> Comprobacion {
    let (estado, detalle, arreglo) =
        estado_directorio(&datos.directorios.configuracion, &datos.rutas.hogar);
    let (estado_config, detalle_config, arreglo_config) = match &datos.config {
        ResultadoConfig::Ausente => (
            Estado::Correcto,
            "config.toml no existe; se creará con los valores por defecto".to_string(),
            None,
        ),
        ResultadoConfig::Valida(_) => (Estado::Correcto, "config.toml correcto".to_string(), None),
        ResultadoConfig::Invalida(motivo) => (
            Estado::Error,
            format!("config.toml no válido: {motivo}"),
            Some("corrige config.toml y vuelve a ejecutar el diagnóstico".to_string()),
        ),
    };
    let (estado, detalle, arreglo) = combinar(
        (estado, detalle, arreglo),
        (estado_config, detalle_config, arreglo_config),
    );
    Comprobacion {
        nombre: "configuración",
        estado,
        detalle,
        arreglo,
        nota: None,
    }
}

fn comprobar_estado(datos: &Datos) -> Comprobacion {
    let (estado, detalle, arreglo) =
        estado_directorio(&datos.directorios.estado, &datos.rutas.hogar);
    Comprobacion {
        nombre: "estado",
        estado,
        detalle: format!("{detalle} · logs"),
        arreglo,
        nota: None,
    }
}

fn comprobar_runtime(datos: &Datos) -> Comprobacion {
    let (estado, detalle, arreglo) =
        estado_directorio(&datos.directorios.runtime, &datos.rutas.hogar);
    let socket = datos.ruta_socket();
    let longitud = socket.as_os_str().len();
    let (estado_socket, detalle_socket, arreglo_socket) = if longitud >= SOCKET_MAXIMO {
        (
            Estado::Error,
            format!(
                "socket {longitud} bytes (demasiado largo: el máximo es {})",
                SOCKET_MAXIMO - 1
            ),
            Some("exporta XDG_RUNTIME_DIR a una ruta más corta".to_string()),
        )
    } else if longitud >= SOCKET_AVISO {
        (
            Estado::Aviso,
            format!(
                "socket {longitud} bytes (cerca del máximo de {})",
                SOCKET_MAXIMO - 1
            ),
            Some("acorta XDG_RUNTIME_DIR para no acercarte al límite".to_string()),
        )
    } else {
        (Estado::Correcto, format!("socket {longitud} bytes"), None)
    };
    let (estado, detalle, arreglo) = combinar(
        (estado, detalle, arreglo),
        (estado_socket, detalle_socket, arreglo_socket),
    );
    Comprobacion {
        nombre: "runtime",
        estado,
        detalle,
        arreglo,
        nota: None,
    }
}

fn comprobar_servidor(datos: &Datos) -> Comprobacion {
    match &datos.servidor {
        ServidorObservado::Apagado => Comprobacion::correcto(
            "servidor",
            "no está en marcha (se arrancará solo)".to_string(),
        ),
        ServidorObservado::EnMarcha {
            version_app,
            protocolo,
            sesiones,
            transferencias,
            tuneles,
            ejecuciones,
        } => {
            let version = if version_app.is_empty() {
                format!("protocolo {protocolo}")
            } else {
                format!("{version_app} · protocolo {protocolo}")
            };
            let mut trabajo = Vec::new();
            if *sesiones > 0 {
                trabajo.push(plural(*sesiones, "sesión", "sesiones"));
            }
            if *transferencias > 0 {
                trabajo.push(plural(*transferencias, "transferencia", "transferencias"));
            }
            if *tuneles > 0 {
                trabajo.push(plural(*tuneles, "túnel", "túneles"));
            }
            if *ejecuciones > 0 {
                trabajo.push(plural(*ejecuciones, "ejecución", "ejecuciones"));
            }
            let trabajo = if trabajo.is_empty() {
                "sin trabajo".to_string()
            } else {
                trabajo.join(", ")
            };
            Comprobacion::correcto("servidor", format!("en marcha · {version} · {trabajo}"))
        }
        ServidorObservado::OtraVersion {
            version_app,
            protocolo,
        } => {
            let version = version_app
                .as_deref()
                .filter(|texto| !texto.is_empty())
                .unwrap_or("sin versión");
            if *protocolo > VERSION_PROTOCOLO {
                Comprobacion::aviso(
                    "servidor",
                    format!(
                        "hay un servidor {version} con protocolo {protocolo}, más nuevo que este MAGI {} (protocolo {VERSION_PROTOCOLO})",
                        datos.version_app
                    ),
                    Some(
                        "actualiza MAGI: este binario no puede ni debe parar ese servidor"
                            .to_string(),
                    ),
                )
            } else {
                Comprobacion::aviso(
                    "servidor",
                    format!(
                        "hay un servidor {version} con protocolo {protocolo} y este MAGI es {} (protocolo {VERSION_PROTOCOLO})",
                        datos.version_app
                    ),
                    Some("al abrir magi se sustituirá o te preguntará antes de pararlo".to_string()),
                )
            }
        }
        ServidorObservado::SinRespuesta => Comprobacion::error(
            "servidor",
            format!(
                "el socket {} existe pero el servidor no responde",
                ruta_legible(&datos.ruta_socket(), &datos.rutas.hogar)
            ),
            Some("borra el socket si no hay ningún servidor en marcha o para el proceso que lo ocupa".to_string()),
        ),
    }
}

fn comprobar_agente(datos: &Datos) -> Comprobacion {
    match &datos.agente {
        AgenteObservado::Claves(cantidad) => Comprobacion::correcto(
            "agente SSH",
            plural(*cantidad as u32, "clave cargada", "claves cargadas"),
        ),
        AgenteObservado::SinVariable => Comprobacion::aviso(
            "agente SSH",
            "sin SSH_AUTH_SOCK: no se podrá usar el agente".to_string(),
            Some(
                "arranca el agente: eval $(ssh-agent -s) y añade tus claves con ssh-add"
                    .to_string(),
            ),
        ),
        AgenteObservado::SinClaves(motivo) => {
            if !datos.disponible("ssh-add") {
                Comprobacion::aviso(
                    "agente SSH",
                    "no se encontró ssh-add".to_string(),
                    Some("instala openssh para usar el agente SSH".to_string()),
                )
            } else {
                Comprobacion::aviso(
                    "agente SSH",
                    motivo.clone(),
                    Some("añade tus claves con ssh-add".to_string()),
                )
            }
        }
    }
}

fn comprobar_llavero(datos: &Datos) -> Comprobacion {
    if cfg!(target_os = "macos") {
        if datos.disponible("security") {
            Comprobacion::correcto("llavero", "security (Keychain)".to_string())
        } else {
            Comprobacion::aviso(
                "llavero",
                "security no encontrado".to_string(),
                Some("no se podrán recordar contraseñas".to_string()),
            )
        }
    } else if datos.disponible("secret-tool") {
        Comprobacion::correcto("llavero", "secret-tool".to_string())
    } else {
        Comprobacion::aviso(
            "llavero",
            "secret-tool no encontrado".to_string(),
            Some("instala libsecret para recordar contraseñas".to_string()),
        )
    }
}

fn comprobar_portapapeles(datos: &Datos) -> Comprobacion {
    let candidatos: &[&str] = if cfg!(target_os = "macos") {
        &["pbcopy", "wl-copy"]
    } else {
        &["wl-copy", "pbcopy"]
    };
    match candidatos
        .iter()
        .find(|programa| datos.disponible(programa))
    {
        Some(programa) => Comprobacion::correcto("portapapeles", (*programa).to_string()),
        None => Comprobacion::aviso(
            "portapapeles",
            format!("{} no encontrado", candidatos[0]),
            Some("instala wl-clipboard".to_string()),
        ),
    }
}

/// Primer programa resoluble de una lista de candidatos, con la etiqueta del
/// origen. Devuelve también el primer candidato que no se encontró.
fn resolver(
    datos: &Datos,
    candidatos: &[(String, &'static str)],
) -> Result<(String, &'static str), Option<(String, &'static str)>> {
    let mut primer_fallo_explicito: Option<(String, &'static str)> = None;
    for (texto, origen) in candidatos {
        let Some((programa, _)) = crate::visor::comando(texto) else {
            continue;
        };
        if datos.disponible(&programa) {
            return Ok((programa, origen));
        }
        // Un candidato configurado o del entorno que no existe se avisa; los
        // de por defecto se saltan para probar el siguiente (nvim → vi,
        // less → more): con que haya uno sirve.
        if *origen != "por defecto" && primer_fallo_explicito.is_none() {
            primer_fallo_explicito = Some((programa, origen));
        }
    }
    Err(primer_fallo_explicito)
}

fn candidatos_de(
    configurado: &str,
    variable: Option<&String>,
    origen: &'static str,
    variable_origen: &'static str,
) -> Vec<(String, &'static str)> {
    let mut candidatos = Vec::new();
    let configurado = configurado.trim();
    if !configurado.is_empty() {
        candidatos.push((configurado.to_string(), origen));
    }
    if let Some(valor) = variable
        .map(|valor| valor.trim())
        .filter(|valor| !valor.is_empty())
    {
        candidatos.push((valor.to_string(), variable_origen));
    }
    candidatos
}

fn comprobar_editor(datos: &Datos, config: &Config) -> Comprobacion {
    let variable = datos
        .variables
        .get("VISUAL")
        .or_else(|| datos.variables.get("EDITOR"));
    let mut candidatos = candidatos_de(
        &config.archivos.editor,
        variable,
        "[archivos] editor",
        "$VISUAL o $EDITOR",
    );
    candidatos.push(("nvim".to_string(), "por defecto"));
    candidatos.push(("vi".to_string(), "por defecto"));
    match resolver(datos, &candidatos) {
        Ok((programa, origen)) => {
            Comprobacion::correcto("editor", format!("{programa} ({origen})"))
        }
        Err(motivo) => {
            let (detalle, arreglo) = match motivo {
                Some((programa, origen)) => (
                    format!("{programa} ({origen}) no encontrado"),
                    format!("instala {programa} o cambia [archivos] editor"),
                ),
                None => (
                    "no se encontró ningún editor".to_string(),
                    "instala nvim o define $EDITOR".to_string(),
                ),
            };
            Comprobacion::aviso("editor", detalle, Some(arreglo))
        }
    }
}

fn comprobar_paginador(datos: &Datos, config: &Config) -> Comprobacion {
    let mut candidatos = candidatos_de(
        &config.archivos.pager,
        datos.variables.get("PAGER"),
        "[archivos] pager",
        "$PAGER",
    );
    candidatos.push(("less".to_string(), "por defecto"));
    candidatos.push(("more".to_string(), "por defecto"));
    match resolver(datos, &candidatos) {
        Ok((programa, origen)) => {
            Comprobacion::correcto("paginador", format!("{programa} ({origen})"))
        }
        Err(motivo) => {
            let (detalle, arreglo) = match motivo {
                Some((programa, origen)) => (
                    format!("{programa} ({origen}) no encontrado"),
                    format!("instala {programa} o cambia el paginador"),
                ),
                None => (
                    "no se encontró ningún paginador".to_string(),
                    "instala less".to_string(),
                ),
            };
            Comprobacion::aviso("paginador", detalle, Some(arreglo))
        }
    }
}

fn comprobar_terminal(datos: &Datos, config: &Config) -> Comprobacion {
    let comando = config.terminal.comando.trim();
    let Some((programa, _)) = crate::visor::comando(comando) else {
        return Comprobacion::aviso(
            "terminal",
            "sin comando de terminal para abrir una ventana nueva".to_string(),
            Some("define [terminal] comando en config.toml".to_string()),
        );
    };
    if datos.disponible(&programa) {
        Comprobacion::correcto("terminal", format!("{programa} ([terminal] comando)"))
    } else {
        Comprobacion::aviso(
            "terminal",
            format!("{programa} no encontrado ([terminal] comando)"),
            Some(format!("instala {programa} o cambia [terminal] comando")),
        )
    }
}

fn comprobar_ssh_config(datos: &Datos) -> Comprobacion {
    if datos.incluye_magi_config {
        return Comprobacion::correcto(
            "ssh_config",
            format!(
                "{} incluye magi_config",
                ruta_legible(&datos.rutas.fichero_ssh_config(), &datos.rutas.hogar)
            ),
        );
    }
    Comprobacion::aviso(
        "ssh_config",
        "sin «Include magi_config»".to_string(),
        Some("ssh fuera de MAGI no verá tus hosts".to_string()),
    )
    .con_nota(Some(
        "(MAGI lo añade al exportar desde la ficha o con `magi exportar`)".to_string(),
    ))
}

fn comprobar_sincronizacion(datos: &Datos) -> Comprobacion {
    let SincronizacionObservada::Configurada {
        carpeta,
        equipos,
        conflictos_pendientes,
        copias_conflicto,
    } = &datos.sincronizacion
    else {
        return Comprobacion::correcto(
            "sincronización",
            "desactivada (sin carpeta en config.toml)".to_string(),
        );
    };
    // La carpeta compartida solo tiene que existir y poder escribirse: es una
    // carpeta del usuario (Syncthing, Nextcloud…), no un directorio privado de
    // MAGI, así que no se avisa de permisos más abiertos.
    let ruta = ruta_legible(&carpeta.ruta, &datos.rutas.hogar);
    let (mut estado, mut detalle, mut arreglo): (Estado, String, Option<String>) = if !carpeta
        .existe
    {
        (
            Estado::Error,
            format!("carpeta {ruta} no existe"),
            Some("crea la carpeta o cambia [sincronizacion] carpeta en config.toml".to_string()),
        )
    } else if !carpeta.es_directorio {
        (
            Estado::Error,
            format!("carpeta {ruta} no es un directorio"),
            Some("cambia [sincronizacion] carpeta en config.toml".to_string()),
        )
    } else if !carpeta.escribible {
        (
            Estado::Error,
            format!("carpeta {ruta}: no se puede escribir"),
            Some(format!("revisa los permisos de {ruta}")),
        )
    } else {
        (
            Estado::Correcto,
            format!("carpeta {ruta} (escribible)"),
            None,
        )
    };
    let (estado_equipos, detalle_equipos, arreglo_equipos) = describir_equipos(datos, equipos);
    if let Some(pendientes) = conflictos_pendientes {
        if *pendientes > 0 {
            let plural = if *pendientes == 1 {
                "conflicto pendiente"
            } else {
                "conflictos pendientes"
            };
            (estado, detalle, arreglo) = combinar(
                (estado, detalle, arreglo),
                (
                    Estado::Aviso,
                    format!("{pendientes} {plural}"),
                    Some("resuélvelos desde la vista CONFLICTOS (tecla c)".to_string()),
                ),
            );
        }
    }
    if *copias_conflicto > 0 {
        let plural = if *copias_conflicto == 1 {
            "copia en conflicto"
        } else {
            "copias en conflicto"
        };
        (estado, detalle, arreglo) = combinar(
            (estado, detalle, arreglo),
            (
                Estado::Aviso,
                format!("{copias_conflicto} {plural} en la carpeta"),
                Some("resuélvelas en la herramienta de sincronización".to_string()),
            ),
        );
    }
    // Los equipos ilegibles se combinan al final: su instrucción (introducir
    // la clave nueva) es la más importante y es la que se queda.
    (estado, detalle, arreglo) = combinar(
        (estado, detalle, arreglo),
        (estado_equipos, detalle_equipos, arreglo_equipos),
    );
    Comprobacion {
        nombre: "sincronización",
        estado,
        detalle,
        arreglo,
        nota: None,
    }
}

/// ¿Qué se puede decir de los equipos de la sincronización?
fn describir_equipos(
    datos: &Datos,
    equipos: &ResultadoEquipos,
) -> (Estado, String, Option<String>) {
    match equipos {
        ResultadoEquipos::SinDatos => match &datos.base_datos {
            ResultadoBaseDatos::Version(version) if *version < VERSION_BD => (
                Estado::Correcto,
                format!("magi.db versión {version} (se migrará al abrir MAGI)"),
                None,
            ),
            ResultadoBaseDatos::NoExiste => (
                Estado::Correcto,
                "sin equipos todavía (magi.db no existe)".to_string(),
                None,
            ),
            _ => (
                Estado::Correcto,
                "sin equipos que mostrar".to_string(),
                None,
            ),
        },
        ResultadoEquipos::NoLegible(motivo) => (
            Estado::Aviso,
            format!("no se pudieron leer los equipos: {motivo}"),
            Some("mira la comprobación «datos»".to_string()),
        ),
        ResultadoEquipos::Equipos(equipos) => {
            let total = equipos.len();
            let mut resumen = format!("{total} {}", if total == 1 { "equipo" } else { "equipos" });
            let mut problemas: Vec<String> = Vec::new();
            let mut hay_ilegible = false;
            for equipo in equipos {
                match equipo.estado.as_str() {
                    crate::almacen::equipos::ESTADO_ILEGIBLE => {
                        hay_ilegible = true;
                        problemas.push(format!("«{}» ilegible", equipo.nombre));
                    }
                    crate::almacen::equipos::ESTADO_DANADO => {
                        problemas.push(format!("«{}» dañado", equipo.nombre));
                    }
                    crate::almacen::equipos::ESTADO_FORMATO_NUEVO => {
                        problemas.push(format!("«{}» pide actualizar MAGI", equipo.nombre));
                    }
                    _ => {}
                }
            }
            if problemas.is_empty() {
                return (Estado::Correcto, resumen, None);
            }
            resumen.push_str(" · ");
            resumen.push_str(&problemas.join(" · "));
            let arreglo = if hay_ilegible {
                "introduce la clave nueva desde la vista SINCRONIZACIÓN".to_string()
            } else {
                "revisa la sincronización desde la vista SINCRONIZACIÓN".to_string()
            };
            (Estado::Aviso, resumen, Some(arreglo))
        }
    }
}

fn plural(cantidad: u32, singular: &str, plural: &str) -> String {
    if cantidad == 1 {
        format!("{cantidad} {singular}")
    } else {
        format!("{cantidad} {plural}")
    }
}

// ------------------------------------------------------------------ salida

/// Código ANSI de un color RGB de la paleta de MAGI.
fn ansi(color: ratatui::style::Color) -> String {
    match color {
        ratatui::style::Color::Rgb(rojo, verde, azul) => {
            format!("\x1b[38;2;{rojo};{verde};{azul}m")
        }
        _ => String::new(),
    }
}

/// Imprime el informe y devuelve el código de salida (0 sin errores).
pub fn ejecutar_en(datos: &Datos, es_terminal: bool, escritor: &mut dyn Write) -> i32 {
    let informe = comprobar(datos);
    if let Err(error) = imprimir(&informe, datos, opciones(datos, es_terminal), escritor) {
        eprintln!("no se pudo escribir el diagnóstico: {error}");
        return 1;
    }
    informe.codigo_salida()
}

/// Escribe el informe en el escritor indicado.
pub fn imprimir(
    informe: &Informe,
    datos: &Datos,
    opciones: Opciones,
    escritor: &mut dyn Write,
) -> std::io::Result<()> {
    let cabecera = format!(
        "MAGI {} · protocolo {} · commit {}",
        datos.version_app, datos.protocolo, datos.commit
    );
    writeln!(escritor, "{}", adaptar(&cabecera, opciones.ascii))?;
    writeln!(escritor)?;
    let verde = ansi(tema::CORRECTO);
    let ambar = ansi(tema::ACENTO);
    let rojo = ansi(tema::CRITICO);
    for comprobacion in &informe.comprobaciones {
        let (signo, color) = match comprobacion.estado {
            Estado::Correcto => ("✓", &verde),
            Estado::Aviso => ("!", &ambar),
            Estado::Error => ("✕", &rojo),
        };
        let signo = if opciones.ascii {
            match comprobacion.estado {
                Estado::Correcto => "ok".to_string(),
                Estado::Aviso => "!".to_string(),
                Estado::Error => "x".to_string(),
            }
        } else if opciones.color && !color.is_empty() {
            format!("{color}{signo}\x1b[0m")
        } else {
            signo.to_string()
        };
        let detalle = adaptar(&comprobacion.detalle, opciones.ascii);
        let mut linea = format!(
            "  {signo} {:<ancho$} {detalle}",
            comprobacion.nombre,
            ancho = ANCHO_NOMBRE
        );
        if let Some(arreglo) = &comprobacion.arreglo {
            let flecha = if opciones.ascii { " -> " } else { " → " };
            linea.push_str(flecha);
            linea.push_str(&adaptar(arreglo, opciones.ascii));
        }
        writeln!(escritor, "{linea}")?;
        if let Some(nota) = &comprobacion.nota {
            writeln!(
                escritor,
                "{:ancho$}{}",
                "",
                adaptar(nota, opciones.ascii),
                ancho = ANCHO_NOMBRE + 5
            )?;
        }
    }
    writeln!(escritor)?;
    writeln!(escritor, "{}", informe.resumen())?;
    escritor.flush()
}

/// Adapta un texto al modo ASCII degradado (los signos se sustituyen aparte).
fn adaptar(texto: &str, ascii: bool) -> String {
    if ascii {
        crate::ui::disposicion::texto_ascii(texto)
    } else {
        texto.to_string()
    }
}

/// `magi diagnostico`: recoge, imprime en la salida estándar y devuelve el
/// código de salida. No toca rutas ni log: se despacha antes que ellos en
/// `main`.
pub fn ejecutar() -> anyhow::Result<i32> {
    let rutas = Rutas::descubrir()?;
    let datos = recoger(rutas);
    let salida = std::io::stdout();
    let es_terminal = salida.is_terminal();
    let mut salida = salida.lock();
    Ok(ejecutar_en(&datos, es_terminal, &mut salida))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// Con un WAL pendiente, comprobar la base de datos no deja laterales
    /// nuevos junto al original: se lee una copia temporal.
    #[test]
    fn comprobar_la_base_de_datos_en_wal_no_crea_ficheros() {
        let directorio = tempfile::tempdir().unwrap();
        let origen = directorio.path().join("origen.db");
        let ruta = directorio.path().join("magi.db");
        {
            let conexion = Connection::open(&origen).unwrap();
            conexion.pragma_update(None, "journal_mode", "WAL").unwrap();
            conexion
                .pragma_update(None, "wal_autocheckpoint", 0)
                .unwrap();
            conexion
                .execute_batch(
                    "CREATE TABLE MARCA (valor TEXT); INSERT INTO MARCA VALUES ('x'); PRAGMA user_version = 6;",
                )
                .unwrap();
            std::fs::copy(&origen, &ruta).unwrap();
            let mut wal_origen = origen.as_os_str().to_os_string();
            wal_origen.push("-wal");
            let mut wal = ruta.as_os_str().to_os_string();
            wal.push("-wal");
            std::fs::copy(&wal_origen, &wal).unwrap();
        }
        let antes: Vec<_> = std::fs::read_dir(directorio.path())
            .unwrap()
            .map(|entrada| entrada.unwrap().file_name())
            .collect();
        let resultado = observar_base_datos(&ruta);
        assert_eq!(resultado, ResultadoBaseDatos::Version(6));
        let despues: Vec<_> = std::fs::read_dir(directorio.path())
            .unwrap()
            .map(|entrada| entrada.unwrap().file_name())
            .collect();
        assert_eq!(antes, despues, "el diagnóstico creó ficheros laterales");
    }

    /// El plazo de ejecutar_con_plazo mata al hijo colgado y no espera.
    #[test]
    fn ejecutar_con_plazo_mata_al_hijo_colgado() {
        let inicio = Instant::now();
        let salida = ejecutar_con_plazo("sleep", &["5"], Duration::from_millis(150));
        assert!(salida.is_none(), "debió vencer el plazo");
        assert!(
            inicio.elapsed() < Duration::from_secs(2),
            "tardó {:?}",
            inicio.elapsed()
        );
    }
}
