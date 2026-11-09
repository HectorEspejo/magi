//! Portapapeles del cliente (Fase 15): copiar texto y peticiones del remoto.
//!
//! Copiar usa `wl-copy`/`pbcopy` (F2) y, si no hay o MAGI corre por SSH,
//! OSC 52 hacia el terminal exterior. El remoto (OSC 52) solo escribe en el
//! portapapeles con permiso por host y siempre con aviso; sin permiso se
//! guarda solo su última petición, 60 s, hasta que `prefijo P` la permite.

use std::time::{Duration, Instant};

/// Segundos que se guarda una petición del remoto sin permiso.
pub const PENDIENTE: Duration = Duration::from_secs(60);

/// De dónde salió la copia.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origen {
    /// Se copió con `wl-copy` o `pbcopy`.
    Sistema(&'static str),
    /// Se pidió al terminal exterior con OSC 52.
    Osc52,
}

/// Una petición de copia del remoto (OSC 52) sin permiso.
#[derive(Debug, Clone)]
pub struct Pendiente {
    pub sesion_id: u32,
    pub host_nombre: String,
    pub bytes: u64,
    pub desde: Instant,
}

impl Pendiente {
    /// ¿Ha caducado?
    #[must_use]
    pub fn caducada(&self, ahora: Instant) -> bool {
        ahora.duration_since(self.desde) > PENDIENTE
    }
}

/// Estado del portapapeles del remoto en esta ventana.
#[derive(Debug, Clone, Default)]
pub struct EstadoPortapapelesRemoto {
    /// Solo hay una petición pendiente: la última.
    pub pendiente: Option<Pendiente>,
    /// Copias del remoto entregadas en esta sesión (para las pruebas).
    pub copias: u64,
}

impl EstadoPortapapelesRemoto {
    /// Guarda una petición sin permiso (sustituye la anterior).
    pub fn pedir(&mut self, sesion_id: u32, host_nombre: String, bytes: u64) {
        self.pendiente = Some(Pendiente {
            sesion_id,
            host_nombre,
            bytes,
            desde: Instant::now(),
        });
    }

    /// La petición pendiente, si sigue viva.
    #[must_use]
    pub fn vigente(&self, ahora: Instant) -> Option<&Pendiente> {
        self.pendiente
            .as_ref()
            .filter(|pendiente| !pendiente.caducada(ahora))
    }

    /// Se lleva la petición al permitirla.
    pub fn tomar(&mut self) -> Option<Pendiente> {
        self.pendiente.take()
    }

    /// Descarta la petición si ha caducado.
    pub fn limpiar(&mut self, ahora: Instant) -> bool {
        if self.pendiente.as_ref().is_some_and(|p| p.caducada(ahora)) {
            self.pendiente = None;
            return true;
        }
        false
    }
}

/// Tamaño legible de una copia: `1,2 KB` o `312 B`.
#[must_use]
pub fn tamano_legible(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let kb = bytes as f64 / 1024.0;
    if kb < 1024.0 {
        return format!("{kb:.1} KB").replace('.', ",");
    }
    let mb = kb / 1024.0;
    format!("{mb:.1} MB").replace('.', ",")
}

/// ¿MAGI corre dentro de otra sesión SSH? Entonces OSC 52 sale por ella.
#[must_use]
pub fn dentro_de_ssh() -> bool {
    std::env::var_os("SSH_CONNECTION").is_some() || std::env::var_os("SSH_TTY").is_some()
}

/// Copia al portapapeles del sistema y, si no hay, al terminal exterior con
/// OSC 52. Devuelve de dónde salió.
pub fn copiar(texto: &str) -> Result<Origen, String> {
    if dentro_de_ssh() {
        return osc52(texto);
    }
    match crate::portapapeles::copiar(texto) {
        Ok(herramienta) => Ok(Origen::Sistema(herramienta.nombre())),
        Err(motivo) => match osc52(texto) {
            Ok(origen) => Ok(origen),
            Err(error) => Err(format!("{motivo}; y OSC 52 falló: {error}")),
        },
    }
}

/// Copia por OSC 52 hacia el terminal exterior.
fn osc52(texto: &str) -> Result<Origen, String> {
    use base64::Engine as _;
    use std::io::Write as _;
    let codificado = base64::engine::general_purpose::STANDARD.encode(texto.as_bytes());
    let mut salida = std::io::stdout();
    write!(salida, "\x1b]52;c;{codificado}\x07").map_err(|error| error.to_string())?;
    salida.flush().map_err(|error| error.to_string())?;
    Ok(Origen::Osc52)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn solo_se_guarda_la_ultima_peticion_y_caduca_a_los_sesenta_segundos() {
        let mut estado = EstadoPortapapelesRemoto::default();
        estado.pedir(1, "web-01".to_string(), 10);
        estado.pedir(2, "db-01".to_string(), 20);
        let ahora = Instant::now();
        assert_eq!(estado.vigente(ahora).unwrap().sesion_id, 2);
        assert!(!estado.limpiar(ahora));
        // Sesenta segundos después ya no vale.
        assert!(estado.limpiar(ahora + PENDIENTE + Duration::from_secs(1)));
        assert!(estado.tomar().is_none());
    }

    #[test]
    fn el_tamano_se_dice_en_bytes_kb_o_mb() {
        assert_eq!(tamano_legible(312), "312 B");
        assert_eq!(tamano_legible(1229), "1,2 KB");
        assert_eq!(tamano_legible(2 * 1024 * 1024), "2,0 MB");
    }
}
