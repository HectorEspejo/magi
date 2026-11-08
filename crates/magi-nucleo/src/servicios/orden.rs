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

/// Shell que se abre dentro de un contenedor (bash si está, si no sh).
pub const SHELL_CONTENEDOR: &str = "command -v bash >/dev/null 2>&1 && exec bash || exec sh";

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

impl Comprobacion {
    /// Texto que viaja como argumento `<comprobación>`.
    pub fn texto(&self) -> &'static str {
        match self {
            Comprobacion::Ninguna => "ninguna",
            Comprobacion::Conocida => "conocida",
            Comprobacion::Propia(_) => "propia",
        }
    }
}

impl From<super::validadores::Comprobacion> for Comprobacion {
    fn from(comprobacion: super::validadores::Comprobacion) -> Self {
        match comprobacion {
            super::validadores::Comprobacion::Ninguna => Comprobacion::Ninguna,
            super::validadores::Comprobacion::Conocida(_) => Comprobacion::Conocida,
            super::validadores::Comprobacion::Propia(orden) => Comprobacion::Propia(orden),
        }
    }
}

/// ¿Es un nombre de unidad válido para acciones? `[A-Za-z0-9@._-]+`,
/// hasta 256 caracteres, acabado en `.service` o `.timer` (§7.11).
pub fn nombre_unidad_valido(nombre: &str) -> bool {
    if nombre.is_empty() || nombre.len() > limites::MAX_NOMBRE {
        return false;
    }
    if !nombre
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'@' | b'.' | b'_' | b'-'))
    {
        return false;
    }
    nombre.ends_with(".service") || nombre.ends_with(".timer")
}

/// ¿Es un nombre de contenedor válido? Id hexadecimal de 12 a 64 caracteres o
/// nombre `[A-Za-z0-9][A-Za-z0-9_.-]*` de hasta 128.
pub fn nombre_contenedor_valido(nombre: &str) -> bool {
    if nombre.is_empty() || nombre.len() > 128 {
        return false;
    }
    // Id hexadecimal completo o corto.
    if (12..=64).contains(&nombre.len()) && nombre.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return true;
    }
    if !nombre
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_alphanumeric())
    {
        return false;
    }
    nombre
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
}

/// ¿Es un nombre de proyecto Compose válido? `[a-z0-9][a-z0-9_-]*`, hasta 64.
pub fn nombre_proyecto_valido(nombre: &str) -> bool {
    if nombre.is_empty() || nombre.len() > 64 {
        return false;
    }
    if !nombre
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    {
        return false;
    }
    nombre.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
    })
}

/// ¿Es un nombre de unidad válido para el sondeo o para una crítica (acepta
/// nombres que no acaban en `.service` o `.timer`, como los targets)?
pub fn nombre_unidad_critica_valido(nombre: &str) -> bool {
    !nombre.is_empty()
        && nombre.len() <= limites::MAX_NOMBRE
        && nombre
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'@' | b'.' | b'_' | b'-'))
}

/// Valida el objetivo completo según su tipo.
pub fn validar_objetivo(objetivo: &Objetivo) -> Result<(), String> {
    let valido = match objetivo.tipo {
        TipoObjetivo::Unidad | TipoObjetivo::Timer => nombre_unidad_valido(&objetivo.nombre),
        TipoObjetivo::Contenedor => nombre_contenedor_valido(&objetivo.nombre),
        TipoObjetivo::Proyecto => nombre_proyecto_valido(&objetivo.nombre),
    };
    if valido {
        Ok(())
    } else {
        Err(format!(
            "«{}»: nombre no admitido para acciones",
            recortar(&objetivo.nombre, 80)
        ))
    }
}

/// Recorta un texto para meterlo en un mensaje (sin partir un carácter).
fn recortar(texto: &str, max_chars: usize) -> String {
    texto.chars().take(max_chars).collect()
}

/// ¿Es un comando de comprobación propia válido? Una línea de 1 a 1 024
/// caracteres, sin NUL ni saltos de línea; si empieza por `sudo`, se avisa
/// pero se acepta (§7.11).
pub fn validar_comando_propio(comando: &str) -> Result<(), String> {
    let caracteres = comando.chars().count();
    if comando.trim().is_empty() {
        return Err("la comprobación propia no puede estar vacía".to_string());
    }
    if caracteres > limites::COMPROBACION_MAX_CHARS {
        return Err(format!(
            "la comprobación propia no puede pasar de {} caracteres",
            limites::COMPROBACION_MAX_CHARS
        ));
    }
    if comando.chars().any(|caracter| caracter == '\0') {
        return Err("la comprobación propia no puede tener NUL".to_string());
    }
    if comando.contains('\n') || comando.contains('\r') {
        return Err("la comprobación propia no puede tener saltos de línea".to_string());
    }
    Ok(())
}

/// Aviso si la comprobación propia empieza por `sudo` (§7.11).
pub fn aviso_comando_propio(comando: &str) -> Option<String> {
    let comienzo = comando.trim_start();
    if comienzo == "sudo" || comienzo.starts_with("sudo ") || comienzo.starts_with("sudo\t") {
        Some(
            "la comprobación propia no necesita sudo: se ejecuta con los privilegios de la acción"
                .to_string(),
        )
    } else {
        None
    }
}

/// ¿Es una ruta de un fichero Compose (absoluta y con los caracteres que
/// admite §7.11)? Se usa con las etiquetas de los contenedores.
pub fn ruta_compose_valida(ruta: &str) -> bool {
    ruta.len() > 1
        && ruta.len() <= 4096
        && ruta.starts_with('/')
        && !ruta.contains("//")
        && !ruta.split('/').any(|segmento| segmento == "..")
        && ruta
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-'))
}

/// ¿Es una unidad crítica (nombre exacto o patrón con `*`) válido?
pub fn patron_critica_valido(patron: &str) -> bool {
    if patron.is_empty() || patron.len() > limites::MAX_NOMBRE {
        return false;
    }
    patron.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'@' | b'.' | b'_' | b'-' | b'*')
    })
}

/// ¿El nombre de unidad coincide con el patrón de crítica (con `*`)?
pub fn coincide_patron(patron: &str, nombre: &str) -> bool {
    if !patron.contains('*') {
        return patron == nombre;
    }
    let patron: Vec<char> = patron.chars().collect();
    let nombre: Vec<char> = nombre.chars().collect();
    let (mut ip, mut ino) = (0usize, 0usize);
    let mut estrella: Option<usize> = None;
    let mut tras_estrella = 0usize;
    while ino < nombre.len() {
        if ip < patron.len() && patron[ip] == nombre[ino] {
            ip += 1;
            ino += 1;
        } else if ip < patron.len() && patron[ip] == '*' {
            estrella = Some(ip);
            ip += 1;
            tras_estrella = ino;
        } else if let Some(posicion) = estrella {
            ip = posicion + 1;
            tras_estrella += 1;
            ino = tras_estrella;
        } else {
            return false;
        }
    }
    while ip < patron.len() && patron[ip] == '*' {
        ip += 1;
    }
    ip == patron.len()
}

/// Argumentos que siguen a `magi-servicios <suborden> <privilegio>`.
pub fn argumentos(suborden: &Suborden) -> Vec<String> {
    match suborden {
        Suborden::Privilegio | Suborden::Sudo => Vec::new(),
        Suborden::Listar { motores } => vec![motores.clone()],
        Suborden::Consumo { motor } => vec![motor.texto().to_string()],
        Suborden::Detalle(objetivo) => {
            vec![objetivo.tipo.texto().to_string(), objetivo.nombre.clone()]
        }
        Suborden::Accion {
            objetivo,
            accion,
            espera_seg,
            comprobacion,
        } => {
            let mut argumentos = vec![
                objetivo.tipo.texto().to_string(),
                objetivo.nombre.clone(),
                accion.texto().to_string(),
                espera_seg.to_string(),
                comprobacion.texto().to_string(),
            ];
            if let Comprobacion::Propia(orden) = comprobacion {
                argumentos.push(orden.clone());
            }
            argumentos
        }
        Suborden::Logs {
            objetivo,
            lineas_iniciales,
        } => vec![
            objetivo.tipo.texto().to_string(),
            objetivo.nombre.clone(),
            lineas_iniciales.to_string(),
        ],
    }
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
/// que pregunta en la propia pestaña. Con Podman nunca se usa sudo (§7.3).
///
/// Devuelve `None` si la orden trae un nombre que no se puede admitir.
pub fn comando_pestana(orden: &OrdenPestana, privilegio: Privilegio) -> Option<String> {
    let (base, usa_sudo) = match orden {
        OrdenPestana::ShellContenedor { motor, id } => {
            if !nombre_contenedor_valido(id) {
                return None;
            }
            (
                format!(
                    "{} exec -it {} sh -c {}",
                    motor.texto(),
                    entrecomillar(id),
                    entrecomillar(SHELL_CONTENEDOR)
                ),
                motor.usa_sudo(),
            )
        }
        OrdenPestana::SeguirDiario { unidad } => {
            if !nombre_unidad_valido(unidad) {
                return None;
            }
            (format!("journalctl -fu {}", entrecomillar(unidad)), true)
        }
        OrdenPestana::SeguirContenedor { motor, id } => {
            if !nombre_contenedor_valido(id) {
                return None;
            }
            (
                format!("{} logs -f {}", motor.texto(), entrecomillar(id)),
                motor.usa_sudo(),
            )
        }
    };
    if !usa_sudo {
        return Some(base);
    }
    Some(match privilegio {
        Privilegio::Root => base,
        Privilegio::SudoSinClave => format!("sudo -n -- {base}"),
        Privilegio::SudoConClave => format!("sudo -- {base}"),
        Privilegio::SinSudo => base,
    })
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

/// Valida que una lista de motores es la que admite el script
/// (`-`, `docker`, `podman`, `docker,podman` o `podman,docker`).
pub fn motores_validos(motores: &str) -> bool {
    matches!(
        motores,
        "-" | "docker" | "podman" | "docker,podman" | "podman,docker"
    )
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

/// Valida un suborden completo antes de mandarlo al host. Devuelve el motivo
/// si algún argumento no vale.
pub fn validar_suborden(suborden: &Suborden) -> Result<(), String> {
    match suborden {
        Suborden::Privilegio | Suborden::Sudo => Ok(()),
        Suborden::Listar { motores } => {
            if motores_validos(motores) {
                Ok(())
            } else {
                Err(format!("motores no admitidos: «{motores}»"))
            }
        }
        Suborden::Consumo { .. } => Ok(()),
        Suborden::Detalle(objetivo) => validar_objetivo(objetivo),
        Suborden::Accion {
            objetivo,
            espera_seg,
            comprobacion,
            ..
        } => {
            validar_objetivo(objetivo)?;
            if !(1..=600).contains(espera_seg) {
                return Err(format!("espera no admitida: {espera_seg}"));
            }
            if let Comprobacion::Propia(orden) = comprobacion {
                validar_comando_propio(orden)?;
            }
            Ok(())
        }
        Suborden::Logs {
            objetivo,
            lineas_iniciales,
        } => {
            validar_objetivo(objetivo)?;
            if *lineas_iniciales > 5000 {
                return Err(format!("líneas iniciales no admitidas: {lineas_iniciales}"));
            }
            Ok(())
        }
    }
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
