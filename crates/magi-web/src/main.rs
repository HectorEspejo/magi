//! `magi-web`: construir, comprobar y preparar la web de MAGI.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};

use magi_web::comprobar::{comprobar_sitio, ListaPermitida};
use magi_web::construir::{construir, OpcionesConstruir};
use magi_web::datos::Plataformas;
use magi_web::descargas::{elegir, OpcionesDescargas};
use magi_web::error::{ErrorWeb, Resultado};
use magi_web::manual::{preparar_manual, OpcionesManual, OrigenManual};
use magi_web::releases::{descargar_releases, DocumentoReleases, FuenteGh};

/// Canal del que se pregunta la etiqueta de la última versión publicada.
#[derive(Clone, Copy, ValueEnum)]
enum CanalEtiqueta {
    /// Escritorio y Termux: `vX.Y.Z`.
    Escritorio,
    /// App de Android: `android-vX.Y.Z`.
    App,
}

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
        manual: PathBuf,
        /// Animación de la portada (`assets/demo/magi.gif`).
        #[arg(long)]
        animacion: Option<PathBuf>,
        /// CHANGELOG del escritorio de la etiqueta de la última versión publicada.
        #[arg(long)]
        changelog: PathBuf,
        /// CHANGELOG de la app de su etiqueta (si hay alguna release de la app).
        #[arg(long)]
        changelog_app: Option<PathBuf>,
        /// Carpeta de salida: se sustituye entera si no existe, está vacía o es un sitio
        /// anterior de magi-web; cualquier otra carpeta se rechaza.
        #[arg(long)]
        salida: PathBuf,
        /// Mostrar el bloque de Homebrew (`PUBLICAR_HOMEBREW=true`).
        #[arg(long)]
        homebrew: bool,
        /// Mostrar el bloque de AUR (`PUBLICAR_AUR=true`).
        #[arg(long)]
        aur: bool,
    },
    /// Prepara el manual para mdBook: copia `docs/manual/` de una etiqueta (o de un
    /// árbol de trabajo), escribe `SUMMARY.md`, fija las anclas de GitHub y reescribe
    /// los enlaces que salen del manual. Deja la raíz de un libro (`book.toml`,
    /// `theme/` y `src/`) lista para `mdbook build`.
    PrepararManual {
        /// Etiqueta de la última versión publicada del escritorio (`v1.6.1`).
        #[arg(long)]
        etiqueta: String,
        /// Lee `docs/manual/` de este árbol de trabajo en vez de la etiqueta
        /// (la rama de una PR y `generar-web.sh --sin-red`).
        #[arg(long)]
        arbol: Option<PathBuf>,
        /// Repositorio git del que se lee la etiqueta.
        #[arg(long, default_value = ".")]
        repositorio: PathBuf,
        /// Carpeta del tema de mdBook (`book.toml` y `theme/`).
        #[arg(long, default_value = "web/manual")]
        tema: PathBuf,
        /// Carpeta de salida: la raíz del libro (se sustituye entera).
        #[arg(long)]
        salida: PathBuf,
    },
    /// Revisa un sitio generado: enlaces internos y anclas, externos permitidos,
    /// sin `style` ni scripts propios y CSP en cada página.
    Comprobar {
        /// Carpeta del sitio.
        dir: PathBuf,
    },
    /// Escribe la etiqueta de la última versión publicada de un canal (nunca un
    /// borrador ni una candidata), para sacar de ella el manual y los CHANGELOG.
    /// Sin ninguna release de la app, no escribe nada.
    Etiqueta {
        /// `releases.json` (lo escribe `magi-web releases`).
        #[arg(long)]
        releases: PathBuf,
        /// Canal del que se pregunta.
        #[arg(long, value_enum)]
        canal: CanalEtiqueta,
        /// `plataformas.toml`.
        #[arg(long, default_value = "web/datos/plataformas.toml")]
        plataformas: PathBuf,
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
            changelog,
            changelog_app,
            salida,
            homebrew,
            aur,
        } => {
            let informe = construir(&OpcionesConstruir {
                web,
                releases,
                manual,
                animacion,
                changelog_escritorio: changelog,
                changelog_app,
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
        Orden::PrepararManual {
            etiqueta,
            arbol,
            repositorio,
            tema,
            salida,
        } => {
            let origen = match arbol {
                Some(carpeta) => OrigenManual::Arbol { carpeta },
                None => OrigenManual::Etiqueta { repositorio },
            };
            let informe = preparar_manual(&OpcionesManual {
                origen,
                etiqueta,
                tema,
                salida: salida.clone(),
            })?;
            println!(
                "manual preparado en {}: {} capítulos, {} ficheros",
                salida.display(),
                informe.capitulos.len(),
                informe.ficheros.len()
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
        Orden::Etiqueta {
            releases,
            canal,
            plataformas,
        } => {
            // La misma elección que hace `construir`, con sus mismas validaciones:
            // la etiqueta que se imprime es la que saldrá en la web.
            let plataformas = Plataformas::cargar(&plataformas)?;
            let documento = DocumentoReleases::leer(&releases)?;
            let eleccion = elegir(&documento, &plataformas, OpcionesDescargas::default())?;
            match canal {
                CanalEtiqueta::Escritorio => println!("{}", eleccion.escritorio.etiqueta),
                CanalEtiqueta::App => {
                    if let Some(app) = &eleccion.app {
                        println!("{}", app.etiqueta);
                    }
                }
            }
            Ok(())
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
