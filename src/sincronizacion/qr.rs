//! QR de la clave de sincronización pintado con medios bloques (Fase 10,
//! §6.2). Sin imagen ni dependencias de render: los módulos se convierten en
//! texto para que la TUI los pinte en blanco y negro.
//!
//! La clave `AGE-SECRET-KEY-1…` mide 74 caracteres; su QR es de 37×37 módulos
//! (45×45 con la zona quieta) y 23 líneas de texto, así que la vista que lo
//! muestra debe ser a pantalla completa o avisar de que no cabe.

use anyhow::Result;
use qrcode::{Color, QrCode};

/// Módulos de margen blanco por cada lado (norma de los QR normales).
pub const ZONA_QUIETA: usize = 4;

/// QR ya convertido a líneas de texto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QrClave {
    /// Columnas de cada línea.
    pub columnas: usize,
    /// Líneas de texto (cada una pinta dos filas de módulos).
    pub lineas: Vec<String>,
}

impl QrClave {
    /// Tamaño en celdas de terminal: (columnas, filas).
    pub fn tamano(&self) -> (usize, usize) {
        (self.columnas, self.lineas.len())
    }
}

/// Codifica un texto y devuelve sus líneas con medios bloques: █ para dos
/// módulos oscuros, ▀ y ▄ para uno, espacio para ninguno. Añade la zona quieta
/// de 4 módulos y una fila blanca de relleno si hace falta para completar el
/// último par de filas.
pub fn lineas(texto: &str) -> Result<QrClave> {
    if texto.is_empty() {
        anyhow::bail!("no hay texto que codificar en el QR");
    }
    let codigo = QrCode::new(texto.as_bytes())
        .map_err(|error| anyhow::anyhow!("codificando el QR: {error}"))?;
    let ancho = codigo.width();
    let modulos = codigo.to_colors();
    let columnas = ancho + 2 * ZONA_QUIETA;
    let alto = if columnas % 2 == 0 {
        columnas
    } else {
        columnas + 1
    };
    let oscuro = |x: usize, y: usize| -> bool {
        if x < ZONA_QUIETA || y < ZONA_QUIETA {
            return false;
        }
        let mx = x - ZONA_QUIETA;
        let my = y - ZONA_QUIETA;
        if mx >= ancho || my >= ancho {
            return false;
        }
        modulos[my * ancho + mx] == Color::Dark
    };
    let mut lineas = Vec::with_capacity(alto / 2);
    for y in (0..alto).step_by(2) {
        let mut linea = String::with_capacity(columnas);
        for x in 0..columnas {
            let arriba = oscuro(x, y);
            let abajo = oscuro(x, y + 1);
            linea.push(match (arriba, abajo) {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                (false, false) => ' ',
            });
        }
        lineas.push(linea);
    }
    Ok(QrClave { columnas, lineas })
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// Tamaño en celdas de un QR, sin depender de la matriz exacta.
    fn tamano_de(texto: &str) -> (usize, usize) {
        lineas(texto).unwrap().tamano()
    }

    #[test]
    fn el_qr_cabe_en_una_pantalla_80x24_y_lleva_zona_quieta() {
        // Una clave de age mide 74 caracteres.
        let clave = format!("AGE-SECRET-KEY-1{}", "Q".repeat(56));
        let qr = lineas(&clave).unwrap();
        let (columnas, filas) = qr.tamano();
        assert_eq!(columnas % 2, 1, "el ancho con zona quieta es impar");
        assert_eq!(filas, columnas.div_ceil(2));
        assert!(columnas <= 80, "no cabe a lo ancho en 80 columnas");
        assert!(
            qr.lineas[0].chars().all(|caracter| caracter == ' '),
            "la primera línea es zona quieta"
        );
        assert!(
            qr.lineas[filas - 1].chars().all(|caracter| caracter == ' '),
            "la última línea es zona quieta"
        );
        for linea in &qr.lineas {
            assert_eq!(linea.chars().count(), columnas);
            assert!(linea
                .chars()
                .all(|caracter| matches!(caracter, '█' | '▀' | '▄' | ' ')));
        }
        // El centro tiene módulos (el QR no es todo blanco).
        assert!(qr.lineas[filas / 2].chars().any(|caracter| caracter != ' '));
    }

    #[test]
    fn textos_distintos_dan_tamanos_coherentes_y_estables() {
        assert_eq!(tamano_de("a"), tamano_de("a"));
        assert_ne!(tamano_de("a"), tamano_de(&"b".repeat(200)));
        assert!(lineas("").is_err());
    }
}
