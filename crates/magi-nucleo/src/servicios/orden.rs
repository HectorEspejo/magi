//! Órdenes remotas de la Fase 13: nombres válidos, composición de los
//! argumentos de `servicios.sh` y conversión de `OrdenPestana` a la orden que
//! se escribe en una pestaña.
//!
//! Todo argumento se valida en Rust antes de mandarlo y el script lo vuelve a
//! validar. Nada del inventario se interpola en el texto de la orden: el script
//! viaja como argumento de `sh -c` y los datos, como argumentos separados.

use super::{limites, Accion, MotorNombre, Objetivo, OrdenPestana, Privilegio, TipoObjetivo};

/// Script embebido que se ejecuta en el host.
pub const SCRIPT: &str = include_str!("../../scripts/servicios.sh");

/// Nombre del programa que ve el script en `$0`.
pub const PROGRAMA: &str = "magi-servicios";

/// Suborden de `servicios.sh` con sus argumentos ya validados.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Suborden {
    Privilegio,
    Sudo,
    Listar {
        /// `docker,podman`, `docker`, `podman` o `-` (sin contenedores).
        motores: String,
    },
    Consumo {
        motor: MotorNombre,
    },
    Detalle(Objetivo),
    Accion {
        objetivo: Objetivo,
        accion: Accion,
        espera_seg: u32,
        comprobacion: Comprobacion,
    },
    Logs {
        objetivo: Objetivo,
        lineas_iniciales: u32,
    },
}

/// Qué comprobación de configuración hay que hacer antes de la acción (§7.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Comprobacion {
    /// No hay comprobación: se ejecuta tal cual.
    Ninguna,
    /// La conoce MAGI (Nginx, Caddy, Apache, HAProxy o Compose).
    Conocida,
    /// La escribió el usuario en COMPROBACIONES_SERVICIO.
    Propia(String),
}

/// ¿Es un nombre de unidad válido para acciones? `[A-Za-z0-9@._-]+`,
/// hasta 256 caracteres, acabado en `.service` o `.timer` (§7.11).
pub fn nombre_unidad_valido(nombre: &str) -> bool {
    let _ = nombre;
    false
}

/// ¿Es un nombre de contenedor válido? Id hexadecimal de 12 a 64 caracteres o
/// nombre `[A-Za-z0-9][A-Za-z0-9_.-]*` de hasta 128.
pub fn nombre_contenedor_valido(nombre: &str) -> bool {
    let _ = nombre;
    false
}

/// ¿Es un nombre de proyecto Compose válido? `[a-z0-9][a-z0-9_-]*`, hasta 64.
pub fn nombre_proyecto_valido(nombre: &str) -> bool {
    let _ = nombre;
    false
}

/// Valida el objetivo completo según su tipo.
pub fn validar_objetivo(objetivo: &Objetivo) -> Result<(), String> {
    let _ = objetivo;
    Err("sin implementar".to_string())
}

/// ¿Es un comando de comprobación propia válido? Una línea de 1 a 1 024
/// caracteres, sin NUL ni saltos de línea; si empieza por `sudo`, se avisa
/// pero se acepta (§7.11).
pub fn validar_comando_propio(comando: &str) -> Result<(), String> {
    let _ = comando;
    Err("sin implementar".to_string())
}

/// ¿Es una unidad crítica (nombre exacto o patrón con `*`) válido?
pub fn patron_critica_valido(patron: &str) -> bool {
    let _ = patron;
    false
}

/// ¿El nombre de unidad coincide con el patrón de crítica (con `*`)?
pub fn coincide_patron(patron: &str, nombre: &str) -> bool {
    let _ = (patron, nombre);
    false
}

/// Argumentos que siguen a `magi-servicios <suborden> <privilegio>`.
pub fn argumentos(suborden: &Suborden) -> Vec<String> {
    let _ = suborden;
    Vec::new()
}

/// Nombre del suborden que ve el script.
pub fn nombre_suborden(suborden: &Suborden) -> &'static str {
    match suborden {
        Suborden::Privilegio => "privilegio",
        Suborden::Sudo => "sudo",
        Suborden::Listar { .. } => "listar",
        Suborden::Consumo { .. } => "consumo",
        Suborden::Detalle(_) => "detalle",
        Suborden::Accion { .. } => "accion",
        Suborden::Logs { .. } => "logs",
    }
}

/// Orden remota completa: `sh -c '<script>' magi-servicios <suborden>
/// <privilegio> <args…>`. Todos los argumentos van entrecomillados por
/// seguridad, aunque ya estén validados.
pub fn comando(suborden: &Suborden, privilegio: Privilegio) -> String {
    let mut partes = vec![
        "sh".to_string(),
        "-c".to_string(),
        entrecomillar(SCRIPT),
        PROGRAMA.to_string(),
        nombre_suborden(suborden).to_string(),
        privilegio.texto().to_string(),
    ];
    partes.extend(
        argumentos(suborden)
            .into_iter()
            .map(|arg| entrecomillar(&arg)),
    );
    partes.join(" ")
}

/// Entrecomilla un argumento para que el shell lo reciba literal.
pub fn entrecomillar(texto: &str) -> String {
    format!("'{}'", texto.replace('\'', "'\\''"))
}

/// Orden que se escribe en la pestaña de una `OrdenPestana` (D248).
///
/// La pestaña es una shell normal: la orden se escribe como comando inicial y
/// termina con `exit`, para que la pestaña se cierre al salir. Con
/// `sudo_sin_clave` se usa `sudo -n`; con `sudo_con_clave`, `sudo` normal,
/// que pregunta en la propia pestaña. `None` si no hay privilegios.
pub fn comando_pestana(orden: &OrdenPestana, privilegio: Privilegio) -> Option<String> {
    let _ = (orden, privilegio);
    None
}

/// Texto de la comprobación que se hará antes de una acción, para la
/// confirmación: «nginx -t», «config -q», «la comprobación propia» o nada.
pub fn texto_comprobacion(comprobacion: &Comprobacion) -> Option<&'static str> {
    match comprobacion {
        Comprobacion::Ninguna => None,
        Comprobacion::Conocida => Some("la comprobación conocida"),
        Comprobacion::Propia(_) => Some("la comprobación propia"),
    }
}

/// Lista de motores que se pasa a la suborden `listar` a partir de la
/// preferencia configurada. Vacía = `-`.
pub fn motores_para_listar(preferencia: &[MotorNombre]) -> String {
    if preferencia.is_empty() {
        return "-".to_string();
    }
    preferencia
        .iter()
        .map(|motor| motor.texto())
        .collect::<Vec<_>>()
        .join(",")
}

/// Valida `[servicios] criticas`: nombres de unidad válidos o patrones con
/// `*`. Devuelve un aviso por cada patrón que no vale.
pub fn validar_criticas(patrones: &[String]) -> Vec<String> {
    patrones
        .iter()
        .filter(|patron| !patron_critica_valido(patron))
        .map(|patron| format!("[servicios] criticas: patrón no válido «{patron}»"))
        .collect()
}

/// Longitud máxima de un nombre que se acepta en una orden.
pub const NOMBRE_MAX: usize = limites::MAX_NOMBRE;

/// Tipos de objetivo que se aceptan en una orden.
pub fn tipo_objetivo_admite_accion(tipo: TipoObjetivo) -> bool {
    matches!(
        tipo,
        TipoObjetivo::Unidad
            | TipoObjetivo::Timer
            | TipoObjetivo::Contenedor
            | TipoObjetivo::Proyecto
    )
}
