//! Rutas de Termux: siempre del entorno, nunca del uid (D383). bionic no sabe
//! dónde está el home de Termux.
//!
//! `EntornoTermux::del_proceso` es, junto a `es_termux`, el único sitio de este
//! módulo que lee `std::env`; `calcular` es pura y se prueba con un entorno de
//! mentira.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Lo que de verdad hace falta del entorno del proceso, inyectado.
#[derive(Debug, Clone, Default)]
pub struct EntornoTermux {
    pub home: Option<PathBuf>,
    /// `$PREFIX` (`/data/data/com.termux/files/usr`).
    pub prefix: Option<PathBuf>,
    pub tmpdir: Option<PathBuf>,
    pub xdg_data_home: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
    pub xdg_state_home: Option<PathBuf>,
    pub xdg_runtime_dir: Option<PathBuf>,
    /// `$PATH`, para buscar las órdenes `termux-*`.
    pub path: Option<OsString>,
    /// El uid del proceso: solo sirve para dar nombre al directorio del
    /// socket (`magi-<uid>`), nunca para localizar el home.
    pub uid: u32,
}

impl EntornoTermux {
    /// El entorno de este proceso. Es el único sitio que lee `std::env`.
    /// Una variable vacía cuenta como no puesta.
    pub fn del_proceso() -> Self {
        let ruta = |nombre: &str| {
            std::env::var_os(nombre)
                .filter(|valor| !valor.is_empty())
                .map(PathBuf::from)
        };
        Self {
            home: ruta("HOME"),
            prefix: ruta("PREFIX"),
            tmpdir: ruta("TMPDIR"),
            xdg_data_home: ruta("XDG_DATA_HOME"),
            xdg_config_home: ruta("XDG_CONFIG_HOME"),
            xdg_state_home: ruta("XDG_STATE_HOME"),
            xdg_runtime_dir: ruta("XDG_RUNTIME_DIR"),
            path: std::env::var_os("PATH").filter(|valor| !valor.is_empty()),
            uid: nix::unistd::Uid::current().as_raw(),
        }
    }
}

/// Rutas calculadas para Termux.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RutasTermux {
    pub datos: PathBuf,
    pub config: PathBuf,
    pub estado: PathBuf,
    pub hogar: PathBuf,
    /// Directorio del socket (700, ruta del socket de menos de 100 bytes).
    pub runtime: PathBuf,
    /// `$PREFIX/bin/sh`: en Termux no existe `/bin/sh`.
    pub sh: PathBuf,
}

/// Exige una variable de ruta absoluta; si falta o no sirve, el error dice
/// qué hacer.
fn exigir_absoluta(valor: Option<&PathBuf>, nombre: &str) -> Result<PathBuf, String> {
    match valor {
        None => Err(format!(
            "MAGI en Termux necesita la variable {nombre} y no está puesta: \
             abre MAGI desde una sesión normal de Termux (no desde un entorno \
             limpio con `env -i`) o exporta {nombre} antes de ejecutarlo"
        )),
        Some(ruta) if !ruta.is_absolute() => Err(format!(
            "MAGI en Termux necesita que {nombre} sea una ruta absoluta y vale «{}»: \
             corrige la variable antes de ejecutar MAGI",
            ruta.display()
        )),
        Some(ruta) => Ok(ruta.clone()),
    }
}

/// Una ruta XDG: la del entorno si es absoluta (como manda la especificación:
/// una relativa se ignora) o, si no, la de por defecto bajo `$HOME`.
fn base_xdg(valor: &Option<PathBuf>, por_defecto: &Path) -> PathBuf {
    match valor {
        Some(ruta) if ruta.is_absolute() => ruta.clone(),
        _ => por_defecto.to_path_buf(),
    }
}

/// Calcula las rutas (§7.1): datos, configuración y estado de `XDG_*` o del
/// `$HOME`; el socket en `$XDG_RUNTIME_DIR/magi` o, si falta,
/// `$TMPDIR/magi-<uid>` (con `$PREFIX/tmp` si no hay `$TMPDIR`). Error con
/// instrucción si falta `$HOME` o `$PREFIX`.
pub fn calcular(entorno: &EntornoTermux) -> Result<RutasTermux, String> {
    let home = exigir_absoluta(entorno.home.as_ref(), "HOME")?;
    let prefix = exigir_absoluta(entorno.prefix.as_ref(), "PREFIX")?;

    let datos = base_xdg(&entorno.xdg_data_home, &home.join(".local/share")).join("magi");
    let config = base_xdg(&entorno.xdg_config_home, &home.join(".config")).join("magi");
    let estado = base_xdg(&entorno.xdg_state_home, &home.join(".local/state")).join("magi");

    // Termux no pone XDG_RUNTIME_DIR y no tiene /tmp: el socket va en el
    // directorio temporal de Termux, con el uid en el nombre para que dos
    // usuarios (o dos perfiles) no se pisen.
    let runtime = match entorno
        .xdg_runtime_dir
        .as_ref()
        .filter(|ruta| ruta.is_absolute())
    {
        Some(ruta) => ruta.join("magi"),
        None => {
            let tmp = entorno
                .tmpdir
                .as_ref()
                .filter(|ruta| ruta.is_absolute())
                .cloned()
                .unwrap_or_else(|| prefix.join("tmp"));
            tmp.join(format!("magi-{}", entorno.uid))
        }
    };

    Ok(RutasTermux {
        datos,
        config,
        estado,
        hogar: home,
        runtime,
        sh: prefix.join("bin").join("sh"),
    })
}

#[cfg(test)]
mod pruebas {
    use super::*;

    const HOME: &str = "/data/data/com.termux/files/home";
    const PREFIX: &str = "/data/data/com.termux/files/usr";

    fn entorno() -> EntornoTermux {
        EntornoTermux {
            home: Some(HOME.into()),
            prefix: Some(PREFIX.into()),
            tmpdir: Some(format!("{PREFIX}/tmp").into()),
            uid: 10123,
            ..EntornoTermux::default()
        }
    }

    #[test]
    fn sin_xdg_todo_cuelga_del_home_y_el_socket_del_tmpdir() {
        let rutas = calcular(&entorno()).unwrap();
        assert_eq!(rutas.datos, Path::new(HOME).join(".local/share/magi"));
        assert_eq!(rutas.config, Path::new(HOME).join(".config/magi"));
        assert_eq!(rutas.estado, Path::new(HOME).join(".local/state/magi"));
        assert_eq!(rutas.hogar, Path::new(HOME));
        assert_eq!(rutas.runtime, Path::new(PREFIX).join("tmp/magi-10123"));
        assert_eq!(rutas.sh, Path::new(PREFIX).join("bin/sh"));
    }

    #[test]
    fn el_socket_de_termux_cabe_de_sobra() {
        let rutas = calcular(&entorno()).unwrap();
        let socket = rutas.runtime.join("servidor.sock");
        assert!(
            socket.as_os_str().len() < 100,
            "{} bytes: {}",
            socket.as_os_str().len(),
            socket.display()
        );
    }

    #[test]
    fn xdg_runtime_dir_se_respeta_y_cambia_el_socket() {
        let mut entorno = entorno();
        entorno.xdg_runtime_dir = Some("/run/usuario".into());
        let rutas = calcular(&entorno).unwrap();
        assert_eq!(rutas.runtime, Path::new("/run/usuario/magi"));
    }

    #[test]
    fn sin_tmpdir_el_socket_va_en_el_tmp_del_prefijo() {
        let mut entorno = entorno();
        entorno.tmpdir = None;
        let rutas = calcular(&entorno).unwrap();
        assert_eq!(rutas.runtime, Path::new(PREFIX).join("tmp/magi-10123"));
    }

    #[test]
    fn una_ruta_xdg_relativa_se_ignora() {
        let mut entorno = entorno();
        entorno.xdg_data_home = Some("datos".into());
        entorno.xdg_runtime_dir = Some("run".into());
        let rutas = calcular(&entorno).unwrap();
        assert_eq!(rutas.datos, Path::new(HOME).join(".local/share/magi"));
        assert_eq!(rutas.runtime, Path::new(PREFIX).join("tmp/magi-10123"));
    }

    #[test]
    fn falta_home_o_prefix_da_una_instruccion() {
        let mut sin_home = entorno();
        sin_home.home = None;
        let error = calcular(&sin_home).unwrap_err();
        assert!(
            error.contains("HOME") && error.contains("exporta"),
            "{error}"
        );

        let mut sin_prefix = entorno();
        sin_prefix.prefix = None;
        let error = calcular(&sin_prefix).unwrap_err();
        assert!(error.contains("PREFIX"), "{error}");

        let mut relativo = entorno();
        relativo.home = Some("casa".into());
        assert!(calcular(&relativo).unwrap_err().contains("absoluta"));
    }
}
