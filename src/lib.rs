pub mod almacen;
pub mod app;
pub mod archivos;
pub mod cliente;
pub mod conexion;
pub mod config;
pub mod deliberacion;
pub mod diagnostico;
pub mod ficheros;
pub mod flota;
pub mod identidades;
pub mod llavero;
pub mod modelo;
pub mod portapapeles;
pub mod protocolo;
pub mod registro;
pub mod servidor;
pub mod sincronizacion;
pub mod snippets;
pub mod sshconfig;
pub mod teclas;
pub mod tema;
pub mod ui;
pub mod visor;
// ------------------------------------------------------- versión y metadatos

/// Versión de la aplicación: la del paquete, que desde la Fase 10 es 1.1.0.
pub const VERSION_APP: &str = env!("CARGO_PKG_VERSION");

/// Commit corto con el que se compiló el binario. Lo saca `build.rs` del
/// repositorio git; compilado desde un tarball de fuentes vale «desconocido».
pub const COMMIT: &str = match option_env!("MAGI_COMMIT") {
    Some(commit) => commit,
    None => "desconocido",
};

/// Fecha de la versión 1.1.0 (Fase 10). Fija la cabecera de la página man: la
/// generación de recursos no puede cambiar en cada compilación.
pub const FECHA_DE_VERSION: &str = "2026-10-31";

/// Versión larga como la imprime `magi --version`: versión, protocolo y commit.
/// Se calcula una sola vez y se devuelve como cadena estática porque clap la
/// exige así en sus atributos.
pub fn version_larga() -> &'static str {
    static VERSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    VERSION
        .get_or_init(|| {
            format!(
                "{VERSION_APP} (protocolo {}, {COMMIT})",
                crate::protocolo::VERSION_PROTOCOLO
            )
        })
        .as_str()
}
