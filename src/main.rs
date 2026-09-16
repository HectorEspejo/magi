use std::collections::HashMap;
use std::time::Duration;

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use tokio::io::AsyncWriteExt as _;
use tokio::net::UnixStream;
use tokio_stream::StreamExt as _;
use tokio_util::codec::FramedRead;

use magi::almacen;
use magi::almacen::Almacen;
use magi::app;
use magi::cliente;
use magi::config::{Config, Rutas};
use magi::flota::{self, PeticionSondeo};
use magi::modelo::{Host, ResultadoRegistro, ResultadoSondeo, Sondeo, Tunel};
use magi::protocolo::{
    self, EstadoTunelRemoto, InfoTunel, MensajeCliente, MensajeServidor, VERSION_PROTOCOLO,
};
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
    /// Activa o para un túnel del host indicado.
    Tunel {
        #[command(subcommand)]
        comando: ComandoTunel,
    },
    /// Lista los túneles definidos y activos.
    Tuneles,
}

#[derive(Subcommand)]
enum ComandoTunel {
    /// Levanta el túnel sobre el servidor de sesiones.
    Activar {
        /// Nombre del host en el inventario.
        host: String,
        /// Nombre del túnel en la ficha del host.
        nombre: String,
    },
    /// Para el túnel y cierra lo que estaba escuchando.
    Parar {
        /// Nombre del host en el inventario.
        host: String,
        /// Nombre del túnel en la ficha del host.
        nombre: String,
    },
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
        // Un pánico se anota en el log y el proceso sale con código 2: el
        // servidor no restaura terminales porque no las tiene.
        std::panic::set_hook(Box::new(|informacion| {
            tracing::error!("pánico del servidor: {informacion}");
            eprintln!("PÁNICO DEL SERVIDOR: {informacion}");
            std::process::exit(2);
        }));
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
        Some(Comando::Tunel { comando }) => {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .context("creando el runtime de tokio")?;
            let codigo = runtime.block_on(tunel_cli(&rutas, comando))?;
            std::process::exit(codigo);
        }
        Some(Comando::Tuneles) => {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .context("creando el runtime de tokio")?;
            let codigo = runtime.block_on(tuneles_cli(&rutas))?;
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
    // Reenvíos que no se pudieron convertir en túnel y se quedaron en opciones
    // extra: el usuario tiene que saberlo, no quedarse solo con el log.
    for aviso in &resumen.avisos {
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
    // El CLI no habla con el servidor de sesiones: sondeo efímero (F2).
    flota::lanzar_lote(
        &runtime,
        peticiones,
        tx,
        std::collections::HashSet::new(),
        magi::cliente::Cliente::sin_servidor(),
    );
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
        // R12: anotar solo las transiciones a CAÍDA y de vuelta.
        let (estado_previo, _) = flota::estado::evaluar(
            almacen.ultimo_sondeo(sondeo.host_id)?.as_ref(),
            &config.flota.umbrales,
        );
        let (estado_nuevo, _) = flota::estado::evaluar(Some(sondeo), &config.flota.umbrales);
        if estado_nuevo == flota::estado::EstadoFlota::Caida
            && estado_previo != flota::estado::EstadoFlota::Caida
        {
            registro::anotar(
                almacen.conexion(),
                registro::SONDEO_FALLIDO,
                Some(sondeo.host_id),
                None,
                &sondeo.error.clone().unwrap_or_default(),
                ResultadoRegistro::Error,
            )?;
        } else if estado_previo == flota::estado::EstadoFlota::Caida
            && estado_nuevo != flota::estado::EstadoFlota::Caida
        {
            registro::anotar(
                almacen.conexion(),
                registro::SONDEO_RECUPERADO,
                Some(sondeo.host_id),
                None,
                "el host responde de nuevo",
                ResultadoRegistro::Ok,
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

// ---------------------------------------------------------------- túneles

/// Desenlace de la espera de `ActivarTunel`/`PararTunel`.
enum DesenlaceTunel {
    /// El servidor hizo lo pedido. El texto es el estado que la difusión ya
    /// refleja, para colgarlo de la línea de éxito; vacío si aún no lo refleja.
    Hecho(String),
    /// El servidor rechazó la orden, con el motivo.
    Rechazado(String),
    /// El host necesita una huella, una frase o una contraseña y esta orden
    /// no tiene ventana que preguntarlas.
    SinCredenciales,
}

/// `magi tunel activar|parar <host> <nombre>`: levanta o para un túnel del
/// inventario sobre el servidor de sesiones (lo autolanza si no está). No
/// dialoga nunca: si el host pide credenciales, avisa y sale con error.
async fn tunel_cli(rutas: &Rutas, comando: ComandoTunel) -> anyhow::Result<i32> {
    let (host, nombre, activar) = match comando {
        ComandoTunel::Activar { host, nombre } => (host, nombre, true),
        ComandoTunel::Parar { host, nombre } => (host, nombre, false),
    };
    let Some(tunel_id) = tunel_definido(rutas, &host, &nombre)? else {
        return Ok(1);
    };
    let Some(mut stream) = conectar_servidor(rutas, true).await? else {
        eprintln!(
            "no hay servidor de sesiones en marcha ni se pudo arrancar uno ({}).",
            servidor::ruta_socket(rutas).display()
        );
        return Ok(1);
    };
    escribir(&mut stream, &saludo()).await?;
    let peticion_id = 1_u64;
    let peticion = if activar {
        MensajeCliente::ActivarTunel {
            tunel_id,
            peticion_id,
        }
    } else {
        MensajeCliente::PararTunel {
            tunel_id,
            peticion_id,
        }
    };
    escribir(&mut stream, &peticion).await?;
    let desenlace = tokio::time::timeout(
        Duration::from_secs(15),
        esperar_tunel(&mut stream, tunel_id, peticion_id, activar),
    )
    .await;
    // Cerrar el socket al salir: el servidor lo nota y no deja un cliente
    // colgado esperando respuestas que ya nadie lee.
    let _ = stream.shutdown().await;
    match desenlace {
        Ok(Ok(DesenlaceTunel::Hecho(sufijo))) => {
            let verbo = if activar { "activado" } else { "parado" };
            println!("Túnel «{nombre}» {verbo}{sufijo}.");
            Ok(0)
        }
        Ok(Ok(DesenlaceTunel::Rechazado(motivo))) => {
            eprintln!("{motivo}");
            Ok(1)
        }
        Ok(Ok(DesenlaceTunel::SinCredenciales)) => {
            eprintln!(
                "«{host}» necesita credenciales y esta orden no puede preguntarlas: abre una sesión desde la TUI primero."
            );
            Ok(1)
        }
        Ok(Err(motivo)) => {
            eprintln!("{motivo}");
            Ok(1)
        }
        Err(_) => {
            eprintln!("el servidor no contestó en 15 s");
            Ok(1)
        }
    }
}

/// Espera el desenlace de la petición: `Hecho`, `Error` o un diálogo de
/// credenciales que aquí no se puede atender. La difusión `Tuneles` y
/// `Bienvenida` se guardan para confirmar el estado final; cualquier otro
/// mensaje se ignora.
async fn esperar_tunel(
    stream: &mut UnixStream,
    tunel_id: i64,
    peticion_id: u64,
    activar: bool,
) -> anyhow::Result<DesenlaceTunel> {
    let mut ultima: Option<Vec<InfoTunel>> = None;
    let mut lector = FramedRead::new(&mut *stream, protocolo::codec());
    loop {
        let linea = match lector.next().await {
            Some(Ok(linea)) => linea,
            Some(Err(error)) => anyhow::bail!("respuesta no válida del servidor: {error}"),
            None => anyhow::bail!("el servidor cerró la conexión"),
        };
        let mensaje: MensajeServidor = match protocolo::decodificar(&linea) {
            Ok(mensaje) => mensaje,
            Err(motivo) => anyhow::bail!("{motivo}"),
        };
        match mensaje {
            MensajeServidor::Bienvenida { tuneles, .. } => ultima = Some(tuneles),
            MensajeServidor::Tuneles { lista } => ultima = Some(lista),
            MensajeServidor::Hecho { peticion_id: id } if id == peticion_id => {
                return Ok(DesenlaceTunel::Hecho(sufijo_estado(
                    ultima.as_deref(),
                    tunel_id,
                    activar,
                )));
            }
            MensajeServidor::Error {
                mensaje,
                peticion_id: Some(id),
            } if id == peticion_id => return Ok(DesenlaceTunel::Rechazado(mensaje)),
            MensajeServidor::PideContrasena { .. }
            | MensajeServidor::PideFrase { .. }
            | MensajeServidor::HuellaDesconocida { .. }
            | MensajeServidor::HuellaCambiada { .. } => return Ok(DesenlaceTunel::SinCredenciales),
            MensajeServidor::VersionIncompatible { version } => {
                anyhow::bail!(
                    "el servidor habla la versión de protocolo {version} y este MAGI la {VERSION_PROTOCOLO}: ciérralo con «magi servidor parar» y vuelve a abrir"
                );
            }
            _ => {}
        }
    }
}

/// Sufijo de la línea de éxito con el estado que la difusión ya refleja: al
/// activar, el estado y la escucha de verdad (que es la útil si se pidió el
/// puerto 0). Si la difusión todavía no lo refleja, no se añade nada.
fn sufijo_estado(ultima: Option<&[InfoTunel]>, tunel_id: i64, activar: bool) -> String {
    if !activar {
        return String::new();
    }
    let Some(info) = ultima.and_then(|lista| lista.iter().find(|info| info.tunel_id == tunel_id))
    else {
        return String::new();
    };
    if !info.estado.en_marcha() {
        return String::new();
    }
    format!(" · {} en {}", info.estado.texto(), info.escucha_mostrada())
}

/// `magi tuneles`: los túneles del inventario, cruzados por `tunel_id` con el
/// estado que difunde el servidor. Sin servidor se listan como inactivos.
async fn tuneles_cli(rutas: &Rutas) -> anyhow::Result<i32> {
    let definidos = listar_definidos(rutas)?;
    let mut estados: HashMap<i64, InfoTunel> = HashMap::new();
    let mut codigo = 0;
    match conectar_servidor(rutas, false).await? {
        Some(mut stream) => {
            escribir(&mut stream, &saludo()).await?;
            let lista = leer_tuneles(&mut stream).await;
            let _ = stream.shutdown().await;
            match lista {
                Ok(Some(lista)) => {
                    for info in lista {
                        estados.insert(info.tunel_id, info);
                    }
                }
                Ok(None) => {
                    eprintln!("el servidor no envió la lista de túneles en 2 s");
                    codigo = 1;
                }
                Err(motivo) => {
                    eprintln!("{motivo}");
                    codigo = 1;
                }
            }
        }
        None => {
            println!("No hay servidor de sesiones en marcha: los túneles no están levantados.");
            codigo = 1;
        }
    }
    imprime_tuneles(&definidos, &estados);
    Ok(codigo)
}

/// Tabla de túneles: estado del servidor cuando lo hay y, si no, la fila del
/// inventario como inactiva. Los caídos dejan su último error al pie.
fn imprime_tuneles(definidos: &[Tunel], estados: &HashMap<i64, InfoTunel>) {
    if definidos.is_empty() {
        println!("No hay túneles definidos.");
        return;
    }
    let activos = definidos
        .iter()
        .filter(|fila| {
            estados
                .get(&fila.id)
                .is_some_and(|info| info.estado.en_marcha())
        })
        .count();
    println!(
        "Túneles: {} definidos · {} activos",
        definidos.len(),
        activos
    );
    println!(
        "{:<6} {:<10} {:<10} {:<20} {:<20} {:<12} {:<8} TRÁFICO",
        "ID", "ESTADO", "TIPO", "ESCUCHA", "→ DESTINO", "HOST", "CONEX."
    );
    for fila in definidos {
        let info = estados.get(&fila.id);
        let estado = info
            .map(|info| info.estado.texto())
            .unwrap_or(EstadoTunelRemoto::Inactivo.texto());
        let escucha = info
            .map(|info| info.escucha_mostrada())
            .unwrap_or(&fila.escucha);
        let conexiones = match info {
            Some(info) => format!("{} ({})", info.conexiones, info.aceptadas),
            None => "—".to_string(),
        };
        let trafico = match info {
            Some(info) => format!(
                "↓ {} ↑ {}",
                magi::archivos::tamano_legible(info.bytes_bajados),
                magi::archivos::tamano_legible(info.bytes_subidos)
            ),
            None => "—".to_string(),
        };
        let destino = fila.destino.as_deref().unwrap_or("(socks5)");
        println!(
            "{:<6} {:<10} {:<10} {:<20} {:<20} {:<12} {:<8} {}",
            fila.id,
            estado,
            fila.tipo.etiqueta(),
            recortar(escucha, 20),
            format!("→ {}", recortar(destino, 18)),
            recortar(&fila.host_nombre, 12),
            conexiones,
            trafico,
        );
    }
    for fila in definidos {
        let Some(error) = estados
            .get(&fila.id)
            .filter(|info| info.estado == EstadoTunelRemoto::Caido)
            .and_then(|info| info.ultimo_error.as_deref())
        else {
            continue;
        };
        println!(
            "Último fallo: {} · {} — {error}",
            fila.host_nombre, fila.nombre
        );
    }
}

/// Recorta un texto al ancho de una columna, con puntos suspensivos.
fn recortar(texto: &str, ancho: usize) -> String {
    if texto.chars().count() <= ancho {
        return texto.to_string();
    }
    let corte: String = texto.chars().take(ancho.saturating_sub(1)).collect();
    format!("{corte}…")
}

/// Pide la lista de túneles al servidor ya saludado: llega en la `Bienvenida`
/// o en la difusión `Tuneles`. Devuelve `None` si no llega en 2 s.
async fn leer_tuneles(stream: &mut UnixStream) -> anyhow::Result<Option<Vec<InfoTunel>>> {
    let futura = async {
        let mut lector = FramedRead::new(&mut *stream, protocolo::codec());
        loop {
            let linea = match lector.next().await {
                Some(Ok(linea)) => linea,
                Some(Err(error)) => anyhow::bail!("respuesta no válida del servidor: {error}"),
                None => anyhow::bail!("el servidor cerró la conexión"),
            };
            let mensaje: MensajeServidor = match protocolo::decodificar(&linea) {
                Ok(mensaje) => mensaje,
                Err(motivo) => anyhow::bail!("{motivo}"),
            };
            match mensaje {
                MensajeServidor::Bienvenida { tuneles, .. } => return Ok(Some(tuneles)),
                MensajeServidor::Tuneles { lista } => return Ok(Some(lista)),
                MensajeServidor::VersionIncompatible { version } => {
                    anyhow::bail!(
                        "el servidor habla la versión de protocolo {version} y este MAGI la {VERSION_PROTOCOLO}: ciérralo con «magi servidor parar» y vuelve a abrir"
                    );
                }
                _ => {}
            }
        }
    };
    match tokio::time::timeout(Duration::from_secs(2), futura).await {
        Ok(resultado) => resultado,
        Err(_) => Ok(None),
    }
}

/// Conecta con el socket del servidor de sesiones. Con `autolanzar` lo arranca
/// desacoplado (como la TUI) si no está en marcha. Devuelve `None` si no hay
/// servidor al que hablar; el motivo lo imprime quien llama.
async fn conectar_servidor(rutas: &Rutas, autolanzar: bool) -> anyhow::Result<Option<UnixStream>> {
    let ruta = servidor::ruta_socket(rutas);
    if let Ok(stream) = UnixStream::connect(&ruta).await {
        return Ok(Some(stream));
    }
    if !autolanzar {
        return Ok(None);
    }
    cliente::lanzar_servidor(rutas);
    if !cliente::esperar_socket(&ruta, Duration::from_secs(5)).await {
        return Ok(None);
    }
    match UnixStream::connect(&ruta).await {
        Ok(stream) => Ok(Some(stream)),
        Err(error) => {
            eprintln!("no se pudo conectar con el servidor de sesiones: {error}");
            Ok(None)
        }
    }
}

/// Saludo de protocolo de cualquier cliente.
fn saludo() -> MensajeCliente {
    MensajeCliente::Hola {
        version: VERSION_PROTOCOLO,
        pid: std::process::id(),
    }
}

/// Escribe un mensaje en el socket como una línea JSON.
async fn escribir(stream: &mut UnixStream, mensaje: &MensajeCliente) -> anyhow::Result<()> {
    let linea = protocolo::codificar(mensaje).context("serializando un mensaje")?;
    stream.write_all(linea.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    stream.flush().await?;
    Ok(())
}

/// Id de la fila de `TUNELES` que pide la orden; `None` con el motivo ya
/// impreso si no existe el host o el túnel.
fn tunel_definido(rutas: &Rutas, host: &str, nombre: &str) -> anyhow::Result<Option<i64>> {
    let almacen = Almacen::abrir(&rutas.base_datos())?;
    let id = match almacen::hosts::por_nombre(almacen.conexion(), host)? {
        None => {
            eprintln!("no existe el host «{host}»");
            None
        }
        Some(fila) => match almacen.tunel_por_nombre(fila.id, nombre)? {
            None => {
                eprintln!("el host «{host}» no tiene un túnel «{nombre}»");
                None
            }
            Some(tunel) => Some(tunel.id),
        },
    };
    almacen.cerrar()?;
    Ok(id)
}

/// Túneles del inventario, de todos los hosts y por nombre de host.
fn listar_definidos(rutas: &Rutas) -> anyhow::Result<Vec<Tunel>> {
    let almacen = Almacen::abrir(&rutas.base_datos())?;
    let tuneles = almacen.listar_tuneles()?;
    almacen.cerrar()?;
    Ok(tuneles)
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
