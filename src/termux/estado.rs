//! Ficheros de estado de Termux en `<estado>/termux/` (§3.4): marcas y lo que
//! el servidor deja para la barra del cliente.
//!
//! Lo escribe el servidor (`wake-lock`, `avisos`, `servidor`) o el cliente
//! (`primer-arranque`) y lo leen los dos. Todo pasa por `ficheros.rs` (atómico,
//! 600). Nada de aquí lleva datos del inventario, del remoto ni secretos.
//!
//! - `wake-lock`: marca; MAGI tiene tomado el wake lock de Termux. Si el
//!   servidor muere sin soltarlo, el siguiente lo suelta en cuanto no tiene
//!   trabajo.
//! - `avisos`: los ids de los avisos de Android publicados, uno por línea, para
//!   retirarlos si el servidor muere.
//! - `segundo-plano`: marca; el aviso único sobre el límite de procesos en
//!   segundo plano de Android 12 o posterior ya se dio.
//! - `primer-arranque`: marca; la fila de teclas ya se ofreció (Sprint 3).
//! - `servidor`: JSON mínimo con lo que el servidor quiere decir en la barra
//!   (D386: el protocolo está congelado en la versión 10, así que el aviso va
//!   por fichero y el cliente lo mira cada pocos segundos, solo en Termux).

use std::collections::BTreeSet;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::config::Rutas;
use crate::ficheros::escribir_atomico;

/// Subdirectorio de estado (`~/.local/state/magi/termux/`).
pub const DIRECTORIO: &str = "termux";

/// Tope de lo que se lee de un fichero de estado (entrada no fiable: otro
/// proceso pudo escribirlo).
const TOPE_LECTURA: u64 = 64 * 1024;

/// Ids distintos que se aceptan al leer el estado de los avisos: cuatro veces
/// los que el servidor publica a la vez como mucho
/// ([`super::avisos::MAXIMO_AVISOS`]). En 64 KiB caben unos 8 000 ids
/// válidos, y retirarlos todos en un móvil sería una avalancha de órdenes: el
/// resto se ignora.
pub const MAXIMO_AVISOS_LEIDOS: usize = 4 * super::avisos::MAXIMO_AVISOS;

/// Las marcas del estado de Termux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marca {
    /// MAGI tiene tomado el wake lock.
    WakeLock,
    /// Ya se dio el aviso único de Android 12 o posterior.
    SegundoPlano,
    /// Ya se ofreció la fila de teclas en el primer arranque.
    PrimerArranque,
}

impl Marca {
    fn nombre(self) -> &'static str {
        match self {
            Self::WakeLock => "wake-lock",
            Self::SegundoPlano => "segundo-plano",
            Self::PrimerArranque => "primer-arranque",
        }
    }
}

/// Lo que el servidor le dice al cliente por la barra, una vez cada cosa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AvisoServidor {
    /// Termux:API está apagada en el servidor: sin avisos de Android.
    ApiApagada,
    /// No se pudo tomar el wake lock: Android puede dormir MAGI.
    SinWakeLock,
    /// Android 12 o posterior: puede cerrar procesos de Termux (`magi
    /// diagnostico`).
    LimiteProcesos,
}

impl AvisoServidor {
    /// Lo que dice la barra del cliente (una sola fuente del texto: el
    /// servidor escribe el aviso y el cliente lo pinta).
    #[must_use]
    pub fn texto(self) -> &'static str {
        match self {
            Self::ApiApagada => {
                "Termux:API no responde en el servidor: sin avisos de Android (magi diagnostico)"
            }
            Self::SinWakeLock => "sin wake lock: Android puede dormir MAGI",
            Self::LimiteProcesos => {
                "Android puede cerrar procesos de Termux: mira magi diagnostico"
            }
        }
    }
}

/// El fichero `servidor`: qué servidor lo escribió (`arranque`, época en
/// segundos; un servidor nuevo vuelve a decir sus avisos) y qué dice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EstadoServidorTermux {
    pub arranque: i64,
    #[serde(default)]
    pub avisos: Vec<AvisoServidor>,
}

/// `<estado>/termux/`.
#[must_use]
pub fn directorio(rutas: &Rutas) -> PathBuf {
    rutas.estado.join(DIRECTORIO)
}

fn ruta(rutas: &Rutas, nombre: &str) -> PathBuf {
    directorio(rutas).join(nombre)
}

fn crear_directorio(rutas: &Rutas) -> Result<()> {
    let directorio = directorio(rutas);
    std::fs::create_dir_all(&directorio)
        .with_context(|| format!("creando {}", directorio.display()))
}

/// ¿Está puesta la marca?
#[must_use]
pub fn hay_marca(rutas: &Rutas, marca: Marca) -> bool {
    ruta(rutas, marca.nombre()).exists()
}

/// Pone la marca (vacía, 600).
pub fn poner_marca(rutas: &Rutas, marca: Marca) -> Result<()> {
    crear_directorio(rutas)?;
    escribir_atomico(&ruta(rutas, marca.nombre()), b"", 0o600)
}

/// Quita la marca; si no estaba, no es un error.
pub fn quitar_marca(rutas: &Rutas, marca: Marca) -> Result<()> {
    match std::fs::remove_file(ruta(rutas, marca.nombre())) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("quitando la marca {}", marca.nombre())),
    }
}

/// ¿Es un id de aviso que MAGI publica? `^magi-p?[0-9a-z]{1,16}$` (§7.10).
#[must_use]
pub fn id_de_aviso_valido(id: &str) -> bool {
    let Some(resto) = id.strip_prefix("magi-") else {
        return false;
    };
    let resto = resto.strip_prefix('p').unwrap_or(resto);
    (1..=16).contains(&resto.len())
        && resto
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte.is_ascii_lowercase())
}

/// Lee un fichero de estado con tope; `None` si no existe, no es un fichero
/// normal o no se puede leer. Es entrada no fiable: se abre sin seguir
/// enlaces y sin esperar (un FIFO dejaría el `open` colgado para siempre) y
/// solo se lee si, ya abierto, es un fichero normal.
fn leer(rutas: &Rutas, nombre: &str) -> Option<String> {
    use std::io::Read as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let fichero = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(ruta(rutas, nombre))
        .ok()?;
    if !fichero.metadata().ok()?.is_file() {
        return None;
    }
    let mut texto = String::new();
    fichero.take(TOPE_LECTURA).read_to_string(&mut texto).ok()?;
    Some(texto)
}

/// Los ids de los avisos publicados. Se ignora todo lo que no sea un id
/// válido: el fichero es de otro proceso y nunca se pasa a una orden sin
/// comprobarlo. Y no se aceptan más de [`MAXIMO_AVISOS_LEIDOS`] ids distintos:
/// los primeros del fichero, y de los demás solo se avisa en el log, sin sus
/// ids.
#[must_use]
pub fn leer_avisos(rutas: &Rutas) -> BTreeSet<String> {
    let Some(texto) = leer(rutas, "avisos") else {
        return BTreeSet::new();
    };
    let mut ids = BTreeSet::new();
    let mut ignorados = 0_usize;
    for id in texto
        .lines()
        .map(str::trim)
        .filter(|id| id_de_aviso_valido(id))
    {
        if ids.len() < MAXIMO_AVISOS_LEIDOS || ids.contains(id) {
            ids.insert(id.to_string());
        } else {
            ignorados += 1;
        }
    }
    if ignorados > 0 {
        warn!(
            aceptados = ids.len(),
            ignorados,
            "el estado de los avisos de Android trae demasiados ids: se ignoran los que sobran"
        );
    }
    ids
}

/// Guarda los ids de los avisos publicados (uno por línea). Vacío borra el
/// fichero.
pub fn guardar_avisos(rutas: &Rutas, ids: &BTreeSet<String>) -> Result<()> {
    if ids.is_empty() {
        return match std::fs::remove_file(ruta(rutas, "avisos")) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).context("vaciando el estado de los avisos"),
        };
    }
    crear_directorio(rutas)?;
    let mut texto = String::new();
    for id in ids.iter().filter(|id| id_de_aviso_valido(id)) {
        texto.push_str(id);
        texto.push('\n');
    }
    escribir_atomico(&ruta(rutas, "avisos"), texto.as_bytes(), 0o600)
}

/// Lo que dejó el servidor para la barra; `None` si no hay fichero o no se
/// entiende.
#[must_use]
pub fn leer_servidor(rutas: &Rutas) -> Option<EstadoServidorTermux> {
    serde_json::from_str(&leer(rutas, "servidor")?).ok()
}

/// Escribe lo que el servidor quiere decir en la barra (lo reescribe entero).
pub fn escribir_servidor(rutas: &Rutas, estado: &EstadoServidorTermux) -> Result<()> {
    crear_directorio(rutas)?;
    let texto = serde_json::to_string(estado).context("serializando el estado del servidor")?;
    escribir_atomico(&ruta(rutas, "servidor"), texto.as_bytes(), 0o600)
}

/// Borra lo del servidor (al terminar): el siguiente cliente no debe leer lo
/// que dijo uno que ya no está.
pub fn quitar_servidor(rutas: &Rutas) -> Result<()> {
    match std::fs::remove_file(ruta(rutas, "servidor")) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).context("quitando el estado del servidor"),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn rutas_en(base: &std::path::Path) -> Rutas {
        Rutas::nuevas(
            base.join("datos"),
            base.join("config"),
            base.join("estado"),
            base.join("hogar"),
            base.join("run"),
        )
    }

    #[test]
    fn las_marcas_se_ponen_se_leen_y_se_quitan() {
        let base = tempfile::tempdir().unwrap();
        let rutas = rutas_en(base.path());
        assert!(!hay_marca(&rutas, Marca::WakeLock));
        poner_marca(&rutas, Marca::WakeLock).unwrap();
        assert!(hay_marca(&rutas, Marca::WakeLock));
        assert!(!hay_marca(&rutas, Marca::SegundoPlano), "cada una la suya");
        assert!(directorio(&rutas).join("wake-lock").exists());
        quitar_marca(&rutas, Marca::WakeLock).unwrap();
        quitar_marca(&rutas, Marca::WakeLock).unwrap();
        assert!(!hay_marca(&rutas, Marca::WakeLock));
    }

    #[test]
    fn solo_son_ids_de_aviso_los_de_magi() {
        for bueno in [
            "magi-7c1e4a90",
            "magi-p3",
            "magi-p12",
            "magi-abcdefghij012345",
        ] {
            assert!(id_de_aviso_valido(bueno), "{bueno}");
        }
        for malo in [
            "",
            "magi-",
            "magi-p",
            "magi-ABC",
            "magi-a b",
            "magi-a;b",
            "magi-abcdefghij0123456",
            "otro-123",
            "magi-../x",
            "magi-ñ",
        ] {
            assert!(!id_de_aviso_valido(malo), "{malo:?}");
        }
    }

    #[test]
    fn los_avisos_se_guardan_y_se_ignora_lo_que_no_es_un_id() {
        let base = tempfile::tempdir().unwrap();
        let rutas = rutas_en(base.path());
        assert!(leer_avisos(&rutas).is_empty());
        let ids: BTreeSet<String> = ["magi-7c1e4a90", "magi-p3"].map(String::from).into();
        guardar_avisos(&rutas, &ids).unwrap();
        assert_eq!(leer_avisos(&rutas), ids);
        // Un fichero retocado por otro proceso: solo valen los ids de verdad.
        std::fs::write(
            directorio(&rutas).join("avisos"),
            "magi-p3\n$(reboot)\n\nmagi-p9 ; ls\nmagi-ok1\n",
        )
        .unwrap();
        let leidos = leer_avisos(&rutas);
        assert_eq!(
            leidos,
            ["magi-ok1", "magi-p3"]
                .map(String::from)
                .into_iter()
                .collect()
        );
        // Vacío borra el fichero.
        guardar_avisos(&rutas, &BTreeSet::new()).unwrap();
        assert!(!directorio(&rutas).join("avisos").exists());
        guardar_avisos(&rutas, &BTreeSet::new()).unwrap();
    }

    #[test]
    fn el_estado_de_los_avisos_acota_cuantos_ids_se_leen() {
        let base = tempfile::tempdir().unwrap();
        let rutas = rutas_en(base.path());
        std::fs::create_dir_all(directorio(&rutas)).unwrap();
        // Unos 4 000 ids válidos y distintos (caben en los 64 KiB de lectura):
        // sin tope, la purga de arranque lanzaría una orden por cada uno.
        let texto: String = (0..4000).map(|n| format!("magi-p{n}\n")).collect();
        assert!(texto.len() < TOPE_LECTURA as usize);
        std::fs::write(directorio(&rutas).join("avisos"), &texto).unwrap();
        let leidos = leer_avisos(&rutas);
        assert_eq!(leidos.len(), MAXIMO_AVISOS_LEIDOS);
        assert_eq!(
            MAXIMO_AVISOS_LEIDOS,
            4 * super::super::avisos::MAXIMO_AVISOS
        );
        // Los primeros del fichero, no unos cualesquiera.
        for n in 0..MAXIMO_AVISOS_LEIDOS {
            assert!(leidos.contains(&format!("magi-p{n}")), "magi-p{n}");
        }
        // Un id repetido no cuenta dos veces ni expulsa a otro.
        let repetidos: String = (0..MAXIMO_AVISOS_LEIDOS)
            .chain(0..MAXIMO_AVISOS_LEIDOS)
            .map(|n| format!("magi-p{n}\n"))
            .collect();
        std::fs::write(directorio(&rutas).join("avisos"), repetidos).unwrap();
        assert_eq!(leer_avisos(&rutas).len(), MAXIMO_AVISOS_LEIDOS);
        // Con menos ids que el tope, todos.
        std::fs::write(directorio(&rutas).join("avisos"), "magi-p1\nmagi-p2\n").unwrap();
        assert_eq!(leer_avisos(&rutas).len(), 2);
    }

    #[test]
    fn un_fifo_o_un_enlace_no_cuelgan_la_lectura_del_estado() {
        let base = tempfile::tempdir().unwrap();
        let rutas = rutas_en(base.path());
        std::fs::create_dir_all(directorio(&rutas)).unwrap();
        // Un FIFO en el sitio de `servidor`: sin las banderas, `open` esperaría
        // para siempre a que alguien escribiera.
        let fifo = directorio(&rutas).join("servidor");
        let nombre = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
        // SAFETY: `nombre` es una cadena C válida y vive durante la llamada.
        assert_eq!(unsafe { nix::libc::mkfifo(nombre.as_ptr(), 0o600) }, 0);
        assert_eq!(leer_servidor(&rutas), None);
        std::fs::remove_file(&fifo).unwrap();
        // Un enlace a un fichero válido tampoco se sigue.
        let real = base.path().join("real.json");
        std::fs::write(&real, r#"{"arranque":1,"avisos":[]}"#).unwrap();
        std::os::unix::fs::symlink(&real, &fifo).unwrap();
        assert_eq!(leer_servidor(&rutas), None);
    }

    #[test]
    fn el_estado_del_servidor_va_y_vuelve_y_se_borra() {
        let base = tempfile::tempdir().unwrap();
        let rutas = rutas_en(base.path());
        assert_eq!(leer_servidor(&rutas), None);
        let estado = EstadoServidorTermux {
            arranque: 1_700_000_000,
            avisos: vec![AvisoServidor::ApiApagada, AvisoServidor::SinWakeLock],
        };
        escribir_servidor(&rutas, &estado).unwrap();
        assert_eq!(leer_servidor(&rutas), Some(estado));
        let texto = std::fs::read_to_string(directorio(&rutas).join("servidor")).unwrap();
        assert!(texto.contains("api_apagada"), "{texto}");
        use std::os::unix::fs::PermissionsExt as _;
        let modo = std::fs::metadata(directorio(&rutas).join("servidor"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(modo & 0o777, 0o600);
        // Basura o un campo que no conocemos: nada, y sin pánico.
        std::fs::write(directorio(&rutas).join("servidor"), "no es json").unwrap();
        assert_eq!(leer_servidor(&rutas), None);
        std::fs::write(
            directorio(&rutas).join("servidor"),
            r#"{"arranque":5,"avisos":["limite_procesos","algo_nuevo"]}"#,
        )
        .unwrap();
        assert_eq!(leer_servidor(&rutas), None, "un aviso desconocido invalida");
        quitar_servidor(&rutas).unwrap();
        quitar_servidor(&rutas).unwrap();
    }
}
