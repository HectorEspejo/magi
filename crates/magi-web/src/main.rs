//! `magi-web`: construir, comprobar y preparar la web de MAGI.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use magi_web::comprobar::{comprobar_sitio, ListaPermitida};
use magi_web::construir::{construir, OpcionesConstruir};
use magi_web::datos::Plataformas;
use magi_web::error::{ErrorWeb, Resultado};
use magi_web::releases::{descargar_releases, FuenteGh};

#[derive(Parser)]
#[command(
    name = "magi-web",
    version,
    about = "Genera y comprueba la web de MAGI"
)]
struct Cli {
    #[command(subcommand)]
    orden: Orden,
}

#[derive(Subcommand)]
enum Orden {
    /// Genera portada, descargas, versiones y 404 en los dos idiomas, copia
    /// capturas, fuentes y estilos, y pone cabecera, pie y CSP a todo (también al
    /// manual ya generado).
    Construir {
        /// Carpeta `web/`.
        #[arg(long, default_value = "web")]
        web: PathBuf,
        /// `releases.json` (lo escribe `magi-web releases`).
        #[arg(long)]
        releases: PathBuf,
        /// Carpeta del manual ya generado por mdBook.
        #[arg(long)]
        manual: Option<PathBuf>,
        /// Animación de la portada (`assets/demo/magi.gif`).
        #[arg(long)]
        animacion: Option<PathBuf>,
        /// Carpeta de salida (se sustituye entera).
        #[arg(long)]
        salida: PathBuf,
        /// Mostrar el bloque de Homebrew (`PUBLICAR_HOMEBREW=true`).
        #[arg(long)]
        homebrew: bool,
        /// Mostrar el bloque de AUR (`PUBLICAR_AUR=true`).
        #[arg(long)]
        aur: bool,
    },
    /// Revisa un sitio generado: enlaces internos y anclas, externos permitidos,
    /// sin `style` ni scripts propios y CSP en cada página.
    Comprobar {
        /// Carpeta del sitio.
        dir: PathBuf,
    },
    /// Lee la API de GitHub y los `.sha256` publicados, los valida y escribe `releases.json`.
    Releases {
        /// Fichero de salida.
        #[arg(long)]
        salida: PathBuf,
        /// `plataformas.toml`.
        #[arg(long, default_value = "web/datos/plataformas.toml")]
        plataformas: PathBuf,
    },
}

fn ejecutar(cli: Cli) -> Resultado<()> {
    match cli.orden {
        Orden::Construir {
            web,
            releases,
            manual,
            animacion,
            salida,
            homebrew,
            aur,
        } => {
            let informe = construir(&OpcionesConstruir {
                web,
                releases,
                manual,
                animacion,
                salida,
                homebrew,
                aur,
            })?;
            println!(
                "sitio generado: {} páginas (escritorio {}{}{})",
                informe.paginas.len(),
                informe.version_escritorio,
                informe
                    .version_termux
                    .as_ref()
                    .map(|v| format!(", Termux {v}"))
                    .unwrap_or_default(),
                informe
                    .version_app
                    .as_ref()
                    .map(|v| format!(", app {v}"))
                    .unwrap_or_default(),
            );
            Ok(())
        }
        Orden::Comprobar { dir } => {
            let hallazgos = comprobar_sitio(&dir, &ListaPermitida::por_defecto())?;
            if hallazgos.is_empty() {
                println!("sitio correcto: {}", dir.display());
                Ok(())
            } else {
                Err(ErrorWeb::Comprobacion(hallazgos))
            }
        }
        Orden::Releases {
            salida,
            plataformas,
        } => {
            let plataformas = Plataformas::cargar(&plataformas)?;
            let documento = descargar_releases(&FuenteGh, &plataformas)?;
            documento.escribir(&salida)?;
            println!(
                "{} releases y {} sumas en {}",
                documento.releases.len(),
                documento.sumas.len(),
                salida.display()
            );
            Ok(())
        }
    }
}

fn main() -> ExitCode {
    match ejecutar(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
