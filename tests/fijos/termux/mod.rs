//! Dobles de las órdenes de Termux (`termux-*`) para las pruebas (Fase 17).
//!
//! Cada doble es el mismo script de `sh` (`falsos/doble`) instalado con el
//! nombre de la orden: anota sus argumentos y su entrada en ficheros y sabe
//! hacer bien, fallar, colgarse, ir despacio o devolver mucha salida según lo
//! que se programe. Se incluye con `#[path]` desde los tests de `magi-ssh`,
//! como los dobles de servicios:
//!
//! ```ignore
//! #[path = "fijos/termux/mod.rs"]
//! mod dobles;                       // desde tests/ del crate raíz
//! ```
//!
//! El script lleva el directorio de estado de cada juego escrito dentro (la
//! sustitución de `@DIR@` al instalarlo), no en una variable de entorno: las
//! pruebas de un mismo proceso corren en paralelo y el entorno es de todas.
//!
//! Los dobles viven en `<dir>/usr/bin` (el `$PREFIX/bin` de un Termux de
//! mentira, con un `sh` que apunta al del sistema) y su estado en
//! `<dir>/estado`. Para el script: `shellcheck tests/fijos/termux/falsos/*`.

// Cada prueba que incluye este módulo usa una parte: lo demás es normal que
// quede sin usar en ella.
#![allow(dead_code)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use magi::termux::EntornoTermux;

/// Las órdenes de Termux que hay doble de ellas.
pub const ORDENES: [&str; 8] = [
    "termux-clipboard-set",
    "termux-open-url",
    "termux-wake-lock",
    "termux-wake-unlock",
    "termux-notification",
    "termux-notification-remove",
    "termux-reload-settings",
    "termux-battery-status",
];

const GUION: &str = include_str!("falsos/doble");

/// Lo que hace un doble cuando se le llama.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modo {
    /// Termina bien (`termux-battery-status` imprime un JSON).
    Ok,
    /// Mensaje en stderr y código 1.
    Fallar,
    /// stderr con caracteres de control y 700 caracteres; código 2.
    ErrorLargo,
    /// No lee la entrada, deja un proceso hijo vivo y duerme 120 s.
    Colgar,
    /// Espera esas décimas de segundo y luego, como `Ok`.
    Lento(u32),
    /// 1 MiB por stdout y otro por stderr; código 0.
    MuchaSalida,
    /// Como `MuchaSalida`, con código 1.
    MuchaSalidaFallo,
}

impl Modo {
    fn linea(self) -> String {
        match self {
            Self::Ok => "ok".into(),
            Self::Fallar => "fallar".into(),
            Self::ErrorLargo => "error_largo".into(),
            Self::Colgar => "colgar".into(),
            Self::Lento(decimas) => format!("lento {decimas}"),
            Self::MuchaSalida => "mucha_salida".into(),
            Self::MuchaSalidaFallo => "mucha_salida_fallo".into(),
        }
    }
}

/// Una llamada que recibió un doble.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Llamada {
    /// Los argumentos, sin el nombre de la orden.
    pub args: Vec<String>,
    /// Lo que llegó por la entrada estándar.
    pub entrada: Vec<u8>,
    /// Los pids que dejó anotados (solo en el modo `Colgar`: el del doble y el
    /// de su hijo).
    pub pids: Vec<i32>,
}

/// Un juego de dobles instalado en un directorio.
pub struct Dobles {
    raiz: PathBuf,
}

impl Dobles {
    /// Instala los dobles en `dir` (que debe existir y ser de la prueba:
    /// normalmente, un `tempfile::tempdir()`).
    pub fn nuevos(dir: &Path) -> Self {
        let estado = dir.join("estado");
        assert!(
            !estado.to_string_lossy().contains('\''),
            "el directorio de los dobles no puede llevar comillas simples"
        );
        let bin = dir.join("usr").join("bin");
        for subdir in [
            bin.clone(),
            dir.join("usr").join("tmp"),
            estado.join("llamadas"),
            estado.join("modos"),
        ] {
            std::fs::create_dir_all(&subdir).expect("directorio de los dobles");
        }
        let guion = GUION.replace("@DIR@", &estado.to_string_lossy());
        for orden in ORDENES {
            instalar(&bin.join(orden), guion.as_bytes());
        }
        // El `$PREFIX/bin/sh` de este Termux de mentira.
        let _ = std::os::unix::fs::symlink("/bin/sh", bin.join("sh"));
        Self {
            raiz: dir.to_path_buf(),
        }
    }

    /// El `$PREFIX` de los dobles.
    pub fn prefix(&self) -> PathBuf {
        self.raiz.join("usr")
    }

    /// El directorio de los dobles (`$PREFIX/bin`).
    pub fn bin(&self) -> PathBuf {
        self.prefix().join("bin")
    }

    fn estado(&self) -> PathBuf {
        self.raiz.join("estado")
    }

    /// Programa lo que hará el doble `programa` desde ahora.
    pub fn programar(&self, programa: &str, modo: Modo) {
        assert!(ORDENES.contains(&programa), "no hay doble de {programa}");
        std::fs::write(
            self.estado().join("modos").join(programa),
            format!("{}\n", modo.linea()),
        )
        .expect("programar un doble");
    }

    /// Quita un doble: esa orden deja de estar en el `PATH`.
    pub fn quitar(&self, programa: &str) {
        std::fs::remove_file(self.bin().join(programa)).expect("quitar un doble");
    }

    /// Las llamadas que ha recibido `programa`, en orden.
    pub fn llamadas(&self, programa: &str) -> Vec<Llamada> {
        let prefijo = format!("{programa}.");
        let mut numeradas: Vec<(u64, PathBuf)> = std::fs::read_dir(self.estado().join("llamadas"))
            .expect("llamadas de los dobles")
            .filter_map(Result::ok)
            .filter_map(|entrada| {
                let nombre = entrada.file_name().to_string_lossy().into_owned();
                let numero = nombre.strip_prefix(&prefijo)?.parse::<u64>().ok()?;
                Some((numero, entrada.path()))
            })
            .collect();
        numeradas.sort();
        numeradas
            .into_iter()
            // Una llamada a medio anotar (sin `args` todavía) no cuenta aún.
            .filter(|(_, dir)| dir.join("args").exists())
            .map(|(_, dir)| {
                let args = std::fs::read(dir.join("args")).unwrap_or_default();
                let args = if args.is_empty() {
                    Vec::new()
                } else {
                    // Cada argumento termina en NUL: el último trozo, vacío,
                    // no es un argumento.
                    let mut trozos: Vec<&[u8]> = args.split(|byte| *byte == 0).collect();
                    trozos.pop();
                    trozos
                        .into_iter()
                        .map(|trozo| String::from_utf8_lossy(trozo).into_owned())
                        .collect()
                };
                let pids = ["pid", "hijo"]
                    .iter()
                    .filter_map(|nombre| std::fs::read_to_string(dir.join(nombre)).ok())
                    .filter_map(|texto| texto.trim().parse::<i32>().ok())
                    .collect();
                Llamada {
                    args,
                    entrada: std::fs::read(dir.join("entrada")).unwrap_or_default(),
                    pids,
                }
            })
            .collect()
    }

    /// El `PATH` de este Termux de mentira: solo los dobles.
    pub fn path(&self) -> OsString {
        self.bin().into_os_string()
    }

    /// Un entorno de Termux listo: `HOME` (`dir_home`), `PREFIX` y `TMPDIR`
    /// bajo los dobles y `PATH` apuntando a ellos. Sin `XDG_*`.
    pub fn entorno(&self, dir_home: &Path) -> EntornoTermux {
        std::fs::create_dir_all(dir_home).expect("HOME de la prueba");
        EntornoTermux {
            home: Some(dir_home.to_path_buf()),
            prefix: Some(self.prefix()),
            tmpdir: Some(self.prefix().join("tmp")),
            path: Some(self.path()),
            uid: nix::unistd::Uid::current().as_raw(),
            ..EntornoTermux::default()
        }
    }
}

fn instalar(destino: &Path, contenido: &[u8]) {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::write(destino, contenido).expect("escribir un doble");
    std::fs::set_permissions(destino, std::fs::Permissions::from_mode(0o755))
        .expect("permisos de un doble");
}

/// ¿Sigue vivo el proceso `pid`? Un zombi no cuenta (nadie lo ha recogido
/// todavía, pero ya no corre): se mira el estado en `/proc` y, donde no hay
/// `/proc`, con `kill(pid, 0)`.
pub fn proceso_vivo(pid: i32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        // «pid (nombre) E ...»: el estado va tras el último paréntesis.
        Ok(texto) => texto
            .rsplit_once(')')
            .and_then(|(_, resto)| resto.trim_start().chars().next())
            .is_some_and(|estado| !matches!(estado, 'Z' | 'X')),
        Err(_) if Path::new("/proc/self").exists() => false,
        Err(_) => nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok(),
    }
}

/// ¿Sigue vivo `pid` pasado un rato? Matar es asíncrono (la señal se entrega
/// cuando el proceso vuelve a ejecutarse): con la máquina cargada, un proceso
/// al que se acaba de mandar SIGKILL puede verse un instante todavía vivo.
/// Espera hasta `espera` a que muera y dice si aún sigue.
pub fn sigue_vivo_tras(pid: i32, espera: std::time::Duration) -> bool {
    let limite = std::time::Instant::now() + espera;
    while proceso_vivo(pid) {
        if std::time::Instant::now() >= limite {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    false
}
