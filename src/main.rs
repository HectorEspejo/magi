use clap::{Parser, Subcommand};

use magi::almacen::Almacen;
use magi::app;
use magi::config::{Config, Rutas};
use magi::sshconfig;
use magi::tema;

#[derive(Parser)]
#[command(
    name = "magi",
    version,
    about = "Gestor SSH de terminal (TUI) con estética de cabina técnica"
)]
struct Cli {
    #[command(subcommand)]
    comando: Option<Comando>,
}

#[derive(Subcommand)]
enum Comando {
    /// Importa un ssh_config (por defecto ~/.ssh/config) sin abrir la TUI.
    Importar {
        /// Ruta del ssh_config que se importa.
        ruta: Option<std::path::PathBuf>,
    },
    /// Regenera ~/.ssh/magi_config sin abrir la TUI.
    Exportar,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let rutas = Rutas::descubrir()?;
    let _guardia_log = iniciar_log(&rutas)?;
    let (config, aviso_config) = Config::cargar(&rutas.fichero_config());
    let ruta_omarchy = rutas
        .hogar
        .join(".config/omarchy/current/theme/alacritty.toml");
    let (tema, aviso_tema) = tema::cargar(&config, &ruta_omarchy);

    match cli.comando {
        Some(Comando::Importar { ruta }) => {
            let ruta = ruta.unwrap_or_else(|| rutas.fichero_ssh_config());
            let almacen = Almacen::abrir(&rutas.base_datos())?;
            importar(&almacen, &rutas, &ruta)?;
            almacen.cerrar()?;
            Ok(())
        }
        Some(Comando::Exportar) => {
            let almacen = Almacen::abrir(&rutas.base_datos())?;
            exportar(&almacen, &rutas)?;
            almacen.cerrar()?;
            Ok(())
        }
        None => {
            app::instalar_hook_panico();
            let almacen = Almacen::abrir(&rutas.base_datos())?;
            let avisos: Vec<String> = [aviso_config, aviso_tema].into_iter().flatten().collect();
            let aviso = if avisos.is_empty() {
                None
            } else {
                Some(avisos.join(" · "))
            };
            app::ejecutar(rutas, config, tema, almacen, aviso)
        }
    }
}

fn importar(almacen: &Almacen, rutas: &Rutas, ruta: &std::path::Path) -> anyhow::Result<()> {
    let analisis = match sshconfig::importar::analizar_fichero(ruta, &rutas.hogar) {
        Ok(analisis) => analisis,
        Err(error) => {
            eprintln!("No se pudo importar {}: {error}", ruta.display());
            std::process::exit(1);
        }
    };
    let conflictos = sshconfig::importar::conflictos(almacen.conexion(), &analisis)?;
    let mut decisiones = std::collections::HashMap::new();
    for nombre in &conflictos {
        decisiones.insert(nombre.clone(), false);
    }
    let resumen = sshconfig::importar::aplicar(almacen.conexion(), &analisis, &decisiones)?;
    println!("Importados: {}", resumen.importados);
    println!("Sobrescritos: {}", resumen.sobrescritos);
    println!(
        "Omitidos: {}",
        resumen.omitidos.len() + analisis.avisos.len()
    );
    for omitido in &resumen.omitidos {
        println!("  · {}: {}", omitido.descripcion, omitido.motivo);
    }
    for aviso in &analisis.avisos {
        println!("  · aviso: {aviso}");
    }
    if !conflictos.is_empty() {
        println!(
            "Nota: {} conflicto(s) se han omitido; usa la TUI para sobrescribir.",
            conflictos.len()
        );
    }
    Ok(())
}

fn exportar(almacen: &Almacen, rutas: &Rutas) -> anyhow::Result<()> {
    let resultado = sshconfig::exportar::exportar(almacen.conexion(), &rutas.dir_ssh())?;
    println!(
        "Exportados {} hosts a {}",
        resultado.hosts,
        resultado.ruta.display()
    );
    if !resultado.include_presente {
        println!("Añade esta línea al principio de ~/.ssh/config:\n    Include ~/.ssh/magi_config");
    }
    Ok(())
}

fn iniciar_log(
    rutas: &Rutas,
) -> anyhow::Result<Option<tracing_appender::non_blocking::WorkerGuard>> {
    let directorio = rutas.dir_logs();
    std::fs::create_dir_all(&directorio)?;
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("magi.log")
        .build(&directorio)?;
    let (escritor, guardia) = tracing_appender::non_blocking(appender);
    let filtro = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_writer(escritor)
        .with_ansi(false)
        .with_env_filter(filtro)
        .init();
    Ok(Some(guardia))
}
