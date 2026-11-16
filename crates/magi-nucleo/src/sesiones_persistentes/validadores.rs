//! Validadores de las sesiones persistentes (§7.7 del informe de la Fase 16):
//! nombre, carpeta, comando, comando de reanudación, nota y host. Los usan el
//! diálogo, el almacén y el servidor, y el script comprueba lo mismo en el
//! host. Pista B del Sprint 1.
//!
//! Cada validador devuelve un `ErrorFicha` que dice qué campo falla y por qué,
//! con un mensaje en castellano que el diálogo puede enseñar tal cual. Los
//! campos opcionales (carpeta, comando, reanudar y nota) aceptan el texto
//! vacío: significa «sin valor» (la carpeta personal, el shell de acceso...).
//!
//! El comando y el de reanudación son texto libre del usuario, como un
//! snippet: no se escapan ni se interpretan aquí, porque son el comando. Solo
//! se acotan.

use std::fmt;

use crate::escritorio::TipoHost;
use crate::sesiones_persistentes::DatosFicha;

/// Largo máximo del nombre de una sesión (el mismo que valida el script).
pub const MAXIMO_NOMBRE: usize = 64;
/// Bytes máximos de la carpeta.
pub const MAXIMO_CARPETA_BYTES: usize = 1024;
/// Bytes máximos del comando de arranque y del de reanudación.
pub const MAXIMO_COMANDO_BYTES: usize = 4096;
/// Caracteres máximos de la nota.
pub const MAXIMO_NOTA_CARACTERES: usize = 500;

/// Campo de la ficha al que se refiere un error, para que el diálogo marque
/// el que falla.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CampoFicha {
    Host,
    Nombre,
    Carpeta,
    Comando,
    ComandoReanudar,
    Nota,
    Equipo,
}

impl CampoFicha {
    /// Etiqueta del campo tal como la enseña el diálogo.
    #[must_use]
    pub fn etiqueta(self) -> &'static str {
        match self {
            CampoFicha::Host => "Host",
            CampoFicha::Nombre => "Nombre",
            CampoFicha::Carpeta => "Carpeta",
            CampoFicha::Comando => "Comando",
            CampoFicha::ComandoReanudar => "Reanudar",
            CampoFicha::Nota => "Nota",
            CampoFicha::Equipo => "Equipo",
        }
    }

    /// Cómo se nombra el campo dentro de una frase («el comando de
    /// reanudación»).
    fn en_frase(self) -> &'static str {
        match self {
            CampoFicha::Host => "el host",
            CampoFicha::Nombre => "el nombre",
            CampoFicha::Carpeta => "la carpeta",
            CampoFicha::Comando => "el comando",
            CampoFicha::ComandoReanudar => "el comando de reanudación",
            CampoFicha::Nota => "la nota",
            CampoFicha::Equipo => "el equipo",
        }
    }
}

/// Por qué una ficha de sesión persistente no es válida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorFicha {
    NombreVacio,
    /// El nombre lleva un carácter fuera de `[A-Za-z0-9_-]`.
    NombreCaracter {
        caracter: char,
    },
    NombreLargo {
        largo: usize,
    },
    CarpetaConNulo,
    CarpetaConSaltoDeLinea,
    CarpetaLarga {
        bytes: usize,
    },
    CarpetaNoAbsoluta,
    /// El comando (de arranque o de reanudación) pasa de 4096 bytes.
    ComandoLargo {
        campo: CampoFicha,
        bytes: usize,
    },
    ComandoConNulo {
        campo: CampoFicha,
    },
    NotaLarga {
        caracteres: usize,
    },
    /// Falta el nombre del equipo que crea la ficha.
    EquipoVacio,
    /// Solo los hosts SSH tienen sesiones persistentes (T100).
    HostNoSsh {
        tipo: TipoHost,
    },
    /// Errores que solo conoce el almacén: el host no existe, el nombre ya lo
    /// usa otra ficha del host o la ficha no existe.
    HostNoExiste {
        host_id: i64,
    },
    NombreEnUso {
        nombre: String,
    },
    FichaNoExiste {
        id: i64,
    },
}

impl ErrorFicha {
    /// Campo del diálogo al que corresponde el error.
    #[must_use]
    pub fn campo(&self) -> CampoFicha {
        match self {
            ErrorFicha::NombreVacio
            | ErrorFicha::NombreCaracter { .. }
            | ErrorFicha::NombreLargo { .. }
            | ErrorFicha::NombreEnUso { .. }
            | ErrorFicha::FichaNoExiste { .. } => CampoFicha::Nombre,
            ErrorFicha::CarpetaConNulo
            | ErrorFicha::CarpetaConSaltoDeLinea
            | ErrorFicha::CarpetaLarga { .. }
            | ErrorFicha::CarpetaNoAbsoluta => CampoFicha::Carpeta,
            ErrorFicha::ComandoLargo { campo, .. } | ErrorFicha::ComandoConNulo { campo } => *campo,
            ErrorFicha::NotaLarga { .. } => CampoFicha::Nota,
            ErrorFicha::EquipoVacio => CampoFicha::Equipo,
            ErrorFicha::HostNoSsh { .. } | ErrorFicha::HostNoExiste { .. } => CampoFicha::Host,
        }
    }
}

impl fmt::Display for ErrorFicha {
    fn fmt(&self, formato: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorFicha::NombreVacio => write!(formato, "el nombre no puede estar vacío"),
            ErrorFicha::NombreCaracter { caracter } if caracter.is_whitespace() => {
                write!(formato, "el nombre no puede llevar espacios")
            }
            ErrorFicha::NombreCaracter { caracter } if caracter.is_control() => {
                write!(formato, "el nombre lleva un carácter de control")
            }
            ErrorFicha::NombreCaracter { caracter } => write!(
                formato,
                "el nombre solo admite letras sin tilde, números, guion y guion bajo \
                 (sobra «{caracter}»)"
            ),
            ErrorFicha::NombreLargo { largo } => write!(
                formato,
                "el nombre no puede pasar de {MAXIMO_NOMBRE} caracteres (tiene {largo})"
            ),
            ErrorFicha::CarpetaConNulo => {
                write!(formato, "la carpeta no puede llevar bytes nulos")
            }
            ErrorFicha::CarpetaConSaltoDeLinea => {
                write!(formato, "la carpeta no puede llevar saltos de línea")
            }
            ErrorFicha::CarpetaLarga { bytes } => write!(
                formato,
                "la carpeta no puede pasar de {MAXIMO_CARPETA_BYTES} bytes (tiene {bytes})"
            ),
            ErrorFicha::CarpetaNoAbsoluta => write!(
                formato,
                "la carpeta debe ser una ruta absoluta (/…) o empezar por ~/"
            ),
            ErrorFicha::ComandoLargo { campo, bytes } => write!(
                formato,
                "{} no puede pasar de {MAXIMO_COMANDO_BYTES} bytes (tiene {bytes})",
                mayuscula_inicial(campo.en_frase())
            ),
            ErrorFicha::ComandoConNulo { campo } => write!(
                formato,
                "{} no puede llevar bytes nulos",
                mayuscula_inicial(campo.en_frase())
            ),
            ErrorFicha::NotaLarga { caracteres } => write!(
                formato,
                "la nota no puede pasar de {MAXIMO_NOTA_CARACTERES} caracteres (tiene {caracteres})"
            ),
            ErrorFicha::EquipoVacio => {
                write!(formato, "falta el nombre del equipo que crea la sesión")
            }
            ErrorFicha::HostNoSsh { tipo } => write!(
                formato,
                "solo los hosts SSH tienen sesiones persistentes (este es «{}»)",
                tipo.etiqueta()
            ),
            ErrorFicha::HostNoExiste { host_id } => {
                write!(formato, "el host {host_id} no existe")
            }
            ErrorFicha::NombreEnUso { nombre } => write!(
                formato,
                "ya hay una sesión persistente con el nombre «{nombre}» en este host"
            ),
            ErrorFicha::FichaNoExiste { id } => {
                write!(formato, "la ficha de sesión {id} no existe")
            }
        }
    }
}

impl std::error::Error for ErrorFicha {}

/// Primera letra en mayúscula, para empezar una frase con «el comando…».
fn mayuscula_inicial(texto: &str) -> String {
    let mut letras = texto.chars();
    match letras.next() {
        Some(primera) => primera.to_uppercase().chain(letras).collect(),
        None => String::new(),
    }
}

/// `true` si el nombre cumple `^[A-Za-z0-9_-]{1,64}$`: lo mismo que valida el
/// script en el host.
#[must_use]
pub fn nombre_valido(nombre: &str) -> bool {
    validar_nombre(nombre).is_ok()
}

/// Nombre de la sesión en el tmux de MAGI: `^[A-Za-z0-9_-]{1,64}$`. No mira si
/// está libre: eso lo hace el almacén (entre las fichas del host) y el script
/// (en el host).
pub fn validar_nombre(nombre: &str) -> Result<(), ErrorFicha> {
    if nombre.is_empty() {
        return Err(ErrorFicha::NombreVacio);
    }
    if let Some(caracter) = nombre
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || *c == '-' || *c == '_'))
    {
        return Err(ErrorFicha::NombreCaracter { caracter });
    }
    // Todo ASCII a partir de aquí: bytes y caracteres coinciden.
    if nombre.len() > MAXIMO_NOMBRE {
        return Err(ErrorFicha::NombreLargo {
            largo: nombre.len(),
        });
    }
    Ok(())
}

/// Carpeta del host donde arranca la sesión: absoluta o `~/…`, hasta 1024
/// bytes, sin saltos de línea ni bytes nulos. Vacía vale: la carpeta personal.
/// El script comprueba además que existe.
pub fn validar_carpeta(carpeta: &str) -> Result<(), ErrorFicha> {
    if carpeta.is_empty() {
        return Ok(());
    }
    if carpeta.contains('\0') {
        return Err(ErrorFicha::CarpetaConNulo);
    }
    if carpeta.contains(['\n', '\r']) {
        return Err(ErrorFicha::CarpetaConSaltoDeLinea);
    }
    if carpeta.len() > MAXIMO_CARPETA_BYTES {
        return Err(ErrorFicha::CarpetaLarga {
            bytes: carpeta.len(),
        });
    }
    if !(carpeta.starts_with('/') || carpeta.starts_with("~/")) {
        return Err(ErrorFicha::CarpetaNoAbsoluta);
    }
    Ok(())
}

/// Comando de arranque: texto libre de hasta 4096 bytes, sin bytes nulos. Vacío
/// vale: el shell de acceso.
pub fn validar_comando(comando: &str) -> Result<(), ErrorFicha> {
    validar_texto_de_comando(CampoFicha::Comando, comando)
}

/// Comando que se usa al reabrir una sesión terminada (por ejemplo `claude
/// --continue`): mismas reglas que el de arranque. Vacío vale: se usa el de
/// arranque.
pub fn validar_comando_reanudar(comando: &str) -> Result<(), ErrorFicha> {
    validar_texto_de_comando(CampoFicha::ComandoReanudar, comando)
}

fn validar_texto_de_comando(campo: CampoFicha, comando: &str) -> Result<(), ErrorFicha> {
    if comando.contains('\0') {
        return Err(ErrorFicha::ComandoConNulo { campo });
    }
    if comando.len() > MAXIMO_COMANDO_BYTES {
        return Err(ErrorFicha::ComandoLargo {
            campo,
            bytes: comando.len(),
        });
    }
    Ok(())
}

/// Nota libre de hasta 500 caracteres.
pub fn validar_nota(nota: &str) -> Result<(), ErrorFicha> {
    let caracteres = nota.chars().count();
    if caracteres > MAXIMO_NOTA_CARACTERES {
        return Err(ErrorFicha::NotaLarga { caracteres });
    }
    Ok(())
}

/// Solo los hosts de tipo `ssh` tienen sesiones persistentes (T100).
pub fn validar_host(tipo: TipoHost) -> Result<(), ErrorFicha> {
    if tipo == TipoHost::Ssh {
        Ok(())
    } else {
        Err(ErrorFicha::HostNoSsh { tipo })
    }
}

/// Recorta un campo opcional y convierte el vacío en `None`.
#[must_use]
pub fn normalizar_opcional(valor: Option<&str>) -> Option<String> {
    valor
        .map(str::trim)
        .filter(|texto| !texto.is_empty())
        .map(str::to_string)
}

/// Valida los datos de una ficha y los devuelve normalizados: nombre y equipo
/// recortados, y los campos opcionales recortados y a `None` si quedan
/// vacíos. Devuelve el primer error, en el orden del diálogo.
pub fn validar_datos(datos: &DatosFicha) -> Result<DatosFicha, ErrorFicha> {
    let nombre = datos.nombre.trim().to_string();
    validar_nombre(&nombre)?;
    let carpeta = normalizar_opcional(datos.carpeta.as_deref());
    validar_carpeta(carpeta.as_deref().unwrap_or(""))?;
    let comando = normalizar_opcional(datos.comando.as_deref());
    validar_comando(comando.as_deref().unwrap_or(""))?;
    let comando_reanudar = normalizar_opcional(datos.comando_reanudar.as_deref());
    validar_comando_reanudar(comando_reanudar.as_deref().unwrap_or(""))?;
    let nota = normalizar_opcional(datos.nota.as_deref());
    validar_nota(nota.as_deref().unwrap_or(""))?;
    let creada_por = datos.creada_por.trim().to_string();
    if creada_por.is_empty() {
        return Err(ErrorFicha::EquipoVacio);
    }
    Ok(DatosFicha {
        nombre,
        carpeta,
        comando,
        comando_reanudar,
        nota,
        origen: datos.origen,
        creada_por,
    })
}
