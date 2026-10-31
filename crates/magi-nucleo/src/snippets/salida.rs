//! Salida de una ejecución tal como se enseña y se guarda: texto plano sin
//! secuencias de escape (nunca se interpretan al pintar) y el fichero que
//! escribe `s` en la vista Resultados.

use std::path::Path;

/// Ancho de tabulador al pintar.
const ANCHO_TAB: usize = 4;

/// Quita las secuencias de escape (CSI, OSC, DCS y demás) y los caracteres de
/// control de un texto; conserva `\n` y convierte `\t` en espacios. Un `\r`
/// suelto (barras de progreso) sobrescribe la línea desde el principio: se
/// queda lo último que se escribió.
pub fn texto_limpio(bytes: &[u8]) -> String {
    let texto = String::from_utf8_lossy(bytes);
    let mut limpio = String::with_capacity(texto.len());
    let mut linea = String::new();
    let mut caracteres = texto.chars().peekable();
    while let Some(c) = caracteres.next() {
        match c {
            '\u{1b}' => saltar_escape(&mut caracteres),
            // CSI de 8 bits.
            '\u{9b}' => saltar_hasta_final_csi(&mut caracteres),
            '\n' => {
                limpio.push_str(&linea);
                limpio.push('\n');
                linea.clear();
            }
            '\r' => {
                if caracteres.peek() == Some(&'\n') {
                    continue;
                }
                linea.clear();
            }
            '\t' => {
                let columna = linea.chars().count();
                let espacios = ANCHO_TAB - columna % ANCHO_TAB;
                linea.extend(std::iter::repeat_n(' ', espacios));
            }
            c if c.is_control() => {}
            c => linea.push(c),
        }
    }
    limpio.push_str(&linea);
    limpio
}

/// Líneas limpias de una salida (sin la vacía final de un `\n` de cierre).
pub fn lineas_limpias(bytes: &[u8]) -> Vec<String> {
    let texto = texto_limpio(bytes);
    let mut lineas: Vec<String> = texto.split('\n').map(str::to_string).collect();
    if lineas.last().is_some_and(String::is_empty) {
        lineas.pop();
    }
    lineas
}

/// Una línea saneada y recortada, para veredictos y detalles (nombres de
/// fichero remotos, la última línea de stderr de un comando local).
pub fn sanear_linea(texto: &str, maximo: usize) -> String {
    let limpio = texto_limpio(texto.as_bytes());
    let linea = limpio.lines().next_back().unwrap_or("").trim().to_string();
    if linea.chars().count() <= maximo {
        return linea;
    }
    let recortada: String = linea.chars().take(maximo.saturating_sub(1)).collect();
    format!("{recortada}…")
}

fn saltar_escape(caracteres: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    match caracteres.next() {
        Some('[') => saltar_hasta_final_csi(caracteres),
        // OSC, DCS, SOS, PM y APC: hasta BEL o ST (`ESC \`).
        Some(']') | Some('P') | Some('X') | Some('^') | Some('_') => {
            while let Some(c) = caracteres.next() {
                if c == '\u{7}' {
                    break;
                }
                if c == '\u{1b}' {
                    if caracteres.peek() == Some(&'\\') {
                        caracteres.next();
                    }
                    break;
                }
            }
        }
        // Designación de juego de caracteres: un carácter más.
        Some('(') | Some(')') | Some('*') | Some('+') => {
            caracteres.next();
        }
        // Cualquier otra secuencia de dos caracteres (`ESC =`, `ESC M`…).
        _ => {}
    }
}

fn saltar_hasta_final_csi(caracteres: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    for c in caracteres.by_ref() {
        if ('\u{40}'..='\u{7e}').contains(&c) {
            break;
        }
    }
}

/// Lo que se guarda de un host en el fichero de salida.
#[derive(Debug, Clone)]
pub struct ParteSalida {
    pub host: String,
    pub estado: String,
    pub codigo: Option<i32>,
    pub duracion_ms: Option<u64>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub truncada: bool,
}

/// Fichero de salida: una cabecera y, por host, su estado y sus dos flujos
/// tal cual llegaron (bytes crudos: es una copia, no una vista).
pub fn fichero_de_salida(cabecera: &str, partes: &[ParteSalida]) -> Vec<u8> {
    let mut fichero = Vec::new();
    fichero.extend_from_slice(cabecera.as_bytes());
    fichero.push(b'\n');
    for parte in partes {
        let mut titulo = format!("\n## {} · {}", parte.host, parte.estado);
        if let Some(codigo) = parte.codigo {
            titulo.push_str(&format!(" · código {codigo}"));
        }
        if let Some(duracion) = parte.duracion_ms {
            titulo.push_str(&format!(" · {}", duracion_legible(duracion)));
        }
        if parte.truncada {
            titulo.push_str(" · salida truncada a 1 MiB por flujo");
        }
        fichero.extend_from_slice(titulo.as_bytes());
        fichero.extend_from_slice(b"\n--- stdout ---\n");
        fichero.extend_from_slice(&parte.stdout);
        if !parte.stdout.is_empty() && !parte.stdout.ends_with(b"\n") {
            fichero.push(b'\n');
        }
        fichero.extend_from_slice(b"--- stderr ---\n");
        fichero.extend_from_slice(&parte.stderr);
        if !parte.stderr.is_empty() && !parte.stderr.ends_with(b"\n") {
            fichero.push(b'\n');
        }
    }
    fichero
}

/// «1,2 s», «850 ms», «2 m 5 s».
pub fn duracion_legible(milisegundos: u64) -> String {
    if milisegundos < 1000 {
        return format!("{milisegundos} ms");
    }
    if milisegundos < 60_000 {
        let decimas = milisegundos / 100;
        return format!("{},{} s", decimas / 10, decimas % 10);
    }
    let segundos = milisegundos / 1000;
    format!("{} m {} s", segundos / 60, segundos % 60)
}

/// Ruta por defecto del fichero de salida en el directorio personal:
/// `magi-salida-<snippet>[-<host>]-AAAAmmdd-HHMMSS.txt`, con nombres saneados.
pub fn ruta_por_defecto(
    hogar: &Path,
    snippet: &str,
    host: Option<&str>,
    fecha: chrono::DateTime<chrono::Local>,
) -> String {
    let mut nombre = format!("magi-salida-{}", para_fichero(snippet));
    if let Some(host) = host {
        nombre.push('-');
        nombre.push_str(&para_fichero(host));
    }
    nombre.push_str(&fecha.format("-%Y%m%d-%H%M%S.txt").to_string());
    hogar.join(nombre).display().to_string()
}

/// Un nombre apto para fichero: letras, dígitos, `.`, `_` y `-`; el resto, `_`.
fn para_fichero(texto: &str) -> String {
    let limpio: String = texto
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if limpio.is_empty() {
        "salida".to_string()
    } else {
        limpio
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn quita_colores_y_movimientos_del_cursor() {
        assert_eq!(texto_limpio(b"\x1b[31mrojo\x1b[0m normal"), "rojo normal");
        assert_eq!(texto_limpio(b"a\x1b[2K\x1b[1Gb"), "ab");
    }

    #[test]
    fn quita_osc_con_bel_y_con_st() {
        assert_eq!(texto_limpio(b"\x1b]0;titulo\x07texto"), "texto");
        assert_eq!(
            texto_limpio(b"\x1b]8;;http://x\x1b\\enlace\x1b]8;;\x1b\\"),
            "enlace"
        );
    }

    #[test]
    fn un_retorno_de_carro_suelto_sobrescribe_la_linea() {
        assert_eq!(texto_limpio(b"10%\r50%\r100%\nfin"), "100%\nfin");
        assert_eq!(texto_limpio(b"a\r\nb\r\n"), "a\nb\n");
    }

    #[test]
    fn tabuladores_y_controles() {
        assert_eq!(texto_limpio(b"a\tb"), "a   b");
        assert_eq!(texto_limpio(b"x\x07\x08y"), "xy");
    }

    #[test]
    fn utf8_invalido_no_rompe_nada() {
        let limpio = texto_limpio(b"ok \xff\xfe fin");
        assert!(limpio.starts_with("ok "));
        assert!(limpio.ends_with(" fin"));
    }

    #[test]
    fn las_lineas_limpias_no_tienen_la_vacia_final() {
        assert_eq!(lineas_limpias(b"a\nb\n"), vec!["a", "b"]);
        assert!(lineas_limpias(b"").is_empty());
    }

    #[test]
    fn sanear_se_queda_con_la_ultima_linea_y_la_recorta() {
        assert_eq!(sanear_linea("uno\n\x1b[31mdos\x1b[0m\n", 20), "dos");
        assert_eq!(sanear_linea("abcdefghij", 5), "abcd…");
    }

    #[test]
    fn el_fichero_lleva_cabecera_y_los_dos_flujos() {
        let fichero = fichero_de_salida(
            "reiniciar nginx · 11:42",
            &[ParteSalida {
                host: "hetzner-02".to_string(),
                estado: "fallo".to_string(),
                codigo: Some(1),
                duracion_ms: Some(800),
                stdout: b"salida".to_vec(),
                stderr: b"error\n".to_vec(),
                truncada: false,
            }],
        );
        let texto = String::from_utf8(fichero).unwrap();
        assert!(texto.starts_with("reiniciar nginx · 11:42\n"));
        assert!(texto.contains("## hetzner-02 · fallo · código 1 · 800 ms"));
        assert!(texto.contains("--- stdout ---\nsalida\n--- stderr ---\nerror\n"));
    }

    #[test]
    fn duraciones() {
        assert_eq!(duracion_legible(850), "850 ms");
        assert_eq!(duracion_legible(1_234), "1,2 s");
        assert_eq!(duracion_legible(125_000), "2 m 5 s");
    }

    #[test]
    fn la_ruta_por_defecto_sanea_los_nombres() {
        use chrono::TimeZone as _;
        let fecha = chrono::Local
            .with_ymd_and_hms(2026, 9, 27, 11, 42, 5)
            .unwrap();
        let ruta = ruta_por_defecto(
            Path::new("/home/h"),
            "reiniciar nginx",
            Some("web/01"),
            fecha,
        );
        assert_eq!(
            ruta,
            "/home/h/magi-salida-reiniciar_nginx-web_01-20260927-114205.txt"
        );
    }
}
