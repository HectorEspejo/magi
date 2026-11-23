//! `web/datos/plataformas.toml`: plataformas, canales, estados, artefactos y la
//! huella del certificado de la app.
//!
//! Se valida al cargar (informe §7.11): ids únicos, canales y estados conocidos,
//! un artefacto por plataforma, órdenes sin repetir y huella con 32 pares
//! hexadecimales en mayúsculas separados por `:` (o vacía si no hay app publicada).

use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{ErrorWeb, Resultado};
use crate::ordenes::texto_seguro;

/// Qué se publica por separado. Escritorio y Termux comparten la release `vX.Y.Z`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Canal {
    Escritorio,
    Termux,
    App,
}

impl Canal {
    pub const TODOS: [Canal; 3] = [Canal::Escritorio, Canal::Termux, Canal::App];

    /// `escritorio`, `termux` o `app`.
    pub fn id(self) -> &'static str {
        match self {
            Canal::Escritorio => "escritorio",
            Canal::Termux => "termux",
            Canal::App => "app",
        }
    }

    pub fn desde_id(id: &str) -> Option<Canal> {
        Canal::TODOS.into_iter().find(|c| c.id() == id)
    }

    /// Capítulo del manual con su instalación: `instalacion`, `termux` o `app-android`.
    pub fn capitulo(self) -> &'static str {
        match self {
            Canal::Escritorio => "instalacion",
            Canal::Termux => "termux",
            Canal::App => "app-android",
        }
    }

    /// CHANGELOG del canal (escritorio y Termux comparten el de la raíz).
    pub fn changelog(self) -> &'static str {
        match self {
            Canal::Escritorio | Canal::Termux => "CHANGELOG.md",
            Canal::App => "android/CHANGELOG.md",
        }
    }

    /// Prefijo de la etiqueta de la release del canal: `v` o `android-v`.
    pub fn prefijo_etiqueta(self) -> &'static str {
        match self {
            Canal::Escritorio | Canal::Termux => "v",
            Canal::App => "android-v",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstadoPlataforma {
    Estable,
    EnPruebas,
    Beta,
}

impl EstadoPlataforma {
    /// `estable`, `en_pruebas` o `beta` (clave de textos: `descargas.estado.<id>`).
    pub fn id(self) -> &'static str {
        match self {
            EstadoPlataforma::Estable => "estable",
            EstadoPlataforma::EnPruebas => "en_pruebas",
            EstadoPlataforma::Beta => "beta",
        }
    }

    pub fn desde_id(id: &str) -> Option<EstadoPlataforma> {
        [
            EstadoPlataforma::Estable,
            EstadoPlataforma::EnPruebas,
            EstadoPlataforma::Beta,
        ]
        .into_iter()
        .find(|e| e.id() == id)
    }
}

/// Una plataforma de descarga (`[[plataforma]]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Plataforma {
    /// `linux-x86_64`, `linux-arm64`, `macos-arm64`, `macos-x86_64`, `termux-arm64` o `android-arm64`.
    pub id: String,
    pub canal: Canal,
    pub estado: EstadoPlataforma,
    /// Nombre del fichero; `{version}` se sustituye por el número de la versión elegida.
    pub artefacto: String,
    /// Orden en la página: Linux, macOS, Termux y app.
    pub orden: u32,
    /// Clave de `textos` con su instalación corta.
    pub instrucciones: String,
}

impl Plataforma {
    /// Nombre del artefacto para la versión `numero` (sin la `v`).
    pub fn nombre_artefacto(&self, numero: &str) -> String {
        self.artefacto.replace("{version}", numero)
    }
}

/// `[app]`: lo propio de la app Android.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DatosApp {
    /// SHA-256 del certificado de firma en mayúsculas con `:`; vacía hasta que el
    /// desarrollador cree el keystore.
    pub huella_certificado: String,
    /// Filtro de etiquetas de Obtainium (`android-v`).
    pub obtainium_etiquetas: String,
    /// Filtro del APK de Obtainium (`magi-android-`).
    pub obtainium_apk: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Plataformas {
    pub app: DatosApp,
    /// Ordenadas por `orden`.
    pub plataforma: Vec<Plataforma>,
}

/// Las plataformas que conoce la web, cada una con su canal (informe §3).
const PLATAFORMAS_CONOCIDAS: [(&str, Canal); 6] = [
    ("linux-x86_64", Canal::Escritorio),
    ("linux-arm64", Canal::Escritorio),
    ("macos-arm64", Canal::Escritorio),
    ("macos-x86_64", Canal::Escritorio),
    ("termux-arm64", Canal::Termux),
    ("android-arm64", Canal::App),
];

/// Tope del fichero `plataformas.toml` (es un fichero del repositorio: sobra).
const TOPE_FICHERO: u64 = 1024 * 1024;

/// `[app]` tal como viene del TOML.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CrudoApp {
    huella_certificado: String,
    obtainium_etiquetas: String,
    obtainium_apk: String,
}

/// `[[plataforma]]` tal como viene del TOML: el canal y el estado van como texto
/// para que el error diga el id de la plataforma, no la posición en el fichero.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CrudaPlataforma {
    id: String,
    canal: String,
    estado: String,
    artefacto: String,
    orden: i64,
    instrucciones: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CrudoPlataformas {
    app: CrudoApp,
    #[serde(default)]
    plataforma: Vec<CrudaPlataforma>,
}

/// `true` si `huella` son 32 pares hexadecimales en mayúsculas separados por `:`
/// (`AA:BB:…`, 95 caracteres).
pub(crate) fn es_huella_valida(huella: &str) -> bool {
    let bytes = huella.as_bytes();
    if bytes.len() != 32 * 3 - 1 {
        return false;
    }
    bytes.iter().enumerate().all(|(i, b)| {
        if i % 3 == 2 {
            *b == b':'
        } else {
            b.is_ascii_digit() || (b'A'..=b'F').contains(b)
        }
    })
}

/// Comprueba que el nombre de artefacto es un nombre de fichero sencillo: letras,
/// cifras, `.`, `_`, `-` y, solo como `{version}`, llaves. Nada de rutas.
fn comprobar_nombre_artefacto(artefacto: &str) -> std::result::Result<(), String> {
    if artefacto.is_empty() {
        return Err("el artefacto está vacío".to_string());
    }
    if artefacto.len() > 200 {
        return Err("el artefacto pasa de 200 caracteres".to_string());
    }
    let sin_version = artefacto.replace("{version}", "");
    if let Some(c) = sin_version
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')))
    {
        return Err(format!(
            "el artefacto «{}» lleva el carácter «{}»: solo se admiten letras, cifras, «.», «_», «-» y «{{version}}»",
            texto_seguro(artefacto, 80),
            texto_seguro(&c.to_string(), 1)
        ));
    }
    if artefacto.contains("..") || sin_version.starts_with(['.', '-']) {
        return Err(format!(
            "el artefacto «{}» no es un nombre de fichero válido",
            texto_seguro(artefacto, 80)
        ));
    }
    Ok(())
}

impl Plataformas {
    /// Lee y valida `web/datos/plataformas.toml`.
    pub fn cargar(ruta: &Path) -> Resultado<Self> {
        let tamano = std::fs::metadata(ruta)
            .map_err(|e| ErrorWeb::lectura(ruta, e))?
            .len();
        if tamano > TOPE_FICHERO {
            return Err(ErrorWeb::dato_raro(
                ruta.display().to_string(),
                format!("el fichero pesa {tamano} bytes y el tope son {TOPE_FICHERO}"),
            ));
        }
        let texto = std::fs::read_to_string(ruta).map_err(|e| ErrorWeb::lectura(ruta, e))?;
        Self::desde_texto(&texto, &ruta.display().to_string())
    }

    /// Valida un texto TOML; `origen` es lo que sale en los mensajes de error.
    /// Un estado que no es `estable`, `en_pruebas` ni `beta` falla con el id de la plataforma.
    pub fn desde_texto(texto: &str, origen: &str) -> Resultado<Self> {
        let crudo: CrudoPlataformas = toml::from_str(texto)
            .map_err(|e| ErrorWeb::dato_raro(origen, format!("TOML no válido: {e}")))?;

        // [app]
        let app = crudo.app;
        if !app.huella_certificado.is_empty() && !es_huella_valida(&app.huella_certificado) {
            return Err(ErrorWeb::dato_raro(
                format!("{origen} ([app])"),
                format!(
                    "huella_certificado «{}» no son 32 pares hexadecimales en mayúsculas separados por «:» (o vacía si no hay app publicada)",
                    texto_seguro(&app.huella_certificado, 100)
                ),
            ));
        }
        for (campo, valor) in [
            ("obtainium_etiquetas", &app.obtainium_etiquetas),
            ("obtainium_apk", &app.obtainium_apk),
        ] {
            if valor.trim().is_empty() {
                return Err(ErrorWeb::dato_raro(
                    format!("{origen} ([app])"),
                    format!("{campo} está vacío"),
                ));
            }
        }

        if crudo.plataforma.is_empty() {
            return Err(ErrorWeb::dato_raro(origen, "no hay ninguna [[plataforma]]"));
        }

        // [[plataforma]]
        let mut ids: HashSet<String> = HashSet::new();
        let mut artefactos: Vec<(String, String)> = Vec::new();
        let mut ordenes: Vec<(u32, String)> = Vec::new();
        let mut plataforma: Vec<Plataforma> = Vec::new();
        for cruda in crudo.plataforma {
            let donde = format!("{origen} (plataforma {})", texto_seguro(&cruda.id, 40));
            let raro = |que: String| ErrorWeb::dato_raro(donde.clone(), que);

            let Some((_, canal_del_id)) =
                PLATAFORMAS_CONOCIDAS.iter().find(|(id, _)| *id == cruda.id)
            else {
                let conocidas: Vec<&str> =
                    PLATAFORMAS_CONOCIDAS.iter().map(|(id, _)| *id).collect();
                return Err(raro(format!(
                    "id desconocido (admitidos: {})",
                    conocidas.join(", ")
                )));
            };
            if !ids.insert(cruda.id.clone()) {
                return Err(raro("id repetido".to_string()));
            }
            let Some(canal) = Canal::desde_id(&cruda.canal) else {
                return Err(raro(format!(
                    "canal «{}» desconocido (escritorio, termux o app)",
                    texto_seguro(&cruda.canal, 40)
                )));
            };
            if canal != *canal_del_id {
                return Err(raro(format!(
                    "la plataforma es del canal «{}», no del «{}»",
                    canal_del_id.id(),
                    canal.id()
                )));
            }
            let Some(estado) = EstadoPlataforma::desde_id(&cruda.estado) else {
                return Err(raro(format!(
                    "estado «{}» desconocido (estable, en_pruebas o beta)",
                    texto_seguro(&cruda.estado, 40)
                )));
            };
            comprobar_nombre_artefacto(&cruda.artefacto).map_err(&raro)?;
            if canal != Canal::Escritorio && !cruda.artefacto.contains("{version}") {
                return Err(raro(format!(
                    "el artefacto «{}» no lleva «{{version}}» (obligatorio en Termux y en la app)",
                    texto_seguro(&cruda.artefacto, 80)
                )));
            }
            if let Some((otro, _)) = artefactos.iter().find(|(_, a)| *a == cruda.artefacto) {
                return Err(raro(format!(
                    "el artefacto «{}» ya es el de la plataforma {otro}",
                    texto_seguro(&cruda.artefacto, 80)
                )));
            }
            let Ok(orden) = u32::try_from(cruda.orden) else {
                return Err(raro(format!(
                    "orden {} fuera de rango (un entero de 0 en adelante)",
                    cruda.orden
                )));
            };
            if let Some((_, otro)) = ordenes.iter().find(|(o, _)| *o == orden) {
                return Err(raro(format!(
                    "el orden {orden} ya es el de la plataforma {otro}"
                )));
            }
            if cruda.instrucciones.trim().is_empty() {
                return Err(raro("instrucciones está vacía".to_string()));
            }

            artefactos.push((cruda.id.clone(), cruda.artefacto.clone()));
            ordenes.push((orden, cruda.id.clone()));
            plataforma.push(Plataforma {
                id: cruda.id,
                canal,
                estado,
                artefacto: cruda.artefacto,
                orden,
                instrucciones: cruda.instrucciones,
            });
        }
        plataforma.sort_by_key(|p| p.orden);

        Ok(Plataformas {
            app: DatosApp {
                huella_certificado: app.huella_certificado,
                obtainium_etiquetas: app.obtainium_etiquetas,
                obtainium_apk: app.obtainium_apk,
            },
            plataforma,
        })
    }

    pub fn por_id(&self, id: &str) -> Option<&Plataforma> {
        self.plataforma.iter().find(|p| p.id == id)
    }

    pub fn del_canal(&self, canal: Canal) -> Vec<&Plataforma> {
        self.plataforma
            .iter()
            .filter(|p| p.canal == canal)
            .collect()
    }
}
