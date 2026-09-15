//! Aviso de ficheros sensibles (`[archivos] avisar`): globs que se comprueban
//! **solo en las subidas**, contra el nombre de cada fichero que se va a
//! transferir, incluidos los de dentro de los directorios.
//!
//! Una lista vacía desactiva el aviso; la barra de la vista lo indica al
//! abrir Archivos, porque desactivarlo sin decirlo sería una trampa.

use globset::{Glob, GlobSet, GlobSetBuilder};

/// Patrones de aviso por defecto: nada de subir secretos sin enterarse.
pub const AVISO_POR_DEFECTO: [&str; 4] = [".env*", "*.pem", "*.key", "id_*"];

/// Cuántas coincidencias enseña el diálogo antes del «+N».
pub const MAXIMO_EN_EL_AVISO: usize = 5;

/// Ficheros que coinciden con los patrones de aviso, con su tamaño.
pub struct Sensibles {
    conjunto: GlobSet,
    patrones: Vec<String>,
}

impl Sensibles {
    /// Compila los patrones. Un glob inválido se descarta con un aviso: una
    /// errata en `config.toml` no puede impedir arrancar.
    pub fn nuevo(patrones: &[String]) -> Self {
        let mut constructor = GlobSetBuilder::new();
        let mut validos = Vec::new();
        for patron in patrones {
            let recortado = patron.trim();
            if recortado.is_empty() {
                continue;
            }
            match Glob::new(recortado) {
                Ok(glob) => {
                    constructor.add(glob);
                    validos.push(recortado.to_string());
                }
                Err(error) => {
                    tracing::warn!("patrón de aviso «{recortado}» descartado: {error}");
                }
            }
        }
        let conjunto = constructor.build().unwrap_or_else(|_| GlobSet::empty());
        Self {
            conjunto,
            patrones: validos,
        }
    }

    /// Sin patrones válidos no hay aviso, y la vista lo dice en la barra.
    pub fn vacia(&self) -> bool {
        self.patrones.is_empty()
    }

    pub fn patrones(&self) -> &[String] {
        &self.patrones
    }

    /// El patrón que hace saltar el aviso para un nombre, si lo hay.
    pub fn coincide(&self, nombre: &str) -> Option<&str> {
        self.conjunto
            .matches(nombre)
            .first()
            .map(|indice| self.patrones[*indice].as_str())
    }

    /// Primeras coincidencias (hasta `MAXIMO_EN_EL_AVISO`) y cuántas quedaron
    /// fuera, para el «+N» del diálogo.
    pub fn coincidencias<'a>(
        &self,
        ficheros: impl Iterator<Item = (&'a str, u64)>,
    ) -> (Vec<(String, u64)>, usize) {
        let mut encontrados = Vec::new();
        let mut restantes = 0usize;
        for (nombre, bytes) in ficheros {
            if self.coincide(nombre).is_none() {
                continue;
            }
            if encontrados.len() < MAXIMO_EN_EL_AVISO {
                encontrados.push((nombre.to_string(), bytes));
            } else {
                restantes += 1;
            }
        }
        (encontrados, restantes)
    }
}

impl Default for Sensibles {
    fn default() -> Self {
        Self::nuevo(
            &AVISO_POR_DEFECTO
                .iter()
                .map(|patron| patron.to_string())
                .collect::<Vec<_>>(),
        )
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn por_defecto() -> Sensibles {
        Sensibles::default()
    }

    #[test]
    fn los_globs_por_defecto_detectan_env_pem_key_e_id() {
        let sensibles = por_defecto();
        assert_eq!(sensibles.coincide(".env"), Some(".env*"));
        assert_eq!(sensibles.coincide(".env.local"), Some(".env*"));
        assert_eq!(sensibles.coincide("clave.pem"), Some("*.pem"));
        assert_eq!(sensibles.coincide("server.key"), Some("*.key"));
        assert_eq!(sensibles.coincide("id_ed25519"), Some("id_*"));
        assert_eq!(sensibles.coincide("main.py"), None);
        assert!(!sensibles.vacia());
    }

    #[test]
    fn un_fichero_dentro_de_un_directorio_tambien_avisa() {
        // El cliente recorre los directorios antes de encolar y comprueba el
        // nombre de cada fichero, no la ruta.
        let sensibles = por_defecto();
        let ficheros = [
            ("static/app.css", 10u64),
            (".env", 1024),
            ("config.yaml", 2048),
        ];
        let (encontrados, restantes) = sensibles.coincidencias(
            ficheros
                .iter()
                .map(|(ruta, bytes)| (ruta.rsplit('/').next().unwrap(), *bytes)),
        );
        assert_eq!(encontrados, vec![(".env".to_string(), 1024)]);
        assert_eq!(restantes, 0);
    }

    #[test]
    fn el_aviso_ensena_cinco_coincidencias_y_el_resto_como_mas_ene() {
        let sensibles = por_defecto();
        let ficheros: Vec<(String, u64)> = (0..8)
            .map(|indice| (format!("clave{indice}.pem"), 10))
            .collect();
        let (encontrados, restantes) = sensibles.coincidencias(
            ficheros
                .iter()
                .map(|(nombre, bytes)| (nombre.as_str(), *bytes)),
        );
        assert_eq!(encontrados.len(), MAXIMO_EN_EL_AVISO);
        assert_eq!(restantes, 3);
    }

    #[test]
    fn la_lista_vacia_nunca_avisa() {
        let sensibles = Sensibles::nuevo(&[]);
        assert!(sensibles.vacia());
        assert_eq!(sensibles.coincide(".env"), None);
    }

    #[test]
    fn un_glob_invalido_no_rompe_la_lista() {
        let sensibles = Sensibles::nuevo(&["[".to_string(), "*.pem".to_string()]);
        assert_eq!(sensibles.patrones(), ["*.pem"]);
        assert_eq!(sensibles.coincide("clave.pem"), Some("*.pem"));
    }

    #[test]
    fn los_patrones_distinguen_mayusculas() {
        let sensibles = por_defecto();
        assert_eq!(sensibles.coincide("ID_RSA"), None);
    }
}
