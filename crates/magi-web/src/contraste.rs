//! Contraste WCAG (informe §7.9): la prueba de `magi-web` exige 4,5:1 o más en
//! texto y 3:1 o más en bordes de controles y foco.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// `#RRGGBB` o `#RGB` (en cualquier caja) a `Rgb`.
///
/// Exige la almohadilla y solo dígitos hexadecimales: ni espacios, ni signos
/// (`u8::from_str_radix` aceptaría un `+`), ni otras longitudes.
pub fn parsear_hex(texto: &str) -> Option<Rgb> {
    let hex = texto.strip_prefix('#')?;
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let digito = |posicion: usize| u8::from_str_radix(hex.get(posicion..posicion + 1)?, 16).ok();
    let par = |posicion: usize| u8::from_str_radix(hex.get(posicion..posicion + 2)?, 16).ok();
    match hex.len() {
        6 => Some(Rgb {
            r: par(0)?,
            g: par(2)?,
            b: par(4)?,
        }),
        3 => Some(Rgb {
            r: digito(0)? * 17,
            g: digito(1)? * 17,
            b: digito(2)? * 17,
        }),
        _ => None,
    }
}

/// Un canal sRGB (0 a 255) pasado a luz lineal, como dice WCAG 2.x.
fn canal_lineal(valor: u8) -> f64 {
    let c = f64::from(valor) / 255.0;
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Luminancia relativa (WCAG 2.x).
pub fn luminancia(color: Rgb) -> f64 {
    0.2126 * canal_lineal(color.r) + 0.7152 * canal_lineal(color.g) + 0.0722 * canal_lineal(color.b)
}

/// Relación de contraste entre dos colores, siempre ≥ 1.
pub fn contraste(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (luminancia(a), luminancia(b));
    let (claro, oscuro) = if la >= lb { (la, lb) } else { (lb, la) };
    (claro + 0.05) / (oscuro + 0.05)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn aproximado(a: f64, b: f64, margen: f64) -> bool {
        (a - b).abs() <= margen
    }

    #[test]
    fn parsea_los_dos_formatos_en_cualquier_caja() {
        assert_eq!(
            parsear_hex("#FFA000"),
            Some(Rgb {
                r: 255,
                g: 160,
                b: 0
            })
        );
        assert_eq!(
            parsear_hex("#ffa000"),
            Some(Rgb {
                r: 255,
                g: 160,
                b: 0
            })
        );
        assert_eq!(
            parsear_hex("#fA0"),
            Some(Rgb {
                r: 255,
                g: 170,
                b: 0
            })
        );
    }

    #[test]
    fn rechaza_lo_que_no_es_un_color_sin_entrar_en_panico() {
        for malo in [
            "", "#", "FFA000", "#FFA00", "#FFA0000", "#GGGGGG", "#+FA000", "#ffa00ñ", "#ñññ",
            " #FFA000", "#FFA000 ", "#-1-1-1", "#12", "#1234",
        ] {
            assert_eq!(parsear_hex(malo), None, "{malo:?} no es un color");
        }
    }

    #[test]
    fn la_luminancia_de_los_extremos() {
        assert!(aproximado(luminancia(Rgb { r: 0, g: 0, b: 0 }), 0.0, 1e-12));
        assert!(aproximado(
            luminancia(Rgb {
                r: 255,
                g: 255,
                b: 255
            }),
            1.0,
            1e-9
        ));
    }

    #[test]
    fn el_contraste_es_simetrico_y_no_baja_de_uno() {
        let a = parsear_hex("#8A5000").unwrap();
        let b = parsear_hex("#F4F0E6").unwrap();
        assert_eq!(contraste(a, b), contraste(b, a));
        assert!(aproximado(contraste(a, a), 1.0, 1e-12));
    }
}
