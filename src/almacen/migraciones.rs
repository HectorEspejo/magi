use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result};
use rusqlite::Connection;

/// Migraciones numeradas y aplicadas por `PRAGMA user_version`. Nunca se
/// modifica una migración ya publicada: se añade otra al final.
pub const MIGRACIONES: &[&str] = &[
    MIGRACION_1_INICIAL,
    MIGRACION_2_FLOTA,
    MIGRACION_3_ARCHIVOS,
    MIGRACION_4_TUNELES,
    MIGRACION_5_SNIPPETS,
    MIGRACION_6_SINCRONIZACIONES,
];

/// Versión de esquema que conoce este binario (el número de migraciones).
pub const VERSION_BD: i64 = MIGRACIONES.len() as i64;

/// Rechaza una base de datos creada por una versión más nueva de MAGI: las
/// migraciones solo van hacia delante y el fichero no se toca. Se llama antes
/// de activar WAL o ajustar permisos, para no modificarlo siquiera.
pub fn comprobar_version(conexion: &Connection) -> Result<()> {
    let version: i64 = conexion.query_row("PRAGMA user_version", [], |fila| fila.get(0))?;
    if version > VERSION_BD {
        anyhow::bail!("esta base de datos es de una versión más nueva de MAGI; actualiza MAGI");
    }
    Ok(())
}

/// Ruta del WAL que acompaña a una base de datos SQLite (`magi.db` →
/// `magi.db-wal`).
fn ruta_wal(ruta: &Path) -> PathBuf {
    let mut texto = ruta.as_os_str().to_os_string();
    texto.push("-wal");
    PathBuf::from(texto)
}

/// ¿Hay un WAL pendiente de recuperar? Un WAL vacío no cuenta.
fn wal_pendiente(ruta: &Path) -> bool {
    std::fs::metadata(ruta_wal(ruta))
        .map(|metadatos| metadatos.is_file() && metadatos.len() > 0)
        .unwrap_or(false)
}

/// Versión leída de la cabecera SQLite (campo `user_version`, bytes 60..64).
/// Solo es fiable cuando no hay WAL pendiente: hasta el checkpoint, la
/// cabecera principal puede ser antigua. Un fichero corto (base de datos
/// nueva o incompleta) cuenta como versión 0 para que la abra el flujo normal.
fn version_de_cabecera(ruta: &Path) -> Result<i64> {
    let mut fichero = std::fs::File::open(ruta)
        .with_context(|| format!("abriendo la cabecera de {}", ruta.display()))?;
    let mut cabecera = [0u8; 100];
    let leidos = fichero
        .read(&mut cabecera)
        .with_context(|| format!("leyendo la cabecera de {}", ruta.display()))?;
    if leidos < 64 {
        return Ok(0);
    }
    let version = i32::from_be_bytes([cabecera[60], cabecera[61], cabecera[62], cabecera[63]]);
    Ok(i64::from(version))
}

/// Versión real de una base de datos con WAL pendiente: se copia el par
/// db+wal a un directorio temporal y se deja que SQLite recupere y lea la
/// copia, sin tocar el original (abrir el original en lectura/escritura
/// aplicaría el WAL y modificaría el fichero antes de poder rechazarlo).
fn version_de_una_copia(ruta: &Path) -> Result<i64> {
    let marca = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duracion| duracion.as_nanos())
        .unwrap_or(0);
    let temporal =
        std::env::temp_dir().join(format!("magi-version-{}-{marca}", std::process::id()));
    std::fs::create_dir(&temporal)
        .with_context(|| format!("creando el directorio temporal {}", temporal.display()))?;
    let resultado = (|| -> Result<i64> {
        let copia = temporal.join("magi.db");
        std::fs::copy(ruta, &copia).with_context(|| format!("copiando {}", ruta.display()))?;
        std::fs::copy(ruta_wal(ruta), ruta_wal(&copia))
            .with_context(|| "copiando el WAL de la base de datos")?;
        let conexion = Connection::open(&copia)
            .with_context(|| format!("abriendo la copia temporal {}", copia.display()))?;
        let version: i64 = conexion.query_row("PRAGMA user_version", [], |fila| fila.get(0))?;
        Ok(version)
    })();
    let _ = std::fs::remove_dir_all(&temporal);
    resultado
}

/// Comprueba la versión de una base de datos **sin abrirla en lectura/
/// escritura**: sin WAL pendiente basta la cabecera; con WAL pendiente se
/// comprueba una copia temporal. Es la primera comprobación de `Almacen::abrir`,
/// antes de permisos, WAL o migraciones.
pub fn comprobar_version_en_disco(ruta: &Path) -> Result<()> {
    if !ruta.exists() {
        return Ok(());
    }
    let version = if wal_pendiente(ruta) {
        version_de_una_copia(ruta)?
    } else {
        version_de_cabecera(ruta)?
    };
    if version > VERSION_BD {
        anyhow::bail!("esta base de datos es de una versión más nueva de MAGI; actualiza MAGI");
    }
    Ok(())
}

const MIGRACION_1_INICIAL: &str = r#"
CREATE TABLE GRUPOS (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    nombre    TEXT    NOT NULL UNIQUE,
    orden     INTEGER NOT NULL DEFAULT 0,
    plegado   INTEGER NOT NULL DEFAULT 0,
    creado_en TEXT    NOT NULL
);

CREATE TABLE HOSTS (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    nombre             TEXT    NOT NULL UNIQUE,
    grupo_id           INTEGER REFERENCES GRUPOS(id) ON DELETE SET NULL,
    direccion          TEXT    NOT NULL,
    puerto             INTEGER NOT NULL DEFAULT 22,
    usuario            TEXT,
    identidad_ref      TEXT,
    salto_host_id      INTEGER REFERENCES HOSTS(id) ON DELETE SET NULL,
    multiplexar        INTEGER NOT NULL DEFAULT 0,
    keepalive_seg      INTEGER,
    opciones_extra     TEXT    NOT NULL DEFAULT '',
    origen             TEXT    NOT NULL DEFAULT 'manual',
    ultimo_estado      TEXT,
    ultima_conexion_en TEXT,
    creado_en          TEXT    NOT NULL,
    actualizado_en     TEXT    NOT NULL
);

CREATE TABLE ETIQUETAS (
    id     INTEGER PRIMARY KEY AUTOINCREMENT,
    nombre TEXT    NOT NULL UNIQUE
);

CREATE TABLE HOST_ETIQUETAS (
    host_id     INTEGER NOT NULL REFERENCES HOSTS(id) ON DELETE CASCADE,
    etiqueta_id INTEGER NOT NULL REFERENCES ETIQUETAS(id) ON DELETE CASCADE,
    PRIMARY KEY (host_id, etiqueta_id)
);

CREATE INDEX idx_hosts_grupo ON HOSTS(grupo_id);
CREATE INDEX idx_hosts_salto ON HOSTS(salto_host_id);
CREATE INDEX idx_host_etiquetas_etiqueta ON HOST_ETIQUETAS(etiqueta_id);
"#;

const MIGRACION_2_FLOTA: &str = r#"
ALTER TABLE HOSTS ADD COLUMN servicios TEXT NOT NULL DEFAULT '';

CREATE TABLE SONDEOS (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    host_id           INTEGER NOT NULL REFERENCES HOSTS(id) ON DELETE CASCADE,
    fecha             TEXT    NOT NULL,
    resultado         TEXT    NOT NULL,
    error             TEXT,
    nucleos           INTEGER,
    carga_1m          REAL,
    carga_5m          REAL,
    carga_15m         REAL,
    mem_total_kb      INTEGER,
    mem_disponible_kb INTEGER,
    disco_total_kb    INTEGER,
    disco_usado_kb    INTEGER,
    red_rx_bytes      INTEGER,
    red_tx_bytes      INTEGER,
    uptime_seg        INTEGER,
    servicios_json    TEXT,
    duracion_ms       INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IDENTIDADES (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    alias         TEXT    NOT NULL UNIQUE,
    tipo          TEXT    NOT NULL,
    huella        TEXT    NOT NULL UNIQUE,
    origen        TEXT    NOT NULL,
    ruta          TEXT,
    comentario    TEXT,
    anadida_en    TEXT    NOT NULL,
    ultimo_uso_en TEXT,
    revocada_en   TEXT
);

CREATE TABLE REGISTRO (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    fecha        TEXT    NOT NULL,
    tipo         TEXT    NOT NULL,
    host_id      INTEGER REFERENCES HOSTS(id) ON DELETE SET NULL,
    identidad_id INTEGER REFERENCES IDENTIDADES(id) ON DELETE SET NULL,
    detalle      TEXT    NOT NULL DEFAULT '',
    resultado    TEXT    NOT NULL
);

CREATE INDEX idx_sondeos_host ON SONDEOS(host_id, id DESC);
CREATE INDEX idx_registro_fecha ON REGISTRO(fecha DESC);
CREATE INDEX idx_registro_tipo ON REGISTRO(tipo);
"#;

/// Fase 4: últimos directorios de la vista Archivos, por host. Nulos mientras
/// no se haya navegado con ese host (el remoto nulo es el directorio de inicio
/// del usuario remoto).
const MIGRACION_3_ARCHIVOS: &str = r#"
ALTER TABLE HOSTS ADD COLUMN sftp_dir_local TEXT;
ALTER TABLE HOSTS ADD COLUMN sftp_dir_remoto TEXT;
"#;

/// Fase 5: túneles SSH. El nombre es único por host; `tipo` se valida en
/// código, no con `CHECK`. El estado en vivo no se persiste: vive en el
/// servidor de sesiones.
const MIGRACION_4_TUNELES: &str = r#"
CREATE TABLE TUNELES (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    host_id        INTEGER NOT NULL REFERENCES HOSTS(id) ON DELETE CASCADE,
    nombre         TEXT    NOT NULL,
    tipo           TEXT    NOT NULL,
    escucha        TEXT    NOT NULL,
    destino        TEXT,
    automatico     INTEGER NOT NULL DEFAULT 0,
    creado_en      TEXT    NOT NULL,
    actualizado_en TEXT    NOT NULL,
    UNIQUE(host_id, nombre)
);

CREATE INDEX idx_tuneles_host ON TUNELES(host_id);
"#;

/// Fase 6: snippets, verificaciones previas por host y deliberaciones MAGI.
/// Los destinos van por nombre de etiqueta (un snippet sobrevive a que la
/// etiqueta se quede sin hosts) o por host suelto (en cascada). Exactamente
/// uno de los dos se valida en código; los índices únicos parciales solo
/// impiden repetir un destino. Sin `CHECK` sobre enumeraciones.
const MIGRACION_5_SNIPPETS: &str = r#"
CREATE TABLE SNIPPETS (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    nombre         TEXT    NOT NULL UNIQUE,
    comando        TEXT    NOT NULL,
    descripcion    TEXT    NOT NULL DEFAULT '',
    etiquetas      TEXT    NOT NULL DEFAULT '',
    critico        INTEGER NOT NULL DEFAULT 0,
    timeout_seg    INTEGER NOT NULL DEFAULT 60,
    parar_al_fallo INTEGER NOT NULL DEFAULT 0,
    usado_veces    INTEGER NOT NULL DEFAULT 0,
    ultimo_uso_en  TEXT,
    creado_en      TEXT    NOT NULL,
    actualizado_en TEXT    NOT NULL
);

CREATE TABLE SNIPPET_DESTINOS (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    snippet_id INTEGER NOT NULL REFERENCES SNIPPETS(id) ON DELETE CASCADE,
    etiqueta   TEXT,
    host_id    INTEGER REFERENCES HOSTS(id) ON DELETE CASCADE
);

CREATE INDEX idx_snippet_destinos_snippet ON SNIPPET_DESTINOS(snippet_id);
CREATE INDEX idx_snippet_destinos_host ON SNIPPET_DESTINOS(host_id);
CREATE UNIQUE INDEX idx_snippet_destinos_etiqueta
    ON SNIPPET_DESTINOS(snippet_id, etiqueta) WHERE etiqueta IS NOT NULL;
CREATE UNIQUE INDEX idx_snippet_destinos_host_unico
    ON SNIPPET_DESTINOS(snippet_id, host_id) WHERE host_id IS NOT NULL;

CREATE TABLE VERIFICACIONES_HOST (
    host_id        INTEGER PRIMARY KEY REFERENCES HOSTS(id) ON DELETE CASCADE,
    salud          INTEGER NOT NULL DEFAULT 0,
    backup         INTEGER NOT NULL DEFAULT 0,
    backup_ruta    TEXT,
    backup_patron  TEXT,
    tests          INTEGER NOT NULL DEFAULT 0,
    tests_comando  TEXT,
    actualizado_en TEXT    NOT NULL
);

CREATE TABLE DELIBERACIONES (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    fecha               TEXT    NOT NULL,
    snippet_id          INTEGER REFERENCES SNIPPETS(id) ON DELETE SET NULL,
    accion              TEXT    NOT NULL,
    hosts_json          TEXT    NOT NULL,
    comprobaciones_json TEXT    NOT NULL,
    resultado           TEXT    NOT NULL,
    bloqueada           INTEGER NOT NULL DEFAULT 0,
    motivo              TEXT,
    usuario             TEXT    NOT NULL,
    ejecucion_resultado TEXT
);

CREATE INDEX idx_deliberaciones_fecha ON DELIBERACIONES(fecha);

ALTER TABLE HOSTS ADD COLUMN snippet_al_conectar_id INTEGER
    REFERENCES SNIPPETS(id) ON DELETE SET NULL;
"#;

/// Fase 8: sincronizaciones de directorio guardadas por host. El nombre es
/// único por host; `direccion` (`subida` | `bajada`) y `ultimo_resultado`
/// (`ok` | `parcial` | `error` | `cancelada`) se validan en código, no con
/// `CHECK`. `exclusiones` son patrones de gitignore, uno por línea.
/// `ultima_ejecucion_en` la escribe el cliente al lanzar y `ultimo_resultado`
/// el servidor al terminar.
const MIGRACION_6_SINCRONIZACIONES: &str = r#"
CREATE TABLE SINCRONIZACIONES_DIR (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    host_id             INTEGER NOT NULL REFERENCES HOSTS(id) ON DELETE CASCADE,
    nombre              TEXT    NOT NULL,
    ruta_local          TEXT    NOT NULL,
    ruta_remota         TEXT    NOT NULL,
    direccion           TEXT    NOT NULL,
    borrar              INTEGER NOT NULL DEFAULT 0,
    exclusiones         TEXT    NOT NULL DEFAULT '',
    ultima_ejecucion_en TEXT,
    ultimo_resultado    TEXT,
    creado_en           TEXT    NOT NULL,
    actualizado_en      TEXT    NOT NULL,
    UNIQUE(host_id, nombre)
);

CREATE INDEX idx_sincronizaciones_dir_host ON SINCRONIZACIONES_DIR(host_id);
"#;

pub fn aplicar(conexion: &mut Connection) -> Result<()> {
    comprobar_version(conexion)?;
    let version: i64 = conexion.query_row("PRAGMA user_version", [], |fila| fila.get(0))?;
    for (indice, sql) in MIGRACIONES.iter().enumerate() {
        let numero = indice as i64 + 1;
        if version >= numero {
            continue;
        }
        let transaccion = conexion.transaction()?;
        transaccion.execute_batch(sql)?;
        transaccion.pragma_update(None, "user_version", numero)?;
        transaccion.commit()?;
    }
    Ok(())
}
