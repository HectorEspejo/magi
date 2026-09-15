use anyhow::Result;
use rusqlite::Connection;

/// Migraciones numeradas y aplicadas por `PRAGMA user_version`. Nunca se
/// modifica una migración ya publicada: se añade otra al final.
pub const MIGRACIONES: &[&str] = &[MIGRACION_1_INICIAL];

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
