//! Ajustes del móvil (informe §5.4).
//!
//! Los valores que en el escritorio viven en `config.toml` (`[flota]`,
//! `[servicios]` y `[deliberacion]`) van en el móvil a DataStore y llegan al
//! núcleo con `Nucleo::aplicar_ajustes`. Aquí está el modelo, sus valores por
//! defecto, sus rangos y la normalización que acota lo que se sale de rango con
//! un aviso legible: un valor malo nunca impide arrancar.
//!
//! Los ajustes ya normalizados viven en `EstadoNucleo.ajustes` (bloque de
//! task-6) en la forma interna que usan el sondeo, los servicios y la
//! deliberación; el resto de módulos los leen con `actuales`, `umbrales` o
//! `motores` en vez de leer el candado a mano.

use magi_nucleo::flota::Umbrales;

use crate::api::{AjustesMovil, ErrorNucleo};
use crate::estado::EstadoNucleo;

/// Auto-refresco de Flota por defecto (30 s).
pub const FLOTA_REFRESCO_SEG_POR_DEFECTO: u64 = 30;
/// Auto-refresco mínimo de Flota (15 s; `0` = apagado).
pub const FLOTA_REFRESCO_SEG_MIN: u64 = 15;
/// Auto-refresco máximo de Flota (300 s).
pub const FLOTA_REFRESCO_SEG_MAX: u64 = 300;

/// Umbral de carga por núcleo por defecto.
pub const CARGA_POR_NUCLEO_POR_DEFECTO: f64 = 1.0;
/// Umbral de carga por núcleo mínimo (0,1).
pub const CARGA_POR_NUCLEO_MIN: f64 = 0.1;
/// Umbral de carga por núcleo máximo (16,0).
pub const CARGA_POR_NUCLEO_MAX: f64 = 16.0;

/// Umbral de memoria por defecto (90 %).
pub const MEMORIA_PCT_POR_DEFECTO: f64 = 90.0;
/// Umbral de disco por defecto (90 %).
pub const DISCO_PCT_POR_DEFECTO: f64 = 90.0;
/// Umbral mínimo de memoria y disco (0 %).
pub const UMBRAL_PCT_MIN: f64 = 0.0;
/// Umbral máximo de memoria y disco (100 %).
pub const UMBRAL_PCT_MAX: f64 = 100.0;

/// Refresco de Servicios por defecto (10 s).
pub const SERVICIOS_REFRESCO_SEG_POR_DEFECTO: u64 = 10;
/// Refresco de Servicios mínimo (5 s; `0` = solo al entrar).
pub const SERVICIOS_REFRESCO_SEG_MIN: u64 = 5;
/// Refresco de Servicios máximo (300 s).
pub const SERVICIOS_REFRESCO_SEG_MAX: u64 = 300;
/// Espera al estado final de una acción por defecto (30 s).
pub const SERVICIOS_ESPERA_SEG_POR_DEFECTO: u64 = 30;
/// Espera mínima al estado final de una acción (5 s).
pub const SERVICIOS_ESPERA_SEG_MIN: u64 = 5;
/// Espera máxima al estado final de una acción (300 s).
pub const SERVICIOS_ESPERA_SEG_MAX: u64 = 300;
/// Minutos de sudo en memoria por defecto (5).
pub const SUDO_MEMORIA_MIN_POR_DEFECTO: u64 = 5;
/// Minutos máximos de sudo en memoria (15; `0` = pedirla siempre).
pub const SUDO_MEMORIA_MIN_MAX: u64 = 15;

/// Antigüedad máxima del backup por defecto (24 h).
pub const BACKUP_HORAS_POR_DEFECTO: u64 = 24;
/// Antigüedad mínima del backup (1 h).
pub const BACKUP_HORAS_MIN: u64 = 1;
/// Antigüedad máxima del backup (720 h, 30 días).
pub const BACKUP_HORAS_MAX: u64 = 720;
/// Antigüedad máxima del sondeo de MELCHIOR-1 por defecto (5 min).
pub const SALUD_MAX_MIN_POR_DEFECTO: u64 = 5;
/// Antigüedad mínima del sondeo de MELCHIOR-1 (1 min).
pub const SALUD_MAX_MIN_MIN: u64 = 1;
/// Antigüedad máxima del sondeo de MELCHIOR-1 (1 440 min, 24 h).
pub const SALUD_MAX_MIN_MAX: u64 = 1_440;

/// Motores de contenedores que admite `servicios_contenedores`.
pub const MOTORES_VALIDOS: [&str; 2] = ["docker", "podman"];

impl AjustesMovil {
    /// Los ajustes por defecto del móvil: los mismos del escritorio (informe
    /// §5.4). La app los pone en DataStore al instalarse.
    pub fn por_defecto() -> Self {
        Self {
            flota_refresco_seg: FLOTA_REFRESCO_SEG_POR_DEFECTO,
            umbral_carga_por_nucleo: CARGA_POR_NUCLEO_POR_DEFECTO,
            umbral_memoria_pct: MEMORIA_PCT_POR_DEFECTO,
            umbral_disco_pct: DISCO_PCT_POR_DEFECTO,
            servicios_refresco_seg: SERVICIOS_REFRESCO_SEG_POR_DEFECTO,
            servicios_espera_seg: SERVICIOS_ESPERA_SEG_POR_DEFECTO,
            servicios_contenedores: MOTORES_VALIDOS
                .iter()
                .map(|motor| motor.to_string())
                .collect(),
            sudo_memoria_min: SUDO_MEMORIA_MIN_POR_DEFECTO,
            deliberacion_backup_horas: BACKUP_HORAS_POR_DEFECTO,
            deliberacion_salud_max_min: SALUD_MAX_MIN_POR_DEFECTO,
        }
    }

    /// Acota los valores fuera de rango y devuelve los ajustes normalizados con
    /// un aviso por cada corrección. Es idempotente: normalizar lo ya
    /// normalizado no produce avisos.
    pub fn normalizado(self) -> (Self, Vec<String>) {
        let mut avisos = Vec::new();
        let normalizados = Self {
            flota_refresco_seg: refresco(
                self.flota_refresco_seg,
                FLOTA_REFRESCO_SEG_MIN,
                FLOTA_REFRESCO_SEG_MAX,
                "flota_refresco_seg",
                &mut avisos,
            ),
            umbral_carga_por_nucleo: umbral(
                self.umbral_carga_por_nucleo,
                CARGA_POR_NUCLEO_MIN,
                CARGA_POR_NUCLEO_MAX,
                CARGA_POR_NUCLEO_POR_DEFECTO,
                "umbral_carga_por_nucleo",
                &mut avisos,
            ),
            umbral_memoria_pct: umbral(
                self.umbral_memoria_pct,
                UMBRAL_PCT_MIN,
                UMBRAL_PCT_MAX,
                MEMORIA_PCT_POR_DEFECTO,
                "umbral_memoria_pct",
                &mut avisos,
            ),
            umbral_disco_pct: umbral(
                self.umbral_disco_pct,
                UMBRAL_PCT_MIN,
                UMBRAL_PCT_MAX,
                DISCO_PCT_POR_DEFECTO,
                "umbral_disco_pct",
                &mut avisos,
            ),
            servicios_refresco_seg: refresco(
                self.servicios_refresco_seg,
                SERVICIOS_REFRESCO_SEG_MIN,
                SERVICIOS_REFRESCO_SEG_MAX,
                "servicios_refresco_seg",
                &mut avisos,
            ),
            servicios_espera_seg: acotar(
                self.servicios_espera_seg,
                SERVICIOS_ESPERA_SEG_MIN,
                SERVICIOS_ESPERA_SEG_MAX,
                "servicios_espera_seg",
                "s",
                &mut avisos,
            ),
            servicios_contenedores: normalizar_motores(self.servicios_contenedores, &mut avisos),
            sudo_memoria_min: acotar(
                self.sudo_memoria_min,
                0,
                SUDO_MEMORIA_MIN_MAX,
                "sudo_memoria_min",
                "min",
                &mut avisos,
            ),
            deliberacion_backup_horas: acotar(
                self.deliberacion_backup_horas,
                BACKUP_HORAS_MIN,
                BACKUP_HORAS_MAX,
                "deliberacion_backup_horas",
                "h",
                &mut avisos,
            ),
            deliberacion_salud_max_min: acotar(
                self.deliberacion_salud_max_min,
                SALUD_MAX_MIN_MIN,
                SALUD_MAX_MIN_MAX,
                "deliberacion_salud_max_min",
                "min",
                &mut avisos,
            ),
        };
        (normalizados, avisos)
    }
}

/// Ajustes ya normalizados, en la forma que usan el sondeo, los servicios y la
/// deliberación (informe §5.4). Es un tipo interno: no cruza UniFFI.
#[derive(Debug, Clone)]
pub struct AjustesNucleo {
    /// Auto-refresco de Flota en segundos (0 = apagado).
    pub flota_refresco_seg: u64,
    /// Umbrales del estado de Flota, los mismos del escritorio.
    pub umbrales: Umbrales,
    /// Refresco de la vista Servicios en segundos (0 = solo al entrar).
    pub servicios_refresco_seg: u64,
    /// Espera máxima al estado final de una acción, en segundos.
    pub servicios_espera_seg: u64,
    /// Motores de contenedores, sin repetir y en orden de preferencia.
    pub motores: Vec<String>,
    /// Minutos sin uso tras los que se olvida la contraseña de sudo.
    pub sudo_memoria_min: u64,
    /// BALTHASAR-2: antigüedad máxima del último backup, en horas.
    pub deliberacion_backup_horas: u64,
    /// MELCHIOR-1: antigüedad máxima del último sondeo, en minutos.
    pub deliberacion_salud_max_min: u64,
}

impl AjustesNucleo {
    /// Los ajustes por defecto del móvil (los del escritorio, informe §5.4).
    pub fn por_defecto() -> Self {
        Self::desde_normalizado(&AjustesMovil::por_defecto())
    }

    /// Construye los ajustes internos a partir de unos ya normalizados: no
    /// vuelve a acotar ni avisa, solo traduce.
    fn desde_normalizado(ajustes: &AjustesMovil) -> Self {
        Self {
            flota_refresco_seg: ajustes.flota_refresco_seg,
            umbrales: Umbrales {
                carga_por_nucleo: ajustes.umbral_carga_por_nucleo,
                memoria_pct: ajustes.umbral_memoria_pct,
                disco_pct: ajustes.umbral_disco_pct,
            },
            servicios_refresco_seg: ajustes.servicios_refresco_seg,
            servicios_espera_seg: ajustes.servicios_espera_seg,
            motores: ajustes.servicios_contenedores.clone(),
            sudo_memoria_min: ajustes.sudo_memoria_min,
            deliberacion_backup_horas: ajustes.deliberacion_backup_horas,
            deliberacion_salud_max_min: ajustes.deliberacion_salud_max_min,
        }
    }
}

/// Ajustes vigentes del móvil, en copia. Si el candado se corrompe se usan los
/// de defecto: el sondeo nunca se queda sin umbrales.
pub fn actuales(estado: &EstadoNucleo) -> AjustesNucleo {
    estado
        .ajustes
        .read()
        .map(|ajustes| ajustes.clone())
        .unwrap_or_else(|_| AjustesNucleo::por_defecto())
}

/// Umbrales vigentes del estado de Flota.
pub fn umbrales(estado: &EstadoNucleo) -> Umbrales {
    actuales(estado).umbrales
}

/// Motores de contenedores vigentes para el sondeo (`<motores>`).
pub fn motores(estado: &EstadoNucleo) -> Vec<String> {
    actuales(estado).motores
}

/// Pasa al núcleo los ajustes del móvil (informe §5.4): acota los valores
/// fuera de rango, guarda los normalizados en `EstadoNucleo` y devuelve los
/// avisos. Un valor malo no impide arrancar: se usa el acotado.
pub fn aplicar(estado: &EstadoNucleo, ajustes: AjustesMovil) -> Result<Vec<String>, ErrorNucleo> {
    let (normalizados, avisos) = ajustes.normalizado();
    let nuevos = AjustesNucleo::desde_normalizado(&normalizados);
    if let Ok(mut actuales) = estado.ajustes.write() {
        *actuales = nuevos;
    }
    Ok(avisos)
}

/// Un refresco admite `0` (apagado) o un rango con mínimo.
fn refresco(valor: u64, minimo: u64, maximo: u64, campo: &str, avisos: &mut Vec<String>) -> u64 {
    if valor == 0 {
        return 0;
    }
    let acotado = valor.clamp(minimo, maximo);
    if acotado != valor {
        avisos.push(format!(
            "{campo} = {valor} s: fuera de rango, se usa {acotado} s (0 = apagado)"
        ));
    }
    acotado
}

/// Acota un entero a su rango, con aviso si había que corregirlo.
fn acotar(
    valor: u64,
    minimo: u64,
    maximo: u64,
    campo: &str,
    unidad: &str,
    avisos: &mut Vec<String>,
) -> u64 {
    let acotado = valor.clamp(minimo, maximo);
    if acotado != valor {
        avisos.push(format!(
            "{campo} = {valor} {unidad}: fuera de rango, se usa {acotado} {unidad}"
        ));
    }
    acotado
}

/// Acota un umbral en coma flotante; un valor no finito vuelve al de defecto.
fn umbral(
    valor: f64,
    minimo: f64,
    maximo: f64,
    defecto: f64,
    campo: &str,
    avisos: &mut Vec<String>,
) -> f64 {
    if !valor.is_finite() {
        avisos.push(format!("{campo} no es un número válido: se usa {defecto}"));
        return defecto;
    }
    let acotado = valor.clamp(minimo, maximo);
    if acotado != valor {
        avisos.push(format!(
            "{campo} = {valor}: fuera de rango, se usa {acotado}"
        ));
    }
    acotado
}

/// Deja solo `docker` y `podman`, sin repetir y en el orden recibido. Un motor
/// desconocido o repetido se ignora con un aviso (informe §5.4).
fn normalizar_motores(motores: Vec<String>, avisos: &mut Vec<String>) -> Vec<String> {
    let mut limpios: Vec<String> = Vec::new();
    for motor in motores {
        let motor = motor.trim().to_lowercase();
        if motor.is_empty() {
            avisos.push("servicios_contenedores: se ignora un motor vacío".to_string());
            continue;
        }
        if !MOTORES_VALIDOS.contains(&motor.as_str()) {
            avisos.push(format!(
                "servicios_contenedores solo admite docker o podman, no «{motor}»: se ignora"
            ));
            continue;
        }
        if limpios.contains(&motor) {
            avisos.push(format!(
                "servicios_contenedores repite «{motor}»: se usa una vez"
            ));
            continue;
        }
        limpios.push(motor);
    }
    limpios
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn los_valores_por_defecto_ya_estan_normalizados() {
        let (normalizados, avisos) = AjustesMovil::por_defecto().normalizado();
        assert_eq!(normalizados, AjustesMovil::por_defecto());
        assert!(avisos.is_empty(), "sin avisos: {avisos:?}");
        assert_eq!(normalizados.flota_refresco_seg, 30);
        assert_eq!(normalizados.servicios_contenedores, ["docker", "podman"]);
    }

    #[test]
    fn fuera_de_rango_se_acota_con_aviso() {
        let ajustes = AjustesMovil {
            flota_refresco_seg: 5,
            umbral_carga_por_nucleo: 0.0,
            umbral_memoria_pct: 150.0,
            umbral_disco_pct: -3.0,
            servicios_refresco_seg: 2,
            servicios_espera_seg: 1_000,
            servicios_contenedores: vec!["docker".to_string()],
            sudo_memoria_min: 99,
            deliberacion_backup_horas: 0,
            deliberacion_salud_max_min: 0,
        };
        let (normalizados, avisos) = ajustes.normalizado();
        assert_eq!(normalizados.flota_refresco_seg, 15);
        assert_eq!(normalizados.umbral_carga_por_nucleo, 0.1);
        assert_eq!(normalizados.umbral_memoria_pct, 100.0);
        assert_eq!(normalizados.umbral_disco_pct, 0.0);
        assert_eq!(normalizados.servicios_refresco_seg, 5);
        assert_eq!(normalizados.servicios_espera_seg, 300);
        assert_eq!(normalizados.sudo_memoria_min, 15);
        assert_eq!(normalizados.deliberacion_backup_horas, 1);
        assert_eq!(normalizados.deliberacion_salud_max_min, 1);
        assert_eq!(avisos.len(), 9, "un aviso por campo acotado: {avisos:?}");
        assert!(avisos
            .iter()
            .any(|aviso| aviso.contains("flota_refresco_seg")));
        assert!(avisos
            .iter()
            .any(|aviso| aviso.contains("umbral_memoria_pct")));
        assert!(avisos
            .iter()
            .any(|aviso| aviso.contains("sudo_memoria_min")));
        // La normalización es idempotente: lo ya acotado no vuelve a avisar.
        let (otra_vez, avisos_otra_vez) = normalizados.clone().normalizado();
        assert!(avisos_otra_vez.is_empty(), "{avisos_otra_vez:?}");
        assert_eq!(otra_vez, normalizados);
    }

    #[test]
    fn el_cero_de_los_refrescos_y_del_sudo_se_respeta() {
        let ajustes = AjustesMovil {
            flota_refresco_seg: 0,
            servicios_refresco_seg: 0,
            sudo_memoria_min: 0,
            ..AjustesMovil::por_defecto()
        };
        let (normalizados, avisos) = ajustes.normalizado();
        assert!(avisos.is_empty(), "{avisos:?}");
        assert_eq!(normalizados.flota_refresco_seg, 0);
        assert_eq!(normalizados.servicios_refresco_seg, 0);
        assert_eq!(normalizados.sudo_memoria_min, 0);
    }

    #[test]
    fn un_umbral_no_finito_vuelve_al_de_defecto() {
        let ajustes = AjustesMovil {
            umbral_carga_por_nucleo: f64::NAN,
            ..AjustesMovil::por_defecto()
        };
        let (normalizados, avisos) = ajustes.normalizado();
        assert_eq!(normalizados.umbral_carga_por_nucleo, 1.0);
        assert_eq!(avisos.len(), 1);
        assert!(avisos[0].contains("no es un número válido"));
    }

    #[test]
    fn los_motores_se_validan_sin_repetir() {
        let ajustes = AjustesMovil {
            servicios_contenedores: vec![
                " Podman ".to_string(),
                "docker".to_string(),
                "docker".to_string(),
                "lxc".to_string(),
                "".to_string(),
            ],
            ..AjustesMovil::por_defecto()
        };
        let (normalizados, avisos) = ajustes.normalizado();
        assert_eq!(normalizados.servicios_contenedores, ["podman", "docker"]);
        assert_eq!(avisos.len(), 3, "{avisos:?}");
        assert!(avisos.iter().any(|aviso| aviso.contains("lxc")));
        assert!(avisos.iter().any(|aviso| aviso.contains("repite")));
    }

    #[test]
    fn una_lista_de_motores_vacia_es_sin_contenedores() {
        let ajustes = AjustesMovil {
            servicios_contenedores: Vec::new(),
            ..AjustesMovil::por_defecto()
        };
        let (normalizados, avisos) = ajustes.normalizado();
        assert!(normalizados.servicios_contenedores.is_empty());
        assert!(avisos.is_empty());
    }

    #[test]
    fn los_ajustes_internos_salen_de_los_normalizados() {
        let nucleo = AjustesNucleo::por_defecto();
        assert_eq!(nucleo.flota_refresco_seg, 30);
        assert_eq!(nucleo.umbrales.carga_por_nucleo, 1.0);
        assert_eq!(nucleo.umbrales.memoria_pct, 90.0);
        assert_eq!(nucleo.umbrales.disco_pct, 90.0);
        assert_eq!(nucleo.servicios_refresco_seg, 10);
        assert_eq!(nucleo.servicios_espera_seg, 30);
        assert_eq!(nucleo.motores, ["docker", "podman"]);
        assert_eq!(nucleo.sudo_memoria_min, 5);
        assert_eq!(nucleo.deliberacion_backup_horas, 24);
        assert_eq!(nucleo.deliberacion_salud_max_min, 5);

        // Un valor fuera de rango se traduce ya acotado.
        let ajustes = AjustesMovil {
            umbral_memoria_pct: 150.0,
            servicios_contenedores: vec!["podman".to_string()],
            ..AjustesMovil::por_defecto()
        };
        let (normalizados, _) = ajustes.normalizado();
        let nucleo = AjustesNucleo::desde_normalizado(&normalizados);
        assert_eq!(nucleo.umbrales.memoria_pct, 100.0);
        assert_eq!(nucleo.motores, ["podman"]);
    }
}
