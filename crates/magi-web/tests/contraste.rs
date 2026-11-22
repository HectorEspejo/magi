//! Pruebas de contraste de las dos paletas de `web/estilos/web.css` (informe §7.9)
//! y de la propia fórmula de WCAG.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use magi_web::contraste::{contraste, luminancia, parsear_hex, Rgb};

fn raiz_del_repositorio() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn leer_css() -> String {
    let ruta = raiz_del_repositorio().join("web/estilos/web.css");
    fs::read_to_string(&ruta).unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", ruta.display()))
}

fn quitar_comentarios(css: &str) -> String {
    let mut salida = String::with_capacity(css.len());
    let mut resto = css;
    while let Some(inicio) = resto.find("/*") {
        salida.push_str(&resto[..inicio]);
        match resto[inicio + 2..].find("*/") {
            Some(fin) => resto = &resto[inicio + 2 + fin + 2..],
            None => {
                resto = "";
                break;
            }
        }
    }
    salida.push_str(resto);
    salida
}

/// Contenido entre las llaves emparejadas de la primera `{` que sigue a `desde`.
fn bloque_desde(css: &str, desde: usize) -> &str {
    let inicio = css[desde..].find('{').expect("falta «{»") + desde + 1;
    let mut profundidad = 1;
    for (i, c) in css[inicio..].char_indices() {
        match c {
            '{' => profundidad += 1,
            '}' => {
                profundidad -= 1;
                if profundidad == 0 {
                    return &css[inicio..inicio + i];
                }
            }
            _ => {}
        }
    }
    panic!("llaves sin cerrar");
}

fn tokens(bloque: &str) -> BTreeMap<String, String> {
    let mut mapa = BTreeMap::new();
    for declaracion in bloque.split(';') {
        let declaracion = declaracion.trim();
        if let Some(resto) = declaracion.strip_prefix("--") {
            if let Some((nombre, valor)) = resto.split_once(':') {
                mapa.insert(nombre.trim().to_string(), valor.trim().to_string());
            }
        }
    }
    mapa
}

struct Paletas {
    clara: BTreeMap<String, String>,
    oscura: BTreeMap<String, String>,
}

fn paletas() -> Paletas {
    let css = quitar_comentarios(&leer_css());
    let primera_raiz = css.find(":root").expect("falta :root");
    let media = css
        .find("@media (prefers-color-scheme: dark)")
        .expect("falta @media (prefers-color-scheme: dark)");
    assert!(
        primera_raiz < media,
        "la paleta clara (:root) tiene que ir antes que la oscura"
    );
    let clara = tokens(bloque_desde(&css, primera_raiz));

    let bloque_media = bloque_desde(&css, media);
    let raiz_oscura = bloque_media
        .find(":root")
        .expect("falta :root dentro de @media");
    let oscura = tokens(bloque_desde(bloque_media, raiz_oscura));
    Paletas { clara, oscura }
}

fn color(paleta: &BTreeMap<String, String>, nombre: &str) -> Rgb {
    let valor = paleta
        .get(nombre)
        .unwrap_or_else(|| panic!("falta el token --{nombre}"));
    parsear_hex(valor).unwrap_or_else(|| panic!("--{nombre}: «{valor}» no es un color #RRGGBB"))
}

const TOKENS: [&str; 10] = [
    "fondo",
    "superficie",
    "texto",
    "secundario",
    "acento",
    "correcto",
    "critico",
    "foco",
    "borde-control",
    "borde-decorativo",
];

const TOKENS_DE_TEXTO: [&str; 5] = ["texto", "secundario", "acento", "correcto", "critico"];
const TOKENS_DE_BORDE: [&str; 2] = ["borde-control", "foco"];
const FONDOS: [&str; 2] = ["fondo", "superficie"];

fn aproximado(a: f64, b: f64, margen: f64) -> bool {
    (a - b).abs() <= margen
}

#[test]
fn el_contraste_de_wcag_sale_con_los_valores_conocidos() {
    let negro = parsear_hex("#000000").expect("negro");
    let blanco = parsear_hex("#FFF").expect("blanco");
    assert!(aproximado(contraste(negro, blanco), 21.0, 1e-9));
    assert!(aproximado(contraste(blanco, negro), 21.0, 1e-9));
    assert!(aproximado(contraste(blanco, blanco), 1.0, 1e-12));
    assert!(aproximado(luminancia(blanco), 1.0, 1e-9));
    assert!(aproximado(luminancia(negro), 0.0, 1e-12));

    // El acento de la TUI sobre su fondo: 9,69:1 (informe §7.9).
    let acento = parsear_hex("#FFA000").expect("acento");
    let fondo = parsear_hex("#0A0A0A").expect("fondo");
    let c = contraste(acento, fondo);
    assert!(aproximado(c, 9.69, 0.005), "acento sobre fondo: {c}");

    // Otras filas de la tabla del informe, con su redondeo a dos decimales.
    let tabla = [
        ("#EDE8DC", "#0A0A0A", 16.19),
        ("#00FF6A", "#0A0A0A", 14.62),
        ("#FF2A1A", "#0A0A0A", 5.28),
        ("#9A968C", "#0A0A0A", 6.71),
        ("#5A5A5A", "#0A0A0A", 2.87),
        ("#0A0A0A", "#F4F0E6", 17.40),
        ("#8A5000", "#F4F0E6", 5.72),
        ("#006B2E", "#F4F0E6", 5.87),
        ("#B3140A", "#F4F0E6", 6.11),
        ("#5C5850", "#F4F0E6", 6.22),
        ("#8C887E", "#F4F0E6", 3.11),
    ];
    for (a, b, esperado) in tabla {
        let (a_rgb, b_rgb) = (parsear_hex(a).expect(a), parsear_hex(b).expect(b));
        let c = contraste(a_rgb, b_rgb);
        assert!(
            aproximado((c * 100.0).round() / 100.0, esperado, 0.0051),
            "{a} sobre {b}: {c:.3} y el informe dice {esperado}"
        );
    }
}

#[test]
fn la_paleta_oscura_coincide_con_tema_rs() {
    let tema = fs::read_to_string(raiz_del_repositorio().join("src/tema.rs"))
        .expect("no se pudo leer src/tema.rs");
    let oscura = paletas().oscura;

    // Constante de tema.rs → token de la web. El inactivo solo sirve de borde decorativo.
    let correspondencias = [
        ("FONDO", "fondo"),
        ("TEXTO", "texto"),
        ("ACENTO", "acento"),
        ("CORRECTO", "correcto"),
        ("CRITICO", "critico"),
        ("INACTIVO", "borde-decorativo"),
    ];
    for (constante, token) in correspondencias {
        let esperado = rgb_de_constante(&tema, constante);
        assert_eq!(
            color(&oscura, token),
            esperado,
            "--{token} de la paleta oscura no es {constante} de src/tema.rs"
        );
    }
    // El gris secundario es el único color nuevo de la oscura (informe §7.9).
    assert_eq!(
        color(&oscura, "secundario"),
        parsear_hex("#9A968C").expect("gris")
    );
}

/// `pub const NOMBRE: Color = Color::Rgb(0x0A, 0x0A, 0x0A);` → `Rgb`.
fn rgb_de_constante(fuente: &str, nombre: &str) -> Rgb {
    let patron = format!("pub const {nombre}: Color = Color::Rgb(");
    let linea = fuente
        .lines()
        .find(|l| l.trim_start().starts_with(&patron))
        .unwrap_or_else(|| panic!("no hay «{patron}…» en src/tema.rs"));
    let dentro =
        &linea[linea.find('(').expect("paréntesis") + 1..linea.rfind(')').expect("paréntesis")];
    let numeros: Vec<u8> = dentro
        .split(',')
        .map(|n| {
            let n = n.trim();
            u8::from_str_radix(n.trim_start_matches("0x"), 16)
                .unwrap_or_else(|_| panic!("«{n}» no es un byte hexadecimal en {linea}"))
        })
        .collect();
    assert_eq!(numeros.len(), 3, "{linea}");
    Rgb {
        r: numeros[0],
        g: numeros[1],
        b: numeros[2],
    }
}

#[test]
fn la_paleta_clara_es_la_del_informe() {
    let clara = paletas().clara;
    let del_informe = [
        ("fondo", "#F4F0E6"),
        ("texto", "#0A0A0A"),
        ("acento", "#8A5000"),
        ("correcto", "#006B2E"),
        ("critico", "#B3140A"),
        ("secundario", "#5C5850"),
        ("borde-decorativo", "#8C887E"),
    ];
    for (token, valor) in del_informe {
        assert_eq!(
            color(&clara, token),
            parsear_hex(valor).expect(valor),
            "--{token} de la paleta clara no es {valor} (informe §7.9)"
        );
    }
}

#[test]
fn las_dos_paletas_definen_los_mismos_tokens() {
    let Paletas { clara, oscura } = paletas();
    for token in TOKENS {
        assert!(
            clara.contains_key(token),
            "la paleta clara no define --{token}"
        );
        assert!(
            oscura.contains_key(token),
            "la paleta oscura no define --{token}"
        );
    }
}

/// Contrastes de las dos paletas: la tabla para el mensaje y los que fallan.
fn evaluar_contraste(paletas: &Paletas) -> (String, Vec<String>) {
    let Paletas { clara, oscura } = paletas;
    let mut tabla = String::new();
    let mut fallos = Vec::new();

    for (nombre, paleta) in [("clara", clara), ("oscura", oscura)] {
        tabla.push_str(&format!("\npaleta {nombre}\n"));
        for fondo in FONDOS {
            let color_fondo = color(paleta, fondo);
            for token in TOKENS_DE_TEXTO {
                let c = contraste(color(paleta, token), color_fondo);
                let bien = c >= 4.5;
                tabla.push_str(&format!(
                    "  {token:<16} sobre {fondo:<10} {c:>6.2}:1  (mínimo 4,5)  {}\n",
                    if bien { "ok" } else { "FALLA" }
                ));
                if !bien {
                    fallos.push(format!(
                        "{nombre}: --{token} sobre --{fondo} = {c:.2}:1 (< 4,5)"
                    ));
                }
            }
            for token in TOKENS_DE_BORDE {
                let c = contraste(color(paleta, token), color_fondo);
                let bien = c >= 3.0;
                tabla.push_str(&format!(
                    "  {token:<16} sobre {fondo:<10} {c:>6.2}:1  (mínimo 3,0)  {}\n",
                    if bien { "ok" } else { "FALLA" }
                ));
                if !bien {
                    fallos.push(format!(
                        "{nombre}: --{token} sobre --{fondo} = {c:.2}:1 (< 3)"
                    ));
                }
            }
        }
        // El borde decorativo no transmite nada: está exento, pero se enseña.
        let c = contraste(color(paleta, "borde-decorativo"), color(paleta, "fondo"));
        tabla.push_str(&format!(
            "  {:<16} sobre {:<10} {c:>6.2}:1  (exento: solo bordes decorativos)\n",
            "borde-decorativo", "fondo"
        ));
    }
    (tabla, fallos)
}

#[test]
fn las_dos_paletas_cumplen_el_contraste() {
    let (tabla, fallos) = evaluar_contraste(&paletas());
    assert!(
        fallos.is_empty(),
        "contraste insuficiente:\n  {}\n{tabla}",
        fallos.join("\n  ")
    );
}

#[test]
fn el_borde_decorativo_solo_se_usa_en_bordes() {
    let css = quitar_comentarios(&leer_css());
    let mut usos = 0;
    for declaracion in css.split([';', '{', '}']) {
        let declaracion = declaracion.trim();
        if !declaracion.contains("var(--borde-decorativo)") {
            continue;
        }
        usos += 1;
        let propiedad = declaracion.split(':').next().unwrap_or("").trim();
        assert!(
            propiedad.starts_with("border") && !propiedad.starts_with("border-radius"),
            "--borde-decorativo (exento del contraste) solo vale para bordes decorativos y se usa en «{declaracion}»"
        );
    }
    assert!(usos > 0, "no se usa --borde-decorativo: ¿se ha renombrado?");
}

#[test]
fn toda_variable_que_se_usa_esta_definida_en_las_dos_paletas() {
    let Paletas { clara, oscura } = paletas();
    let css = quitar_comentarios(&leer_css());
    let mut resto = css.as_str();
    let mut usadas = std::collections::BTreeSet::new();
    while let Some(i) = resto.find("var(--") {
        let tras = &resto[i + "var(--".len()..];
        let fin = tras.find(')').expect("var( sin cerrar");
        usadas.insert(
            tras[..fin]
                .split(',')
                .next()
                .unwrap_or("")
                .trim()
                .to_string(),
        );
        resto = &tras[fin..];
    }
    assert!(!usadas.is_empty());
    for nombre in usadas {
        assert!(
            clara.contains_key(&nombre),
            "var(--{nombre}) no está en la paleta clara"
        );
        assert!(
            oscura.contains_key(&nombre),
            "var(--{nombre}) no está en la paleta oscura"
        );
    }
}

#[test]
fn el_foco_es_visible_y_usa_su_token() {
    let css = quitar_comentarios(&leer_css());
    let inicio = css.find(":focus-visible").expect("falta :focus-visible");
    let bloque = bloque_desde(&css, inicio);
    assert!(
        bloque.contains("outline: 3px solid var(--foco)"),
        "{bloque}"
    );
    assert!(css.contains("prefers-reduced-motion: reduce"));
}

#[test]
fn el_css_no_carga_nada_de_fuera_ni_usa_style() {
    let css = quitar_comentarios(&leer_css());
    assert!(!css.contains("@import"), "@import carga otra hoja");
    assert!(!css.to_ascii_lowercase().contains("expression("));
    assert!(
        !css.contains("http://") && !css.contains("https://"),
        "URL externa en el CSS"
    );
    assert!(!css.contains("//"), "URL con protocolo implícito en el CSS");
    // Las únicas `url(...)` son las fuentes de la propia web.
    let mut resto = css.as_str();
    let mut urls = 0;
    while let Some(i) = resto.find("url(") {
        let tras = &resto[i + 4..];
        let fin = tras.find(')').expect("url( sin cerrar");
        let destino = tras[..fin].trim().trim_matches('"').trim_matches('\'');
        assert!(
            destino.starts_with("../fuentes/") && destino.ends_with(".woff2"),
            "url({destino}) no es una fuente de la web"
        );
        urls += 1;
        resto = &tras[fin..];
    }
    assert_eq!(urls, 3, "se esperaban las tres fuentes");
}

#[test]
fn las_fuentes_se_declaran_como_dice_el_informe() {
    let css = quitar_comentarios(&leer_css());
    assert_eq!(css.matches("@font-face").count(), 3);
    assert_eq!(css.matches("font-display: swap").count(), 3);
    for fichero in [
        "../fuentes/JetBrainsMono-Regular.woff2",
        "../fuentes/JetBrainsMono-Bold.woff2",
        "../fuentes/magi-simbolos.woff2",
    ] {
        assert!(css.contains(fichero), "falta {fichero}");
    }
    assert!(css.contains(
        "unicode-range: U+21B5, U+21BB, U+21C4, U+21C5, U+25A3, U+25D0, U+25D1, U+2605, U+2691"
    ));
    assert!(
        css.contains("font-family: \"JetBrains Mono\", \"MAGI Simbolos\", ui-monospace, monospace")
    );
}

// ---------------------------------------------------------------------------
// La paleta del tema de mdBook (`web/manual/theme/css/variables.css`)
// ---------------------------------------------------------------------------

/// Las dos paletas del manual: `light`, `rust` y la ausencia de JavaScript, la clara;
/// `coal`, `navy` y `ayu`, la oscura. Son los mismos diez tokens que los de la web.
fn paletas_del_manual() -> Paletas {
    let ruta = raiz_del_repositorio().join("web/manual/theme/css/variables.css");
    let css = quitar_comentarios(
        &fs::read_to_string(&ruta)
            .unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", ruta.display())),
    );
    let clara = css.find(".light,").expect("falta la regla de .light");
    let oscura = css.find(".coal,").expect("falta la regla de .coal");
    Paletas {
        clara: tokens(bloque_desde(&css, clara)),
        oscura: tokens(bloque_desde(&css, oscura)),
    }
}

#[test]
fn las_dos_paletas_del_manual_definen_los_mismos_tokens() {
    let Paletas { clara, oscura } = paletas_del_manual();
    for token in TOKENS {
        assert!(
            clara.contains_key(token),
            "la paleta clara del manual no define --{token}"
        );
        assert!(
            oscura.contains_key(token),
            "la paleta oscura del manual no define --{token}"
        );
    }
}

#[test]
fn la_paleta_del_manual_cumple_el_contraste() {
    let (tabla, fallos) = evaluar_contraste(&paletas_del_manual());
    assert!(
        fallos.is_empty(),
        "contraste insuficiente en el manual:\n  {}\n{tabla}",
        fallos.join("\n  ")
    );
}

#[test]
fn la_paleta_oscura_del_manual_coincide_con_tema_rs() {
    let tema = fs::read_to_string(raiz_del_repositorio().join("src/tema.rs"))
        .expect("no se pudo leer src/tema.rs");
    let oscura = paletas_del_manual().oscura;
    for (constante, token) in [
        ("FONDO", "fondo"),
        ("TEXTO", "texto"),
        ("ACENTO", "acento"),
        ("CORRECTO", "correcto"),
        ("CRITICO", "critico"),
        ("INACTIVO", "borde-decorativo"),
    ] {
        assert_eq!(
            color(&oscura, token),
            rgb_de_constante(&tema, constante),
            "--{token} de la paleta oscura del manual no es {constante} de src/tema.rs"
        );
    }
    assert_eq!(
        color(&oscura, "secundario"),
        parsear_hex("#9A968C").expect("gris")
    );
}

#[test]
fn la_paleta_del_manual_es_la_misma_que_la_de_la_web() {
    let del_manual = paletas_del_manual();
    let de_la_web = paletas();
    for token in TOKENS {
        assert_eq!(
            color(&del_manual.clara, token),
            color(&de_la_web.clara, token),
            "--{token} de la paleta clara no es el de la web"
        );
        assert_eq!(
            color(&del_manual.oscura, token),
            color(&de_la_web.oscura, token),
            "--{token} de la paleta oscura no es el de la web"
        );
    }
}
