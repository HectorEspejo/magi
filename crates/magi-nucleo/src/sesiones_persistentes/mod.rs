//! Sesiones persistentes (Fase 16): sesiones con nombre que viven en un tmux
//! propio de MAGI en el host y sobreviven a MAGI y al equipo.
//!
//! Aquí vive lo común al escritorio y, más adelante, al móvil: los tipos, los
//! validadores, los nombres automáticos, el estado derivado de cada ficha, la
//! unión de fichas y la lectura de lo que devuelve el script. Nada de este
//! módulo ejecuta órdenes ni abre conexiones: el vigilante, la vista previa y
//! renombrar viven en `src/servidor/sesiones_persistentes.rs`.
//!
//! El host es la fuente de verdad: las fichas (`SESIONES_PERSISTENTES`) viajan
//! con la sincronización; el estado de una sesión y su contenido no, y solo
//! viven en memoria.

pub mod estado;
pub mod lectura;
pub mod nombres;
pub mod union;
pub mod validadores;

use serde::{Deserialize, Serialize};

/// Cómo nació una ficha (`SESIONES_PERSISTENTES.origen`). Se valida en código,
/// sin `CHECK` en SQLite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrigenFicha {
    /// Marcada al abrir una pestaña desde el diálogo.
    Manual,
    /// Pestaña de un host con `sesiones_persistentes = 1`.
    Host,
    /// Añadida desde la lista (una sesión del tmux de MAGI que no tenía ficha).
    Descubierta,
}

impl OrigenFicha {
    pub fn texto(self) -> &'static str {
        match self {
            OrigenFicha::Manual => "manual",
            OrigenFicha::Host => "host",
            OrigenFicha::Descubierta => "descubierta",
        }
    }

    pub fn desde_texto(texto: &str) -> Option<Self> {
        match texto {
            "manual" => Some(OrigenFicha::Manual),
            "host" => Some(OrigenFicha::Host),
            "descubierta" => Some(OrigenFicha::Descubierta),
            _ => None,
        }
    }
}

/// Qué servidor de tmux del host: el propio de MAGI (`tmux -L magi -f
/// /dev/null`) o el habitual del usuario, que solo se ve, se engancha y se
/// termina.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServidorTmux {
    Magi,
    Habitual,
}

impl ServidorTmux {
    pub fn texto(self) -> &'static str {
        match self {
            ServidorTmux::Magi => "magi",
            ServidorTmux::Habitual => "habitual",
        }
    }

    pub fn desde_texto(texto: &str) -> Option<Self> {
        match texto {
            "magi" => Some(ServidorTmux::Magi),
            "habitual" => Some(ServidorTmux::Habitual),
            _ => None,
        }
    }
}

/// Ficha de una sesión persistente: una fila de `SESIONES_PERSISTENTES`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ficha {
    pub id: i64,
    pub uuid: String,
    pub host_id: i64,
    /// Nombre de la sesión en el tmux de MAGI, único por host.
    pub nombre: String,
    /// Carpeta del host donde arranca (absoluta o `~/…`); `None`, la personal.
    pub carpeta: Option<String>,
    /// Comando de arranque (texto libre, como un snippet). Nunca va a REGISTRO.
    pub comando: Option<String>,
    /// Comando al reabrir una terminada; `None`, el de arranque.
    pub comando_reanudar: Option<String>,
    pub nota: Option<String>,
    pub origen: OrigenFicha,
    /// Etiqueta del equipo que la creó.
    pub creada_por: String,
    pub creada_en: String,
    pub actualizado_en: String,
}

/// Lo editable de una ficha, sin id, uuid ni fechas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatosFicha {
    pub nombre: String,
    pub carpeta: Option<String>,
    pub comando: Option<String>,
    pub comando_reanudar: Option<String>,
    pub nota: Option<String>,
    pub origen: OrigenFicha,
    pub creada_por: String,
}

/// Versión de tmux (`3.4`, `3.3a` → 3.3). La letra y los prefijos `next-` o
/// `master` no cuentan para comparar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct VersionTmux {
    pub mayor: u32,
    pub menor: u32,
}

impl VersionTmux {
    /// Mínima para el servidor propio (D331).
    pub const MINIMA: VersionTmux = VersionTmux { mayor: 3, menor: 0 };

    pub fn nueva(mayor: u32, menor: u32) -> Self {
        VersionTmux { mayor, menor }
    }
}

/// Una sesión de tmux tal como la lista `estado`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SesionVista {
    pub servidor: ServidorTmux,
    pub nombre: String,
    /// Identificador interno de tmux (`$3`): no cambia si otro equipo la
    /// renombra y no se repite mientras viva el servidor de tmux. Con él,
    /// MAGI sigue «la misma» sesión tras un renombrado (R157). `None` si el host
    /// no lo dio o no es un identificador válido.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Clientes enganchados ahora.
    pub clientes: u32,
    /// Etiqueta del último equipo que se enganchó (`@magi_equipo`); `None` si
    /// no consta o no es una etiqueta válida (se ve como «?»).
    pub equipo: Option<String>,
    /// Segundos Unix (hora del host) de la última actividad de sus ventanas.
    pub actividad: i64,
    /// Segundos Unix (hora del host) de la creación.
    pub creada: i64,
    /// Alguna ventana tiene la campana sin mirar: es la atención (D335).
    pub campana: bool,
    pub ventanas: u32,
    /// Título del programa, saneado y de hasta 120 caracteres.
    pub titulo: Option<String>,
}

/// Por qué no se pudo mirar un host (nunca equivale a «terminada»).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorComprobacion {
    /// El host no responde o la conexión se cayó.
    SinRespuesta,
    /// Pediría un diálogo (huella, frase, contraseña): el vigilante no abre.
    NecesitaDialogo,
    /// Pasó el plazo de 10 s.
    Plazo,
}

/// Resultado de comprobar un host con `estado`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComprobacionHost {
    pub host_id: i64,
    /// Versión de tmux; `None` si el host no tiene tmux.
    pub tmux: Option<VersionTmux>,
    /// Hora del host (segundos Unix) al comprobar, para comparar la actividad
    /// sin depender de que los relojes coincidan.
    pub ahora: i64,
    pub sesiones: Vec<SesionVista>,
    /// `Some` si no se pudo mirar; entonces `tmux` y `sesiones` no valen.
    pub error: Option<ErrorComprobacion>,
}

/// Estado derivado de una ficha (§7.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstadoFicha {
    VivaLibre,
    VivaEnganchada,
    Terminada,
    /// Aún no se ha comprobado su host.
    SinComprobar,
    /// El host no respondió: nunca se da por terminada.
    Inalcanzable,
    /// El host no tiene tmux o es anterior a 3.0.
    SinTmux,
}

impl EstadoFicha {
    pub fn texto(self) -> &'static str {
        match self {
            EstadoFicha::VivaLibre => "viva_libre",
            EstadoFicha::VivaEnganchada => "viva_enganchada",
            EstadoFicha::Terminada => "terminada",
            EstadoFicha::SinComprobar => "sin_comprobar",
            EstadoFicha::Inalcanzable => "inalcanzable",
            EstadoFicha::SinTmux => "sin_tmux",
        }
    }
}
