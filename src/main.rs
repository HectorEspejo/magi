use anyhow::Context as _;
use clap::{Parser, Subcommand};

use magi::almacen;
use magi::almacen::Almacen;
use magi::app;
use magi::conexion;
use magi::config::{Config, Rutas};
use magi::flota::{self, PeticionSondeo};
use magi::modelo::{Host, ResultadoRegistro, ResultadoSondeo, Sondeo};
use magi::registro;
use magi::servidor;
use magi::sshconfig;
use magi::tema;

#[derive(Parser)]
#[command(
    name = "magi",
    version,
    about = "Gestor SSH de terminal (TUI) con estética de cabina técnica"
)]
struct Cli {
    /// Ejecuta el servidor de sesiones en primer plano.
    #[arg(long)]
    servidor: bool,

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
    /// Sondea la flota sin abrir la TUI.
    Sondear {
        /// Hosts a sondear; sin argumentos se sondean todos.
        hosts: Vec<String>,
    },
    /// Exporta el historial de MAGI.
    Registro {
        #[command(subcommand)]
        comando: ComandoRegistro,
    },
    /// El servidor de sesiones: estado o parada.
    Servidor {
        #[command(subcommand)]
        comando: ComandoServidor,
    },
    /// Abre la TUI con una sesión nueva al host indicado.
    Conectar { host: String },
}

#[derive(Subcommand)]
enum ComandoServidor {
    /// Imprime pid, versión de protocolo, sesiones y clientes del servidor.
    Estado,
    /// Cierra todas las sesiones y apaga el servidor.
    Parar {
        /// No pide confirmación aunque haya sesiones abiertas.
        #[arg(long)]
        si: bool,
    },
}

#[derive(Subcommand)]
enum ComandoRegistro {
    /// Exporta el registro a CSV (por defecto) o JSON.
    Exportar {
        /// Ruta del fichero de salida.
        ruta: std::path::PathBuf,
        /// Exporta en JSON en lugar de CSV.
        #[arg(long)]
        json: bool,
        /// Solo entradas desde esta fecha (AAAA-MM-DD).
        #[arg(long)]
        desde: Option<String>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let rutas = Rutas::descubrir()?;
    let log_servidor = cli.servidor;
    let _guardia_log = iniciar_log(
        &rutas,
        if log_servidor {
            "servidor.log"
        } else {
            "magi.log"
        },
    )?;
    let (config, aviso_config) = Config::cargar(&rutas.fichero_config());
    let ruta_omarchy = rutas
        .hogar
        .join(".config/omarchy/current/theme/alacritty.toml");
    let (tema, aviso_tema) = tema::cargar(&config, &ruta_omarchy);

    if cli.servidor {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .context("creando el runtime de tokio")?;
        return runtime.block_on(servidor::arrancar(rutas, config));
    }

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
        Some(Comando::Sondear { hosts }) => {
            let almacen = Almacen::abrir(&rutas.base_datos())?;
            sondear(&almacen, &rutas, &config, &hosts)?;
            almacen.cerrar()?;
            Ok(())
        }
        Some(Comando::Registro { comando }) => {
            let ComandoRegistro::Exportar { ruta, json, desde } = comando;
            let almacen = Almacen::abrir(&rutas.base_datos())?;
            exportar_registro(&almacen, &ruta, json, desde.as_deref())?;
            almacen.cerrar()?;
            Ok(())
        }
        Some(Comando::Servidor { comando }) => {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .context("creando el runtime de tokio")?;
            let codigo = runtime.block_on(async {
                match comando {
                    ComandoServidor::Estado => servidor::estado_cli(&rutas).await,
                    ComandoServidor::Parar { si } => servidor::parar_cli(&rutas, si).await,
                }
            })?;
            std::process::exit(codigo);
        }
        Some(Comando::Conectar { host }) => {
            let almacen = Almacen::abrir(&rutas.base_datos())?;
            let existente = almacen::hosts::por_nombre(almacen.conexion(), &host)?;
            let Some(host) = existente else {
                eprintln!("no existe el host «{host}»");
                std::process::exit(1);
            };
            let avisos: Vec<String> = [aviso_config, aviso_tema].into_iter().flatten().collect();
            let aviso = if avisos.is_empty() {
                None
            } else {
                Some(avisos.join(" · "))
            };
            app::ejecutar(rutas, config, tema, almacen, aviso, Some(host.id))
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
            app::ejecutar(rutas, config, tema, almacen, aviso, None)
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
    registro::anotar(
        almacen.conexion(),
        registro::IMPORTACION,
        None,
        None,
        &format!(
            "importados {} · sobrescritos {} · omitidos {}",
            resumen.importados,
            resumen.sobrescritos,
            resumen.omitidos.len() + analisis.avisos.len()
        ),
        ResultadoRegistro::Ok,
    )?;
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
    registro::anotar(
        almacen.conexion(),
        registro::EXPORTACION,
        None,
        None,
        &format!("{} hosts → {}", resultado.hosts, resultado.ruta.display()),
        ResultadoRegistro::Ok,
    )?;
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

fn exportar_registro(
    almacen: &Almacen,
    ruta: &std::path::Path,
    json: bool,
    desde: Option<&str>,
) -> anyhow::Result<()> {
    let entradas = almacen.registro_para_exportar(desde)?;
    let total = if json {
        registro::exportar_json(ruta, &entradas)?
    } else {
        registro::exportar_csv(ruta, &entradas)?
    };
    println!(
        "Exportadas {total} entradas a {} ({})",
        ruta.display(),
        if json { "JSON" } else { "CSV" }
    );
    Ok(())
}

fn sondear(
    almacen: &Almacen,
    rutas: &Rutas,
    config: &Config,
    nombres: &[String],
) -> anyhow::Result<()> {
    let todos: std::collections::HashMap<i64, Host> = almacen
        .listar_hosts()?
        .into_iter()
        .map(|host| (host.id, host))
        .collect();
    let mut seleccionados: Vec<Host> = Vec::new();
    if nombres.is_empty() {
        seleccionados = todos.values().cloned().collect();
    } else {
        for nombre in nombres {
            match todos.values().find(|host| &host.nombre == nombre) {
                Some(host) => seleccionados.push(host.clone()),
                None => eprintln!("no existe el host «{nombre}»"),
            }
        }
    }
    if seleccionados.is_empty() {
        anyhow::bail!("no hay hosts que sondear");
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let usuario = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "root".to_string());
    let peticiones: Vec<PeticionSondeo> = seleccionados
        .iter()
        .map(|host| PeticionSondeo {
            host: host.clone(),
            todos_los_hosts: todos.clone(),
            known_hosts: rutas.fichero_known_hosts(),
            dir_ssh: rutas.dir_ssh(),
            hogar: rutas.hogar.clone(),
            usuario_local: usuario.clone(),
            servicios: magi::modelo::servicios_de(host),
        })
        .collect();
    let total = peticiones.len();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Sondeo>();
    flota::lanzar_lote(&runtime, peticiones, tx, conexion::registro_sesiones());
    let mut resultados: Vec<Sondeo> = Vec::with_capacity(total);
    for _ in 0..total {
        match rx.blocking_recv() {
            Some(sondeo) => resultados.push(sondeo),
            None => break,
        }
    }
    resultados.sort_by(|a, b| {
        let nombre = |sondeo: &Sondeo| {
            todos
                .get(&sondeo.host_id)
                .map(|host| host.nombre.clone())
                .unwrap_or_default()
        };
        nombre(a).cmp(&nombre(b))
    });

    println!(
        "{:<22} {:<10} {:<9} {:<5} {:<5} SERVICIOS",
        "HOST", "ESTADO", "CARGA", "MEM", "DSK"
    );
    for sondeo in &resultados {
        if sondeo.resultado == ResultadoSondeo::Error {
            let motivo = sondeo.error.clone().unwrap_or_default();
            registro::anotar(
                almacen.conexion(),
                registro::SONDEO_FALLIDO,
                Some(sondeo.host_id),
                None,
                &motivo,
                ResultadoRegistro::Error,
            )?;
        }
        almacen.guardar_sondeo(sondeo)?;
        let nombre = todos
            .get(&sondeo.host_id)
            .map(|host| host.nombre.as_str())
            .unwrap_or("?");
        let (estado, _) = flota::estado::evaluar(Some(sondeo), &config.flota.umbrales);
        let carga = match (sondeo.carga_1m, sondeo.nucleos) {
            (Some(carga), Some(nucleos)) => format!("{carga:.1}/{nucleos}"),
            (Some(carga), None) => format!("{carga:.1}"),
            _ => "—".to_string(),
        };
        let memoria = sondeo
            .memoria_pct()
            .map(|pct| format!("{pct:.0}%"))
            .unwrap_or_else(|| "—".to_string());
        let disco = sondeo
            .disco_pct()
            .map(|pct| format!("{pct:.0}%"))
            .unwrap_or_else(|| "—".to_string());
        let servicios = if sondeo.servicios.is_empty() {
            "—".to_string()
        } else {
            sondeo
                .servicios
                .iter()
                .map(|(unidad, valor)| format!("{unidad}={valor}"))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let detalle = if sondeo.resultado == ResultadoSondeo::Error {
            sondeo.error.clone().unwrap_or_default()
        } else {
            servicios
        };
        println!(
            "{:<22} {:<10} {:<9} {:<5} {:<5} {}",
            nombre,
            estado.palabra(),
            carga,
            memoria,
            disco,
            detalle
        );
    }
    Ok(())
}

fn iniciar_log(
    rutas: &Rutas,
    prefijo: &str,
) -> anyhow::Result<Option<tracing_appender::non_blocking::WorkerGuard>> {
    let directorio = rutas.dir_logs();
    std::fs::create_dir_all(&directorio)?;
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix(prefijo)
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
