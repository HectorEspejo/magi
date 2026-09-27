use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::ficheros::escribir_atomico;

/// Rutas XDG de MAGI (equivalentes en macOS vía `directories`).
#[derive(Debug, Clone)]
pub struct Rutas {
    pub datos: PathBuf,
    pub config: PathBuf,
    pub estado: PathBuf,
    pub hogar: PathBuf,
    /// Directorio de ejecución del servidor (`$XDG_RUNTIME_DIR/magi/` en
    /// Linux, `~/Library/Caches/magi/` en macOS); inyectable para pruebas.
    pub runtime: PathBuf,
}

impl Rutas {
    pub fn descubrir() -> Result<Self> {
        let dirs = directories::ProjectDirs::from("org", "4d3", "magi")
            .context("no se pudo determinar el directorio de datos del usuario")?;
        let hogar = directories::BaseDirs::new()
            .context("no se pudo determinar el directorio personal")?
            .home_dir()
            .to_path_buf();
        Ok(Self {
            datos: dirs.data_dir().to_path_buf(),
            config: dirs.config_dir().to_path_buf(),
            estado: dirs
                .state_dir()
                .unwrap_or_else(|| dirs.data_dir())
                .to_path_buf(),
            hogar,
            runtime: runtime_del_usuario(),
        })
    }

    pub fn base_datos(&self) -> PathBuf {
        self.datos.join("magi.db")
    }

    pub fn fichero_config(&self) -> PathBuf {
        self.config.join("config.toml")
    }

    pub fn dir_logs(&self) -> PathBuf {
        self.estado.join("logs")
    }

    pub fn dir_ssh(&self) -> PathBuf {
        self.hogar.join(".ssh")
    }

    pub fn fichero_ssh_config(&self) -> PathBuf {
        self.dir_ssh().join("config")
    }

    pub fn fichero_magi_config(&self) -> PathBuf {
        self.dir_ssh().join("magi_config")
    }

    pub fn fichero_known_hosts(&self) -> PathBuf {
        self.dir_ssh().join("known_hosts")
    }

    /// Directorio de ejecución del servidor: `$XDG_RUNTIME_DIR/magi/`
    /// (fallback `/tmp/magi-<uid>/` en Linux) o `~/Library/Caches/magi/`
    /// en macOS. Debe crearse con permisos 700.
    pub fn dir_runtime(&self) -> PathBuf {
        self.runtime.clone()
    }
}

/// Directorio de ejecución para el proceso real, según el sistema.
fn runtime_del_usuario() -> PathBuf {
    if cfg!(target_os = "macos") {
        let dirs = directories::ProjectDirs::from("org", "4d3", "magi");
        if let Some(dirs) = dirs {
            return dirs.cache_dir().join("magi");
        }
        return std::env::temp_dir().join("magi");
    }
    if let Ok(ruta) = std::env::var("XDG_RUNTIME_DIR") {
        if !ruta.is_empty() {
            return PathBuf::from(ruta).join("magi");
        }
    }
    let uid = nix::unistd::Uid::current().as_raw();
    PathBuf::from("/tmp").join(format!("magi-{uid}"))
}

pub const PREFIJO_POR_DEFECTO: &str = "Ctrl+]";

/// Sección `[flota]` de `config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SeccionFlota {
    /// Segundos entre auto-refrescos; 0 = desactivado y mínimo 15.
    pub auto_refresco_seg: u64,
    pub umbrales: crate::flota::estado::Umbrales,
    /// `[flota.atajos]`: tecla (una letra) → nombre de snippet, sobre el host
    /// seleccionado en Flota.
    pub atajos: std::collections::BTreeMap<String, String>,
}

/// Teclas que ya usa la vista Flota: un atajo no puede quitárselas.
pub const TECLAS_DE_FLOTA: &[char] = &['s', 'j', 'k', 'r', 'R', '/', 'e', 'a', '?', 'q', '!'];

/// Valida `[flota.atajos]` contra las teclas de Flota y los snippets que
/// existen. Devuelve los atajos válidos (tecla, snippet) y un aviso por cada
/// uno que se ignora.
pub fn validar_atajos(
    atajos: &std::collections::BTreeMap<String, String>,
    snippets: &[String],
) -> (Vec<(char, String)>, Vec<String>) {
    let mut validos = Vec::new();
    let mut avisos = Vec::new();
    for (tecla, snippet) in atajos {
        let mut caracteres = tecla.chars();
        let (Some(caracter), None) = (caracteres.next(), caracteres.next()) else {
            avisos.push(format!("atajo «{tecla}» ignorado: debe ser una sola tecla"));
            continue;
        };
        if caracter.is_control() || caracter.is_whitespace() {
            avisos.push(format!("atajo «{tecla}» ignorado: no es una tecla válida"));
            continue;
        }
        if TECLAS_DE_FLOTA.contains(&caracter) {
            avisos.push(format!("atajo «{tecla}» ignorado: Flota ya usa esa tecla"));
            continue;
        }
        if !snippets.iter().any(|nombre| nombre == snippet) {
            avisos.push(format!(
                "atajo «{tecla}» ignorado: no existe el snippet «{snippet}»"
            ));
            continue;
        }
        validos.push((caracter, snippet.clone()));
    }
    (validos, avisos)
}

/// Sección `[deliberacion]` de `config.toml` (deliberación MAGI, Fase 6).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SeccionDeliberacion {
    /// BALTHASAR-2: antigüedad máxima del último backup.
    pub backup_horas: u64,
    /// MELCHIOR-1: antigüedad máxima del último sondeo antes de sondear.
    pub salud_max_min: u64,
    /// Plazo de cada comprobación, medido desde el inicio.
    pub limite_seg: u64,
    /// Caracteres mínimos del motivo de un forzado.
    pub motivo_min: usize,
}

impl Default for SeccionDeliberacion {
    fn default() -> Self {
        Self {
            backup_horas: 24,
            salud_max_min: 5,
            limite_seg: 2,
            motivo_min: 10,
        }
    }
}

/// Sección `[terminal]` de `config.toml`: comando para lanzar una ventana
/// nueva con MAGI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SeccionTerminal {
    /// Ejemplo: «alacritty -e magi» (Linux) o «open -a Terminal magi» (macOS).
    pub comando: String,
}

impl Default for SeccionTerminal {
    fn default() -> Self {
        Self {
            comando: comando_terminal_por_defecto().to_string(),
        }
    }
}

fn comando_terminal_por_defecto() -> &'static str {
    if cfg!(target_os = "macos") {
        "open -a Terminal magi"
    } else {
        "alacritty -e magi"
    }
}

/// Sección `[archivos]` de `config.toml` (vista Archivos, F4).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SeccionArchivos {
    /// Globs que disparan el aviso antes de subir; vacío desactiva el aviso
    /// (la barra lo indica al abrir Archivos).
    pub avisar: Vec<String>,
    /// Mostrar los ficheros que empiezan por punto al abrir un panel.
    pub mostrar_ocultos: bool,
    /// Visor de ficheros; vacío usa `$PAGER` y, si no hay, `less`.
    pub pager: String,
}

impl Default for SeccionArchivos {
    fn default() -> Self {
        Self {
            avisar: crate::archivos::sensibles::AVISO_POR_DEFECTO
                .iter()
                .map(|patron| patron.to_string())
                .collect(),
            mostrar_ocultos: false,
            pager: String::new(),
        }
    }
}

/// Visor de ficheros de la vista Archivos: `[archivos] pager`, luego `$PAGER`
/// y, si no hay ninguno, `less`.
pub fn pager_por_defecto(seccion: &SeccionArchivos) -> String {
    if !seccion.pager.trim().is_empty() {
        return seccion.pager.trim().to_string();
    }
    match std::env::var("PAGER") {
        Ok(valor) if !valor.trim().is_empty() => valor.trim().to_string(),
        _ => "less".to_string(),
    }
}

/// Sección `[servidor]` de `config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SeccionServidor {
    /// Segundos sin clientes ni sesiones antes de que el servidor se apague.
    pub gracia_apagado_seg: u64,
}

impl Default for SeccionServidor {
    fn default() -> Self {
        Self {
            gracia_apagado_seg: 10,
        }
    }
}

/// Contenido de `~/.config/magi/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Prefijo de escape de la vista Sesión, p. ej. «Ctrl+]».
    pub prefijo_escape: String,
    /// Regenerar `magi_config` al guardar una ficha.
    pub exportar_al_guardar: bool,
    /// `auto`, `fijo` o la ruta a un tema de Alacritty.
    pub tema: String,
    /// Forzar el modo degradado ASCII.
    pub terminal_ascii: bool,
    /// Sondeo de flota: auto-refresco y umbrales.
    pub flota: SeccionFlota,
    /// Ventana nueva de terminal: `[terminal] comando`.
    pub terminal: SeccionTerminal,
    /// Servidor de sesiones: `[servidor] gracia_apagado_seg`.
    pub servidor: SeccionServidor,
    /// Vista Archivos: `[archivos] avisar`, `mostrar_ocultos`, `pager`.
    pub archivos: SeccionArchivos,
    /// Deliberación MAGI: `[deliberacion]` plazos y mínimos.
    pub deliberacion: SeccionDeliberacion,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            prefijo_escape: PREFIJO_POR_DEFECTO.to_string(),
            exportar_al_guardar: true,
            tema: "auto".to_string(),
            terminal_ascii: false,
            flota: SeccionFlota::default(),
            terminal: SeccionTerminal::default(),
            servidor: SeccionServidor::default(),
            archivos: SeccionArchivos::default(),
            deliberacion: SeccionDeliberacion::default(),
        }
    }
}

impl Config {
    /// Carga la configuración; si no existe la crea con los valores por
    /// defecto. Devuelve un aviso legible si el fichero no se pudo leer.
    pub fn cargar(ruta: &Path) -> (Self, Option<String>) {
        match fs::read_to_string(ruta) {
            Ok(texto) => match toml::from_str::<Config>(&texto) {
                Ok(config) => (config, None),
                Err(error) => (
                    Config::default(),
                    Some(format!("config.toml no válido: {error}")),
                ),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let config = Config::default();
                let aviso = match config.guardar(ruta) {
                    Ok(()) => None,
                    Err(error) => Some(format!("no se pudo crear config.toml: {error}")),
                };
                (config, aviso)
            }
            Err(error) => (
                Config::default(),
                Some(format!("no se pudo leer config.toml: {error}")),
            ),
        }
    }

    pub fn guardar(&self, ruta: &Path) -> Result<()> {
        let texto = toml::to_string_pretty(self).context("serializando config.toml")?;
        escribir_atomico(ruta, texto.as_bytes(), 0o600)
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn las_secciones_de_la_fase_6_se_leen_y_tienen_defectos() {
        let config: Config = toml::from_str(
            r#"
[flota.atajos]
u = "uptime"
N = "reiniciar nginx"

[deliberacion]
backup_horas = 48
"#,
        )
        .unwrap();
        assert_eq!(
            config.flota.atajos.get("u").map(String::as_str),
            Some("uptime")
        );
        assert_eq!(config.deliberacion.backup_horas, 48);
        assert_eq!(config.deliberacion.salud_max_min, 5);
        assert_eq!(config.deliberacion.limite_seg, 2);
        assert_eq!(config.deliberacion.motivo_min, 10);
        let vacia = Config::default();
        assert!(vacia.flota.atajos.is_empty());
    }

    #[test]
    fn los_atajos_invalidos_se_ignoran_con_aviso() {
        let atajos: std::collections::BTreeMap<String, String> = [
            ("u", "uptime"),
            ("r", "uptime"),
            ("xy", "uptime"),
            ("N", "reiniciar nginx"),
            ("z", "no existe"),
        ]
        .into_iter()
        .map(|(tecla, snippet)| (tecla.to_string(), snippet.to_string()))
        .collect();
        let (validos, avisos) = validar_atajos(
            &atajos,
            &["uptime".to_string(), "reiniciar nginx".to_string()],
        );
        assert_eq!(
            validos,
            vec![
                ('N', "reiniciar nginx".to_string()),
                ('u', "uptime".to_string())
            ]
        );
        assert_eq!(avisos.len(), 3, "{avisos:?}");
        assert!(avisos.iter().any(|aviso| aviso.contains("Flota ya usa")));
        assert!(avisos.iter().any(|aviso| aviso.contains("una sola tecla")));
        assert!(avisos.iter().any(|aviso| aviso.contains("no existe")));
    }
}
