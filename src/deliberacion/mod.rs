//! Deliberación MAGI (Fase 6): la pausa obligatoria antes de una ejecución
//! arriesgada. Tres comprobaciones por host —MELCHIOR-1 salud, BALTHASAR-2
//! backup reciente, CASPER-3 tests en verde—, unanimidad para ejecutar y
//! forzado solo con motivo escrito, que queda registrado.
//!
//! Este módulo tiene el modelo (qué exige cada host, veredictos, la fila de
//! `DELIBERACIONES`); las comprobaciones y su orquestación, en sus submódulos.

pub mod backup;
pub mod estado;
pub mod salud;
pub mod tests;

use serde::{Deserialize, Serialize};

/// Qué comprobaciones exige un host antes de una ejecución
/// (`VERIFICACIONES_HOST`; sin fila, ninguna).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Verificaciones {
    pub host_id: i64,
    pub salud: bool,
    pub backup: bool,
    /// Directorio remoto de los backups.
    pub backup_ruta: Option<String>,
    /// Glob opcional sobre el nombre de fichero (`*.sql.gz`).
    pub backup_patron: Option<String>,
    pub tests: bool,
    /// Comando local (`sh -c`) que dice si los tests están en verde.
    pub tests_comando: Option<String>,
    pub actualizado_en: String,
}

impl Verificaciones {
    pub fn alguna_activa(&self) -> bool {
        self.salud || self.backup || self.tests
    }

    pub fn activa(&self, comprobacion: Comprobacion) -> bool {
        match comprobacion {
            Comprobacion::Salud => self.salud,
            Comprobacion::Backup => self.backup,
            Comprobacion::Tests => self.tests,
        }
    }

    pub fn datos(&self) -> DatosVerificaciones {
        DatosVerificaciones {
            salud: self.salud,
            backup: self.backup,
            backup_ruta: self.backup_ruta.clone(),
            backup_patron: self.backup_patron.clone(),
            tests: self.tests,
            tests_comando: self.tests_comando.clone(),
        }
    }
}

/// Lo que se edita en el bloque «Verificaciones previas» de la ficha.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DatosVerificaciones {
    pub salud: bool,
    pub backup: bool,
    pub backup_ruta: Option<String>,
    pub backup_patron: Option<String>,
    pub tests: bool,
    pub tests_comando: Option<String>,
}

impl DatosVerificaciones {
    pub fn alguna_activa(&self) -> bool {
        self.salud || self.backup || self.tests
    }

    /// Recorta los textos y deja en `None` los vacíos.
    pub fn normalizada(mut self) -> Self {
        let limpiar = |texto: Option<String>| {
            texto
                .map(|texto| texto.trim().to_string())
                .filter(|texto| !texto.is_empty())
        };
        self.backup_ruta = limpiar(self.backup_ruta);
        self.backup_patron = limpiar(self.backup_patron);
        self.tests_comando = limpiar(self.tests_comando);
        self
    }
}

/// Valida el bloque de la ficha: con backup, la ruta es obligatoria (absoluta
/// o relativa al inicio del usuario remoto) y el patrón, si lo hay, un glob
/// válido; con tests, el comando es obligatorio.
pub fn validar_verificaciones(datos: &DatosVerificaciones) -> Result<(), String> {
    if datos.backup {
        let Some(ruta) = datos.backup_ruta.as_deref() else {
            return Err("la comprobación de backup necesita el directorio remoto".to_string());
        };
        if ruta.chars().any(char::is_control) {
            return Err("la ruta del backup no puede llevar caracteres de control".to_string());
        }
    }
    if let Some(patron) = datos.backup_patron.as_deref() {
        globset::Glob::new(patron)
            .map_err(|error| format!("el patrón del backup no es válido: {error}"))?;
    }
    if datos.tests && datos.tests_comando.is_none() {
        return Err("la comprobación de tests necesita un comando".to_string());
    }
    Ok(())
}

/// Las tres comprobaciones, con su nombre MAGI y la palabra que dice qué
/// comprueban (la metáfora nunca va sola).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Comprobacion {
    Salud,
    Backup,
    Tests,
}

impl Comprobacion {
    pub const TODAS: [Comprobacion; 3] = [
        Comprobacion::Salud,
        Comprobacion::Backup,
        Comprobacion::Tests,
    ];

    pub fn nombre_magi(self) -> &'static str {
        match self {
            Comprobacion::Salud => "MELCHIOR-1",
            Comprobacion::Backup => "BALTHASAR-2",
            Comprobacion::Tests => "CASPER-3",
        }
    }

    pub fn palabra(self) -> &'static str {
        match self {
            Comprobacion::Salud => "salud",
            Comprobacion::Backup => "backup",
            Comprobacion::Tests => "tests",
        }
    }
}

/// Veredicto de una comprobación en un host. Siempre con el dato que lo
/// justifica: nunca un ✕ sin motivo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "estado")]
pub enum Veredicto {
    /// La comprobación no está activa en ese host (`—`).
    #[serde(rename = "n/a")]
    NoActiva,
    /// Aún comprobando (`◐`); en la fila de una deliberación cancelada.
    #[serde(rename = "pendiente")]
    Pendiente,
    #[serde(rename = "aprueba")]
    Aprueba { detalle: String, ms: u64 },
    #[serde(rename = "rechaza")]
    Rechaza { detalle: String, ms: u64 },
}

impl Veredicto {
    pub fn cuenta(&self) -> bool {
        !matches!(self, Veredicto::NoActiva)
    }

    pub fn aprueba(&self) -> bool {
        matches!(self, Veredicto::Aprueba { .. })
    }

    pub fn rechaza(&self) -> bool {
        matches!(self, Veredicto::Rechaza { .. })
    }

    pub fn detalle(&self) -> Option<&str> {
        match self {
            Veredicto::Aprueba { detalle, .. } | Veredicto::Rechaza { detalle, .. } => {
                Some(detalle)
            }
            _ => None,
        }
    }
}

/// Las tres celdas de un host en la deliberación (`comprobaciones_json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComprobacionesHost {
    pub host_id: i64,
    pub host: String,
    pub salud: Veredicto,
    pub backup: Veredicto,
    pub tests: Veredicto,
}

impl ComprobacionesHost {
    pub fn veredicto(&self, comprobacion: Comprobacion) -> &Veredicto {
        match comprobacion {
            Comprobacion::Salud => &self.salud,
            Comprobacion::Backup => &self.backup,
            Comprobacion::Tests => &self.tests,
        }
    }
}

/// Cómo se resolvió una deliberación (`DELIBERACIONES.resultado`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultadoDeliberacion {
    Aprobada,
    Forzada,
    Cancelada,
}

impl ResultadoDeliberacion {
    pub fn como_texto(self) -> &'static str {
        match self {
            ResultadoDeliberacion::Aprobada => "aprobada",
            ResultadoDeliberacion::Forzada => "forzada",
            ResultadoDeliberacion::Cancelada => "cancelada",
        }
    }

    pub fn desde_texto(texto: &str) -> Option<Self> {
        match texto {
            "aprobada" => Some(ResultadoDeliberacion::Aprobada),
            "forzada" => Some(ResultadoDeliberacion::Forzada),
            "cancelada" => Some(ResultadoDeliberacion::Cancelada),
            _ => None,
        }
    }
}

/// Cómo acabó la ejecución deliberada (`DELIBERACIONES.ejecucion_resultado`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EjecucionResultado {
    Ok,
    Parcial,
    Error,
    Cancelada,
}

impl EjecucionResultado {
    pub fn como_texto(self) -> &'static str {
        match self {
            EjecucionResultado::Ok => "ok",
            EjecucionResultado::Parcial => "parcial",
            EjecucionResultado::Error => "error",
            EjecucionResultado::Cancelada => "cancelada",
        }
    }

    pub fn desde_texto(texto: &str) -> Option<Self> {
        match texto {
            "ok" => Some(EjecucionResultado::Ok),
            "parcial" => Some(EjecucionResultado::Parcial),
            "error" => Some(EjecucionResultado::Error),
            "cancelada" => Some(EjecucionResultado::Cancelada),
            _ => None,
        }
    }

    /// `ok` si todos ok; `error` si ninguno; `parcial` en otro caso.
    pub fn de_hosts(ok: usize, total: usize, cancelada: bool) -> Self {
        if cancelada {
            EjecucionResultado::Cancelada
        } else if total > 0 && ok == total {
            EjecucionResultado::Ok
        } else if ok == 0 {
            EjecucionResultado::Error
        } else {
            EjecucionResultado::Parcial
        }
    }
}

/// Una deliberación resuelta, lista para insertar.
#[derive(Debug, Clone, PartialEq)]
pub struct NuevaDeliberacion {
    pub snippet_id: Option<i64>,
    /// «reiniciar nginx → hetzner-01, hetzner-02».
    pub accion: String,
    pub hosts: Vec<(i64, String)>,
    pub comprobaciones: Vec<ComprobacionesHost>,
    pub resultado: ResultadoDeliberacion,
    /// Hubo al menos un rechazo.
    pub bloqueada: bool,
    pub motivo: Option<String>,
    pub usuario: String,
}

/// Una fila de `DELIBERACIONES`.
#[derive(Debug, Clone, PartialEq)]
pub struct RegistroDeliberacion {
    pub id: i64,
    pub fecha: String,
    pub snippet_id: Option<i64>,
    pub accion: String,
    pub hosts: Vec<(i64, String)>,
    pub comprobaciones: Vec<ComprobacionesHost>,
    pub resultado: ResultadoDeliberacion,
    pub bloqueada: bool,
    pub motivo: Option<String>,
    pub usuario: String,
    pub ejecucion_resultado: Option<EjecucionResultado>,
}

impl RegistroDeliberacion {
    /// «N / M»: comprobaciones activas que aprobaron sobre las activas.
    pub fn consenso(&self) -> (usize, usize) {
        let celdas = self.comprobaciones.iter().flat_map(|host| {
            Comprobacion::TODAS
                .iter()
                .map(move |comprobacion| host.veredicto(*comprobacion))
        });
        let (mut aprobadas, mut activas) = (0, 0);
        for celda in celdas.filter(|celda| celda.cuenta()) {
            activas += 1;
            if celda.aprueba() {
                aprobadas += 1;
            }
        }
        (aprobadas, activas)
    }
}

/// Prefijo del detalle de las anotaciones `deliberacion_*` en `REGISTRO`: con
/// él, la vista Registro encuentra la fila de `DELIBERACIONES`.
const PREFIJO_DETALLE: &str = "deliberación #";

/// Detalle compacto de una anotación `deliberacion_*`: acción, hosts,
/// consenso, motivo, usuario y, si ya se ejecutó, el resultado. Los veredictos
/// por celda viven en `DELIBERACIONES`, no en el registro.
pub fn detalle_registro(
    deliberacion: &RegistroDeliberacion,
    resultado: Option<EjecucionResultado>,
) -> String {
    let (aprobadas, activas) = deliberacion.consenso();
    let mut detalle = format!(
        "{PREFIJO_DETALLE}{} · {} · {} host(s) · consenso {aprobadas}/{activas}",
        deliberacion.id,
        deliberacion.accion,
        deliberacion.hosts.len()
    );
    if let Some(motivo) = &deliberacion.motivo {
        detalle.push_str(&format!(" · motivo «{motivo}»"));
    }
    detalle.push_str(&format!(" · usuario {}", deliberacion.usuario));
    if let Some(resultado) = resultado {
        detalle.push_str(&format!(" · resultado {}", resultado.como_texto()));
    }
    detalle
}

/// Id de la deliberación en el detalle de una anotación `deliberacion_*`.
pub fn id_en_detalle(detalle: &str) -> Option<i64> {
    let resto = detalle.strip_prefix(PREFIJO_DETALLE)?;
    let numero: String = resto.chars().take_while(char::is_ascii_digit).collect();
    numero.parse().ok()
}

/// Usuario local para `DELIBERACIONES.usuario`: el del uid del proceso (una
/// variable de entorno se puede falsear y esto es un registro de auditoría);
/// `$USER` solo si el sistema no lo sabe.
pub fn usuario_del_sistema() -> String {
    nix::unistd::User::from_uid(nix::unistd::Uid::current())
        .ok()
        .flatten()
        .map(|usuario| usuario.name)
        .unwrap_or_else(crate::conexion::usuario_local)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_veredicto_viaja_como_json_legible() {
        let json = serde_json::to_string(&Veredicto::Rechaza {
            detalle: "hace 31 h".to_string(),
            ms: 120,
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"estado":"rechaza","detalle":"hace 31 h","ms":120}"#
        );
        let no_activa = serde_json::to_string(&Veredicto::NoActiva).unwrap();
        assert_eq!(no_activa, r#"{"estado":"n/a"}"#);
        let vuelta: Veredicto = serde_json::from_str(&json).unwrap();
        assert!(vuelta.rechaza());
    }

    #[test]
    fn validar_las_verificaciones() {
        assert!(validar_verificaciones(&DatosVerificaciones::default()).is_ok());
        assert!(validar_verificaciones(&DatosVerificaciones {
            backup: true,
            ..Default::default()
        })
        .is_err());
        assert!(validar_verificaciones(&DatosVerificaciones {
            backup: true,
            backup_ruta: Some("/var/backups".to_string()),
            backup_patron: Some("*.sql.gz".to_string()),
            ..Default::default()
        })
        .is_ok());
        assert!(validar_verificaciones(&DatosVerificaciones {
            backup_patron: Some("[".to_string()),
            ..Default::default()
        })
        .is_err());
        assert!(validar_verificaciones(&DatosVerificaciones {
            tests: true,
            ..Default::default()
        })
        .is_err());
    }

    #[test]
    fn el_resultado_de_la_ejecucion_segun_los_hosts() {
        assert_eq!(
            EjecucionResultado::de_hosts(3, 3, false),
            EjecucionResultado::Ok
        );
        assert_eq!(
            EjecucionResultado::de_hosts(0, 3, false),
            EjecucionResultado::Error
        );
        assert_eq!(
            EjecucionResultado::de_hosts(2, 3, false),
            EjecucionResultado::Parcial
        );
        assert_eq!(
            EjecucionResultado::de_hosts(3, 3, true),
            EjecucionResultado::Cancelada
        );
    }

    #[test]
    fn el_detalle_lleva_el_id_y_se_recupera() {
        let deliberacion = RegistroDeliberacion {
            id: 12,
            fecha: String::new(),
            snippet_id: Some(1),
            accion: "reiniciar nginx → hetzner-01".to_string(),
            hosts: vec![(1, "hetzner-01".to_string())],
            comprobaciones: vec![ComprobacionesHost {
                host_id: 1,
                host: "hetzner-01".to_string(),
                salud: Veredicto::Aprueba {
                    detalle: "NOMINAL".to_string(),
                    ms: 1,
                },
                backup: Veredicto::Rechaza {
                    detalle: "hace 31 h".to_string(),
                    ms: 1,
                },
                tests: Veredicto::NoActiva,
            }],
            resultado: ResultadoDeliberacion::Forzada,
            bloqueada: true,
            motivo: Some("revisado a mano".to_string()),
            usuario: "hector".to_string(),
            ejecucion_resultado: None,
        };
        let detalle = detalle_registro(&deliberacion, Some(EjecucionResultado::Ok));
        assert!(detalle.starts_with("deliberación #12 · "));
        assert!(detalle.contains("consenso 1/2"));
        assert!(detalle.contains("motivo «revisado a mano»"));
        assert!(detalle.contains("resultado ok"));
        assert_eq!(id_en_detalle(&detalle), Some(12));
        assert_eq!(id_en_detalle("otra cosa"), None);
    }

    #[test]
    fn el_usuario_del_sistema_no_esta_vacio() {
        assert!(!usuario_del_sistema().is_empty());
    }
}
