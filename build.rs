//! Metadatos de compilación para `magi --version`: el commit corto de git.
//!
//! Si se compila sin repositorio git (tarball de fuentes), el commit queda
//! como «desconocido» y `--version` lo dice así.

use std::process::Command;

fn main() {
    // El commit solo cambia con el repositorio; que no recompile por gusto.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");
    println!("cargo:rerun-if-changed=.git/refs/tags");
    // Solo vale el commit si este crate ES la raíz del repositorio: una copia
    // de las fuentes (tarball) extraída dentro de otro repositorio no debe
    // heredar el commit del padre.
    let commit = if es_la_raiz_del_repositorio() {
        Command::new("git")
            .args(["rev-parse", "--short=7", "HEAD"])
            .output()
            .ok()
            .filter(|salida| salida.status.success())
            .and_then(|salida| String::from_utf8(salida.stdout).ok())
            .map(|texto| texto.trim().to_string())
            .filter(|texto| !texto.is_empty())
            .unwrap_or_else(|| "desconocido".to_string())
    } else {
        "desconocido".to_string()
    };
    println!("cargo:rustc-env=MAGI_COMMIT={commit}");
}

/// ¿El directorio del manifiesto es la raíz del repositorio git que lo
/// contiene? Si el árbol de fuentes está dentro de otro repositorio (o no hay
/// git), el commit es «desconocido».
fn es_la_raiz_del_repositorio() -> bool {
    let Ok(manifiesto) = std::env::var("CARGO_MANIFEST_DIR") else {
        return false;
    };
    let Ok(manifiesto) = std::fs::canonicalize(manifiesto) else {
        return false;
    };
    let Some(raiz) = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|salida| salida.status.success())
        .and_then(|salida| String::from_utf8(salida.stdout).ok())
    else {
        return false;
    };
    std::fs::canonicalize(raiz.trim())
        .map(|raiz| raiz == manifiesto)
        .unwrap_or(false)
}
