use std::fs;
use std::path::Path;

use ratatui::style::Color;
use ratatui::symbols::border;

use crate::config::Config;

/// Paleta fija de respaldo del informe de diseño.
pub const FONDO: Color = Color::Rgb(0x0A, 0x0A, 0x0A);
pub const TEXTO: Color = Color::Rgb(0xED, 0xE8, 0xDC);
pub const ACENTO: Color = Color::Rgb(0xFF, 0xA0, 0x00);
pub const CORRECTO: Color = Color::Rgb(0x00, 0xFF, 0x6A);
pub const CRITICO: Color = Color::Rgb(0xFF, 0x2A, 0x1A);
pub const INACTIVO: Color = Color::Rgb(0x5A, 0x5A, 0x5A);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Paleta {
    pub fondo: Color,
    pub texto: Color,
    pub acento: Color,
    pub correcto: Color,
    pub critico: Color,
    pub inactivo: Color,
}

impl Paleta {
    pub fn respaldo() -> Self {
        Self {
            fondo: FONDO,
            texto: TEXTO,
            acento: ACENTO,
            correcto: CORRECTO,
            critico: CRITICO,
            inactivo: INACTIVO,
        }
    }
}

/// Glifos del sistema visual. En modo ASCII todos tienen equivalente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glifos {
    pub conectado: &'static str,
    pub conectando: &'static str,
    pub desconectado: &'static str,
    pub error: &'static str,
    pub plegado: &'static str,
    pub desplegado: &'static str,
    pub salto: &'static str,
    pub marca: &'static str,
}

impl Glifos {
    pub fn unicode() -> Self {
        Self {
            conectado: "●",
            conectando: "◐",
            desconectado: "○",
            error: "✕",
            plegado: "▶",
            desplegado: "▼",
            salto: "⤴",
            marca: "•",
        }
    }

    pub fn ascii() -> Self {
        Self {
            conectado: "*",
            conectando: "o",
            desconectado: "o",
            error: "x",
            plegado: ">",
            desplegado: "v",
            salto: ">",
            marca: "*",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tema {
    pub paleta: Paleta,
    pub glifos: Glifos,
    pub ascii: bool,
}

impl Tema {
    pub fn respaldo() -> Self {
        Self {
            paleta: Paleta::respaldo(),
            glifos: Glifos::unicode(),
            ascii: false,
        }
    }

    pub fn bordes(&self) -> border::Set<'static> {
        if self.ascii {
            border::Set {
                top_left: "+",
                top_right: "+",
                bottom_left: "+",
                bottom_right: "+",
                horizontal_top: "-",
                horizontal_bottom: "-",
                vertical_left: "|",
                vertical_right: "|",
            }
        } else {
            border::PLAIN
        }
    }
}

/// Carga el tema según `config.tema`: `auto` detecta Omarchy, `fijo` usa la
/// paleta de respaldo y cualquier otro valor se interpreta como ruta.
pub fn cargar(config: &Config, ruta_omarchy: &Path) -> (Tema, Option<String>) {
    let ascii = ascii_por_entorno(config);
    let glifos = if ascii {
        Glifos::ascii()
    } else {
        Glifos::unicode()
    };
    let (paleta, aviso) = match config.tema.trim() {
        "fijo" => (Paleta::respaldo(), None),
        "auto" => match leer_paleta(ruta_omarchy) {
            Ok(paleta) => (paleta, None),
            Err(motivo) => (Paleta::respaldo(), Some(motivo)),
        },
        ruta => match leer_paleta(Path::new(ruta)) {
            Ok(paleta) => (paleta, None),
            Err(motivo) => (
                Paleta::respaldo(),
                Some(format!("tema «{ruta}» no utilizable: {motivo}")),
            ),
        },
    };
    (
        Tema {
            paleta,
            glifos,
            ascii,
        },
        aviso,
    )
}

fn leer_paleta(ruta: &Path) -> Result<Paleta, String> {
    let texto = fs::read_to_string(ruta).map_err(|error| error.to_string())?;
    paleta_desde_toml(&texto)
}

/// Lee los colores del tema de Alacritty de Omarchy.
pub fn paleta_desde_toml(texto: &str) -> Result<Paleta, String> {
    let valor: toml::Value = toml::from_str(texto).map_err(|error| error.to_string())?;
    let colores = valor
        .get("colors")
        .ok_or_else(|| "falta la sección [colors]".to_string())?;
    let primario = colores
        .get("primary")
        .ok_or_else(|| "falta [colors.primary]".to_string())?;
    let normal = colores.get("normal");
    let bright = colores.get("bright");
    let fondo = primario
        .get("background")
        .and_then(color_de_valor)
        .ok_or_else(|| "falta colors.primary.background".to_string())?;
    let texto_color = primario
        .get("foreground")
        .and_then(color_de_valor)
        .ok_or_else(|| "falta colors.primary.foreground".to_string())?;
    let amarillo = normal
        .and_then(|n| n.get("yellow"))
        .and_then(color_de_valor)
        .unwrap_or(ACENTO);
    let verde = normal
        .and_then(|n| n.get("green"))
        .and_then(color_de_valor)
        .unwrap_or(CORRECTO);
    let rojo = normal
        .and_then(|n| n.get("red"))
        .and_then(color_de_valor)
        .unwrap_or(CRITICO);
    let negro_brillante = bright
        .and_then(|b| b.get("black"))
        .and_then(color_de_valor)
        .unwrap_or(INACTIVO);
    Ok(Paleta {
        fondo,
        texto: texto_color,
        acento: amarillo,
        correcto: verde,
        critico: rojo,
        inactivo: negro_brillante,
    })
}

/// Modo degradado si el locale no es UTF-8, si `terminal_ascii` está activo
/// o si se exporta `MAGI_ASCII=1`.
pub fn ascii_por_entorno(config: &Config) -> bool {
    if config.terminal_ascii {
        return true;
    }
    if let Ok(valor) = std::env::var("MAGI_ASCII") {
        if valor == "1" || valor.eq_ignore_ascii_case("true") {
            return true;
        }
    }
    !locale_es_utf8()
}

fn locale_es_utf8() -> bool {
    let variables = ["LC_ALL", "LC_CTYPE", "LANG"];
    for variable in variables {
        if let Ok(valor) = std::env::var(variable) {
            if valor.is_empty() {
                continue;
            }
            let valor = valor.to_lowercase();
            return valor.contains("utf-8") || valor.contains("utf8");
        }
    }
    // Sin locale definido se asume UTF-8 (comportamiento habitual en macOS).
    true
}

fn color_de_valor(valor: &toml::Value) -> Option<Color> {
    let texto = valor.as_str()?;
    let texto = texto.trim();
    if let Some(hex) = texto.strip_prefix('#') {
        return color_hex(hex);
    }
    Some(match texto.to_lowercase().as_str() {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "white" => Color::White,
        _ => return None,
    })
}

fn color_hex(hex: &str) -> Option<Color> {
    let expandido;
    let hex = match hex.len() {
        3 => {
            expandido = hex.chars().flat_map(|c| [c, c]).collect::<String>();
            expandido.as_str()
        }
        6 => hex,
        _ => return None,
    };
    let rojo = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let verde = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let azul = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color::Rgb(rojo, verde, azul))
}
