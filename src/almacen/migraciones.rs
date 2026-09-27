use anyhow::Result;
use rusqlite::Connection;

/// Migraciones numeradas y aplicadas por `PRAGMA user_version`. Nunca se
/// modifica una migración ya publicada: se añade otra al final.
pub const MIGRACIONES: &[&str] = &[
    MIGRACION_1_INICIAL,
    MIGRACION_2_FLOTA,
    MIGRACION_3_ARCHIVOS,
    MIGRACION_4_TUNELES,
    MIGRACION_5_SNIPPETS,
];

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

pub fn aplicar(conexion: &mut Connection) -> Result<()> {
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
