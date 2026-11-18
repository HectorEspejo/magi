//! `magi termux teclas [--instalar]` y `magi termux aviso` (§5.5).
//!
//! CONTRATO-S3-TECLAS: el cuerpo de `teclas` llega en el Sprint 3; hasta
//! entonces lo dice y sale con 1. `aviso` (Sprint 2) publica un aviso de
//! prueba con las mismas órdenes que el servidor y lo retira a los 10 s.

use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use crate::config::{Config, Rutas};

use super::avisos::{self, ID_PRUEBA};
use super::ordenes::{ApiProceso, FalloOrden, Ordenes};
use super::EntornoTermux;

/// Cuánto dura el aviso de prueba antes de retirarse (§5.5).
pub const ESPERA_AVISO: Duration = Duration::from_secs(10);

/// Lo que pide `magi termux …`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrdenTermux {
    /// Enseña la fila propuesta y la actual; con `instalar`, pregunta y escribe.
    Teclas { instalar: bool },
    /// Publica un aviso de prueba y lo retira a los 10 s.
    Aviso,
}

/// Ejecuta la orden y devuelve el código de salida. Fuera de Termux dice
/// «solo en Termux» y sale con 1.
pub fn ejecutar(orden: OrdenTermux, rutas: &Rutas) -> anyhow::Result<i32> {
    if !super::es_termux() {
        eprintln!("magi termux: solo en Termux");
        return Ok(1);
    }
    match orden {
        OrdenTermux::Teclas { .. } => {
            eprintln!("magi termux: todavía sin implementar en esta versión");
            Ok(1)
        }
        OrdenTermux::Aviso => {
            // Solo hace falta el plazo de las órdenes; un `config.toml` que no
            // se lee deja el de siempre.
            let (config, _) = Config::cargar(&rutas.fichero_config());
            let ordenes = Ordenes::nuevas(
                &EntornoTermux::del_proceso(),
                config.termux.api_plazo(),
                Arc::new(ApiProceso::default()),
            );
            Ok(aviso_de_prueba(
                &ordenes,
                ESPERA_AVISO,
                &mut std::io::stdout(),
                &mut std::io::stderr(),
            ))
        }
    }
}

/// Publica el aviso de prueba, espera `espera` y lo retira. Las dos órdenes
/// son las que usa el servidor y, como ellas, van sin shell y con plazo;
/// ninguna lleva datos del inventario. Devuelve el código de salida: 0 si el
/// aviso se publicó y se retiró; 1 si no se pudo, diciendo por qué y qué
/// hacer (sobre todo si falta Termux:API o no responde).
///
/// Que la orden responda bien no prueba que el aviso se vea: en Android 13 o
/// posterior hace falta el permiso de notificaciones de Termux:API (R169), y
/// para eso es esta orden: se publica y se deja ver.
pub fn aviso_de_prueba(
    ordenes: &Ordenes,
    espera: Duration,
    salida: &mut dyn Write,
    error: &mut dyn Write,
) -> i32 {
    let titulo = avisos::titulo(&avisos::Origen::Persistente {
        nombre: "prueba".to_string(),
    });
    // El texto de la prueba es «MAGI · prueba»: no hay host que nombrar.
    let publicar = avisos::argumentos_con_texto(ID_PRUEBA, &titulo, "MAGI · prueba");
    let argumentos: Vec<&str> = publicar.iter().map(String::as_str).collect();
    if let Err(fallo) = ordenes.ejecutar(&argumentos, None) {
        let _ = writeln!(error, "magi termux aviso: {}", explicar(&fallo));
        return 1;
    }
    let _ = writeln!(
        salida,
        "Aviso de prueba publicado: míralo en la barra de notificaciones de Android."
    );
    let _ = writeln!(
        salida,
        "Se retira solo en {} s. Si no lo ves, Termux:API no tiene permiso para \
         notificar: Ajustes de Android > Aplicaciones > Termux:API > Notificaciones.",
        espera.as_secs()
    );
    let _ = salida.flush();
    std::thread::sleep(espera);
    let retirar = avisos::argumentos_retirar(ID_PRUEBA);
    let argumentos: Vec<&str> = retirar.iter().map(String::as_str).collect();
    match ordenes.ejecutar(&argumentos, None) {
        Ok(_) => {
            let _ = writeln!(salida, "Aviso de prueba retirado.");
            0
        }
        Err(fallo) => {
            let _ = writeln!(
                error,
                "magi termux aviso: no se pudo retirar el aviso de prueba ({}); \
                 quítalo a mano deslizándolo",
                explicar(&fallo)
            );
            1
        }
    }
}

/// Por qué falló una orden de Termux, con lo que hay que hacer.
fn explicar(fallo: &FalloOrden) -> String {
    match fallo {
        FalloOrden::NoInstalada(programa) => format!(
            "no se pudo publicar el aviso: falta {programa}; instala el paquete \
             con «pkg install termux-api» y la app Termux:API (de la misma \
             fuente que Termux)"
        ),
        FalloOrden::SinRespuesta | FalloOrden::ApiApagada => "Termux:API no responde: instala \
             la app Termux:API (de la misma fuente que Termux, F-Droid o GitHub), ábrela \
             una vez y quítale la restricción de batería"
            .to_string(),
        FalloOrden::Codigo { error, .. } if error.is_empty() => "Termux:API falló sin decir \
             por qué: abre la app Termux:API una vez y quítale la restricción de batería"
            .to_string(),
        FalloOrden::Codigo { error, .. } => format!(
            "Termux:API falló ({error}): abre la app Termux:API una vez y quítale la \
             restricción de batería"
        ),
        FalloOrden::Io(causa) => format!("no se pudo lanzar la orden: {causa}"),
    }
}
