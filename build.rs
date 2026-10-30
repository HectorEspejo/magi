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
    let commit = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .ok()
        .filter(|salida| salida.status.success())
        .and_then(|salida| String::from_utf8(salida.stdout).ok())
        .map(|texto| texto.trim().to_string())
        .filter(|texto| !texto.is_empty())
        .unwrap_or_else(|| "desconocido".to_string());
    println!("cargo:rustc-env=MAGI_COMMIT={commit}");
}
