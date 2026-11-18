//! `magi termux teclas [--instalar]` y `magi termux aviso` (§5.5).
//!
//! `teclas` enseña la fila de teclas propuesta y la actual de
//! `termux.properties` sin escribir nada; con `--instalar` pregunta
//! «¿instalar? [s/N]» en el terminal (sin terminal, error con instrucción y
//! nada escrito), hace la copia con fecha, escribe solo la línea `extra-keys`,
//! anota `termux_teclas_instaladas` y pide a Termux que recargue sus ajustes
//! (§4.8). `aviso` publica un aviso de prueba con las mismas órdenes que el
//! servidor y lo retira a los 10 s.
//!
//! Todo lo que habla con la persona pasa por el trait `Consola` del llavero
//! (la real o un doble de pruebas): así el flujo se prueba sin un terminal.

use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use chrono::NaiveDate;

use crate::config::{Config, Rutas};
use crate::llavero_cli::{Consola, ConsolaTerminal, ErrorEntrada};
use crate::modelo::ResultadoRegistro;
use crate::registro;

use super::avisos::{self, ID_PRUEBA};
use super::ordenes::{ApiProceso, FalloOrden, Ordenes};
use super::teclas::{self, EstadoFila, Propuesta};
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
        OrdenTermux::Teclas { instalar } => {
            // Hace falta el prefijo (la fila lo lleva como macro) y el plazo
            // de las órdenes; un `config.toml` que no se lee deja los de
            // siempre.
            let (config, _) = Config::cargar(&rutas.fichero_config());
            let entorno = EntornoTermux::del_proceso();
            let ordenes = Ordenes::nuevas(
                &entorno,
                config.termux.api_plazo(),
                Arc::new(ApiProceso::default()),
            );
            Ok(orden_teclas(
                instalar,
                &entorno,
                &config.prefijo_escape,
                &ordenes,
                rutas,
                &mut ConsolaTerminal,
                chrono::Local::now().date_naive(),
            ))
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

/// `magi termux teclas [--instalar]` (§4.8, §5.5). Devuelve el código de
/// salida.
///
/// Sin `instalar`, enseña la fila propuesta y la actual sin escribir nada. Con
/// `instalar`: si la fila ya es la de MAGI dice «ya está» (ni copia ni
/// escritura); si no, hace falta un terminal (sin él, error con instrucción y
/// nada escrito), enseña qué se va a tocar, pregunta «¿instalar? [s/N]» y, con
/// «s», hace la copia con fecha, escribe solo la línea `extra-keys`, anota
/// `termux_teclas_instaladas` (con el efecto ya producido) y pide a Termux que
/// recargue sus ajustes: si no responde, dice que hay que reiniciarlo.
///
/// Sale con 0 si se enseñó, se instaló o ya estaba, y con 1 si el fichero no se
/// puede leer, no hay terminal, se respondió que no o falló algo.
pub fn orden_teclas(
    instalar: bool,
    entorno: &EntornoTermux,
    prefijo_escape: &str,
    ordenes: &Ordenes,
    rutas: &Rutas,
    consola: &mut dyn Consola,
    fecha: NaiveDate,
) -> i32 {
    let propuesta = teclas::preparar(entorno, prefijo_escape, fecha);
    mostrar_propuesta(consola, &propuesta, prefijo_escape);

    match &propuesta.estado {
        EstadoFila::Ilegible(motivo) => {
            consola.avisar(&format!(
                "magi termux teclas: termux.properties no se puede leer ({motivo}): edítalo a mano. No se ha tocado nada"
            ));
            return 1;
        }
        EstadoFila::LaDeMagi => {
            consola.decir(if instalar {
                "Ya está: la fila de teclas de MAGI ya es la de termux.properties. No se ha copiado ni escrito nada."
            } else {
                "Es la fila de MAGI: no hace falta instalar nada."
            });
            return 0;
        }
        EstadoFila::SinFichero | EstadoFila::SinExtraKeys | EstadoFila::Otra => {}
    }
    if !instalar {
        consola.decir("Para instalarla: magi termux teclas --instalar");
        return 0;
    }
    let Some(ruta) = propuesta.ruta.clone() else {
        consola.avisar("magi termux teclas: no está puesto $HOME. No se ha escrito nada");
        return 1;
    };
    if !consola.es_interactiva() {
        consola.avisar(
            "magi termux teclas --instalar: hace falta un terminal para preguntar «¿instalar?». Ejecútala en una sesión de Termux, sin redirigir la entrada estándar ni la salida de error, y no desde un guion. No se ha escrito nada",
        );
        return 1;
    }
    consola.avisar(&texto_de_lo_que_se_va_a_tocar(&propuesta));
    match consola.leer_linea("¿instalar? [s/N] ") {
        Ok(respuesta) if es_afirmativa(&respuesta) => {}
        Ok(_) | Err(ErrorEntrada::Cancelada) => {
            consola.avisar("Cancelado: no se ha instalado nada.");
            return 1;
        }
        Err(ErrorEntrada::Terminal(motivo)) => {
            consola.avisar(&format!(
                "magi termux teclas: {motivo}. No se ha escrito nada"
            ));
            return 1;
        }
    }

    // La ruta es la que se enseñó, no se vuelve a calcular.
    match teclas::escribir_fila(&ruta, prefijo_escape, fecha) {
        Err(error) => {
            consola.avisar(&format!("magi termux teclas: {error}"));
            1
        }
        Ok(escritura) if escritura.ya_estaba => {
            consola.decir("Ya está: la fila de MAGI ya estaba. No se ha copiado ni escrito nada.");
            0
        }
        Ok(escritura) => {
            // El efecto ya se ha producido: se anota ahora (y si el registro
            // falla, el fichero ya está escrito y no se deshace).
            if let Err(motivo) = anotar_instalacion(rutas, &escritura.detalle_de_registro()) {
                consola.avisar(&format!(
                    "aviso: no se pudo anotar en el registro de MAGI ({motivo})"
                ));
            }
            consola.decir(&format!(
                "Fila de teclas instalada en {}.",
                propuesta.ruta_visible
            ));
            match &escritura.copia {
                Some(copia) => consola.decir(&format!(
                    "Copia anterior: {copia} (en el mismo directorio)."
                )),
                None => consola.decir("El fichero no existía: no hay copia."),
            }
            match teclas::recargar(ordenes) {
                Ok(()) => consola.decir(&format!("{}.", teclas::texto_de_recarga(&Ok(())))),
                Err(fallo) => consola.avisar(&format!(
                    "No se pudo recargar Termux ({}): {}.",
                    teclas::motivo_de_recarga(&fallo),
                    teclas::REINICIA_TERMUX
                )),
            }
            0
        }
    }
}

/// La fila propuesta y la actual, por la salida estándar.
fn mostrar_propuesta(consola: &mut dyn Consola, propuesta: &Propuesta, prefijo_escape: &str) {
    consola.decir(&format!(
        "Fila de teclas que propone MAGI (prefijo {prefijo_escape}):"
    ));
    consola.decir(&format!("  {}", propuesta.fila));
    consola.decir(&format!("  arriba: {}", propuesta.arriba.join(" ")));
    consola.decir(&format!("  abajo:  {}", propuesta.abajo.join(" ")));
    if let Some(aviso) = &propuesta.aviso_prefijo {
        consola.decir(&format!("  aviso: el prefijo no va en la fila ({aviso})"));
    }
    consola.decir(&format!("Fichero: {}", propuesta.ruta_visible));
    let actual = match &propuesta.estado {
        EstadoFila::SinFichero => "el fichero no existe".to_string(),
        EstadoFila::SinExtraKeys => {
            "no hay línea extra-keys: Termux usa la suya por defecto".to_string()
        }
        EstadoFila::Ilegible(_) => "no se puede leer".to_string(),
        EstadoFila::LaDeMagi | EstadoFila::Otra => match &propuesta.actual {
            Some(valor) => format!("extra-keys = {valor}"),
            None => "no hay línea extra-keys".to_string(),
        },
    };
    consola.decir(&format!("Fila actual: {actual}"));
}

/// Lo que se va a tocar, antes de preguntar.
fn texto_de_lo_que_se_va_a_tocar(propuesta: &Propuesta) -> String {
    match &propuesta.copia {
        Some(copia) => format!(
            "Se va a escribir {} (solo la línea extra-keys). Antes, una copia en el mismo directorio: {copia}",
            propuesta.ruta_visible
        ),
        None => format!(
            "Se va a crear {} con la línea extra-keys. No existe todavía: no hay nada que copiar.",
            propuesta.ruta_visible
        ),
    }
}

/// «s», «si» o «sí» (sin distinguir mayúsculas): todo lo demás es que no.
fn es_afirmativa(respuesta: &str) -> bool {
    matches!(respuesta.trim().to_lowercase().as_str(), "s" | "si" | "sí")
}

/// Anota `termux_teclas_instaladas` abriendo el almacén como las demás órdenes
/// de la CLI. Si no se puede, devuelve el motivo para que la orden lo avise.
fn anotar_instalacion(rutas: &Rutas, detalle: &str) -> Result<(), String> {
    let almacen =
        crate::almacen::abrir(&rutas.base_datos()).map_err(|causa| format!("{causa:#}"))?;
    let resultado = registro::anotar(
        almacen.conexion(),
        registro::TERMUX_TECLAS_INSTALADAS,
        None,
        None,
        detalle,
        ResultadoRegistro::Ok,
    );
    let cierre = almacen.cerrar();
    resultado.map_err(|causa| format!("{causa:#}"))?;
    cierre.map_err(|causa| format!("{causa:#}"))
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
