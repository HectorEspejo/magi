//! Pruebas del exportador de capturas (`magi_web::capturas`) contra su API pública.
//!
//! Lo que se exporta se inserta en las páginas con `|safe`, así que estas pruebas
//! cuidan sobre todo la barrera (`validar_fragmento` y `validar_css`) con casos
//! adversariales, y que nada entre en pánico con basura.

use std::collections::BTreeSet;

use magi_web::capturas::{
    exportar, validar_css, validar_fragmento, CapturaCeldas, CeldaCaptura, Cobertura, ColorCaptura,
    ConjuntoExportado, COLUMNAS, FILAS, FONDO, IDS, TEXTO,
};

// ---------------------------------------------------------------------------
// Ayudas
// ---------------------------------------------------------------------------

/// Generador congruencial lineal propio (sin dependencias): mismas cadenas en
/// cada ejecución.
struct Lcg(u64);

impl Lcg {
    fn siguiente(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn hasta(&mut self, n: usize) -> usize {
        (self.siguiente() % n as u64) as usize
    }

    fn elegir<'a, T>(&mut self, lista: &'a [T]) -> &'a T {
        &lista[self.hasta(lista.len())]
    }
}

const ROJO: ColorCaptura = ColorCaptura::Rgb(0xff, 0x00, 0x00);
const VERDE: ColorCaptura = ColorCaptura::Rgb(0x00, 0xff, 0x6a);
const AMBAR: ColorCaptura = ColorCaptura::Rgb(0xff, 0xa0, 0x00);

/// Una celda con el estilo por defecto.
fn celda(simbolo: &str) -> CeldaCaptura {
    CeldaCaptura {
        simbolo: simbolo.to_string(),
        fg: ColorCaptura::Reset,
        bg: ColorCaptura::Reset,
        negrita: false,
        tenue: false,
        cursiva: false,
        subrayado: false,
        invertido: false,
        oculta: false,
    }
}

fn con_fg(simbolo: &str, fg: ColorCaptura) -> CeldaCaptura {
    CeldaCaptura {
        fg,
        ..celda(simbolo)
    }
}

fn con_bg(simbolo: &str, bg: ColorCaptura) -> CeldaCaptura {
    CeldaCaptura {
        bg,
        ..celda(simbolo)
    }
}

/// Una fila con una celda por carácter y el estilo por defecto.
fn fila_de(texto: &str) -> Vec<CeldaCaptura> {
    texto.chars().map(|c| celda(&c.to_string())).collect()
}

fn captura(id: &str, filas: &[&str]) -> CapturaCeldas {
    CapturaCeldas {
        id: id.to_string(),
        filas: filas.iter().map(|f| fila_de(f)).collect(),
    }
}

fn captura_de_filas(id: &str, filas: Vec<Vec<CeldaCaptura>>) -> CapturaCeldas {
    CapturaCeldas {
        id: id.to_string(),
        filas,
    }
}

/// ASCII imprimible y algunos caracteres de caja: lo que cubren las fuentes en
/// estas pruebas. `↵`, `↻` y `⤴` no están.
fn cobertura() -> Cobertura {
    let mut codigos: BTreeSet<u32> = (0x21..=0x7e).collect();
    codigos.extend("─│┌┐└┘├┤┬┴┼█▀▄".chars().map(u32::from));
    Cobertura::desde_codigos(codigos)
}

/// El degradador de las pruebas: `⤴` → `>`; el resto, algo que no vale.
fn degradar(simbolo: &str) -> String {
    match simbolo {
        "⤴" => ">".to_string(),
        "↵" => "enter".to_string(),
        _ => "?".to_string(),
    }
}

fn exportar_bien(capturas: &[CapturaCeldas]) -> ConjuntoExportado {
    exportar(capturas, &cobertura(), &degradar).expect("la exportación debe salir bien")
}

fn error_de(capturas: &[CapturaCeldas]) -> String {
    exportar(capturas, &cobertura(), &degradar)
        .expect_err("la exportación debe fallar")
        .to_string()
}

const PRE: &str = r#"<pre class="captura" aria-hidden="true">"#;

/// El contenido de las reglas (`selector`, cuerpo) de un CSS.
fn reglas(css: &str) -> Vec<(String, String)> {
    css.lines()
        .map(|linea| {
            let (selector, resto) = linea.split_once('{').expect("regla con «{»");
            let cuerpo = resto.strip_suffix('}').expect("regla con «}»");
            (selector.to_string(), cuerpo.to_string())
        })
        .collect()
}

/// El texto que se ve de un fragmento: sin etiquetas y con las entidades
/// descodificadas.
fn texto_visible(fragmento: &str) -> String {
    let mut sin_etiquetas = String::new();
    let mut en_etiqueta = false;
    for c in fragmento.chars() {
        match c {
            '<' => en_etiqueta = true,
            '>' if en_etiqueta => en_etiqueta = false,
            _ if en_etiqueta => {}
            _ => sin_etiquetas.push(c),
        }
    }
    sin_etiquetas
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// Las líneas visibles de un fragmento sin espacios finales ni líneas vacías al
/// final: lo que debe coincidir con `CapturaCeldas::texto`.
fn lineas_visibles(fragmento: &str) -> Vec<String> {
    let mut lineas: Vec<String> = texto_visible(fragmento)
        .split('\n')
        .map(|l| l.trim_end_matches(' ').to_string())
        .collect();
    while lineas.last().is_some_and(String::is_empty) {
        lineas.pop();
    }
    lineas
}

// ---------------------------------------------------------------------------
// Estilos y clases
// ---------------------------------------------------------------------------

#[test]
fn el_estilo_por_defecto_no_genera_span() {
    let c = captura("flota", &["hola mundo", "otra fila"]);
    let r = exportar_bien(&[c]);
    assert_eq!(
        r.fragmentos["flota"],
        format!("{PRE}hola mundo\notra fila</pre>")
    );
    assert!(!r.fragmentos["flota"].contains("<span"));
    // Sin clases: solo la regla de `.captura`.
    assert_eq!(r.css.lines().count(), 1, "{}", r.css);
    assert!(r
        .css
        .starts_with(".captura{background:#0a0a0a;color:#ede8dc;"));

    // Los colores que valen lo mismo que los de por defecto tampoco generan span:
    // `Reset`, `Rgb(TEXTO)` y `Rgb(FONDO)` son el mismo estilo.
    let explicito = captura_de_filas(
        "flota",
        vec![vec![
            con_fg("a", ColorCaptura::Rgb(TEXTO.0, TEXTO.1, TEXTO.2)),
            con_bg("b", ColorCaptura::Rgb(FONDO.0, FONDO.1, FONDO.2)),
            CeldaCaptura {
                fg: ColorCaptura::Rgb(TEXTO.0, TEXTO.1, TEXTO.2),
                bg: ColorCaptura::Rgb(FONDO.0, FONDO.1, FONDO.2),
                ..celda("c")
            },
            // Invertida sobre los colores de por defecto con los dos intercambiados:
            // no, esa sí cambia (ver `reset_se_resuelve_antes_de_invertir`).
        ]],
    );
    let r = exportar_bien(&[explicito]);
    assert_eq!(r.fragmentos["flota"], format!("{PRE}abc</pre>"));
}

#[test]
fn un_estilo_distinto_genera_una_clase_numerada_por_orden_de_aparicion() {
    let fila = vec![
        con_fg("a", ROJO),
        celda("b"),
        con_fg("c", VERDE),
        con_fg("d", ROJO),
        con_fg("e", ROJO),
    ];
    let r = exportar_bien(&[captura_de_filas("flota", vec![fila])]);
    assert_eq!(
        r.fragmentos["flota"],
        format!(
            "{PRE}<span class=\"c0\">a</span>b<span class=\"c1\">c</span>\
             <span class=\"c0\">de</span></pre>"
        )
    );
    let lineas: Vec<&str> = r.css.lines().collect();
    assert_eq!(lineas.len(), 3, "{}", r.css);
    assert!(lineas[0].starts_with(".captura{"));
    assert_eq!(lineas[1], ".c0{color:#ff0000}");
    assert_eq!(lineas[2], ".c1{color:#00ff6a}");

    // Cada modificador, solo con lo que hace falta, en un orden fijo.
    let todo = CeldaCaptura {
        fg: ROJO,
        bg: VERDE,
        negrita: true,
        cursiva: true,
        subrayado: true,
        ..celda("x")
    };
    let r = exportar_bien(&[captura_de_filas(
        "flota",
        vec![vec![
            CeldaCaptura {
                negrita: true,
                ..celda("a")
            },
            CeldaCaptura {
                cursiva: true,
                ..celda("b")
            },
            CeldaCaptura {
                subrayado: true,
                ..celda("c")
            },
            con_bg("d", AMBAR),
            todo,
        ]],
    )]);
    let reglas = reglas(&r.css);
    assert_eq!(
        reglas[1],
        (".c0".to_string(), "font-weight:700".to_string())
    );
    assert_eq!(
        reglas[2],
        (".c1".to_string(), "font-style:italic".to_string())
    );
    assert_eq!(
        reglas[3],
        (".c2".to_string(), "text-decoration:underline".to_string())
    );
    assert_eq!(
        reglas[4],
        (".c3".to_string(), "background:#ffa000".to_string())
    );
    assert_eq!(
        reglas[5],
        (
            ".c4".to_string(),
            "color:#ff0000;background:#00ff6a;font-weight:700;font-style:italic;\
             text-decoration:underline"
                .to_string()
        )
    );
}

#[test]
fn la_numeracion_de_clases_es_comun_a_todas_las_capturas() {
    let uno = captura_de_filas("flota", vec![vec![con_fg("x", ROJO)]]);
    let dos = captura_de_filas(
        "archivos",
        vec![vec![
            con_fg("y", VERDE),
            con_fg("x", ROJO),
            con_fg("z", AMBAR),
        ]],
    );
    let r = exportar_bien(&[uno.clone(), dos.clone()]);
    assert_eq!(
        r.fragmentos["flota"],
        format!("{PRE}<span class=\"c0\">x</span></pre>")
    );
    assert_eq!(
        r.fragmentos["archivos"],
        format!(
            "{PRE}<span class=\"c1\">y</span><span class=\"c0\">x</span>\
             <span class=\"c2\">z</span></pre>"
        )
    );
    // Un solo CSS para todas.
    assert_eq!(r.css.lines().count(), 4, "{}", r.css);

    // El orden de la lista manda: al invertirla, cambia la numeración.
    let r = exportar_bien(&[dos, uno]);
    assert_eq!(
        r.fragmentos["archivos"],
        format!(
            "{PRE}<span class=\"c0\">y</span><span class=\"c1\">x</span>\
             <span class=\"c2\">z</span></pre>"
        )
    );
    assert_eq!(
        r.fragmentos["flota"],
        format!("{PRE}<span class=\"c1\">x</span></pre>")
    );
}

#[test]
fn la_primera_aparicion_se_cuenta_por_filas_y_de_izquierda_a_derecha() {
    let c = captura_de_filas(
        "flota",
        vec![
            vec![celda("a"), celda("b"), con_fg("c", VERDE)],
            vec![con_fg("d", ROJO), con_fg("e", VERDE)],
        ],
    );
    let r = exportar_bien(&[c]);
    // `VERDE` aparece antes (fila 1) que `ROJO` (fila 2), aunque este vaya a la
    // izquierda.
    assert_eq!(
        r.fragmentos["flota"],
        format!("{PRE}ab<span class=\"c0\">c</span>\n<span class=\"c1\">d</span><span class=\"c0\">e</span></pre>")
    );
}

#[test]
fn reset_se_resuelve_antes_de_invertir() {
    let invertida = CeldaCaptura {
        invertido: true,
        ..celda("x")
    };
    let r = exportar_bien(&[captura_de_filas("flota", vec![vec![invertida]])]);
    // Fondo claro y texto oscuro: fg = FONDO y bg = TEXTO.
    assert_eq!(
        reglas(&r.css)[1],
        (
            ".c0".to_string(),
            "color:#0a0a0a;background:#ede8dc".to_string()
        )
    );
    assert_eq!(
        r.fragmentos["flota"],
        format!("{PRE}<span class=\"c0\">x</span></pre>")
    );

    // Con un color explícito, solo el otro lado se resuelve desde `Reset`.
    let c = CeldaCaptura {
        fg: ROJO,
        invertido: true,
        ..celda("x")
    };
    let r = exportar_bien(&[captura_de_filas("flota", vec![vec![c]])]);
    assert_eq!(
        reglas(&r.css)[1].1,
        "color:#0a0a0a;background:#ff0000".to_string()
    );
    let c = CeldaCaptura {
        bg: ROJO,
        invertido: true,
        ..celda("x")
    };
    let r = exportar_bien(&[captura_de_filas("flota", vec![vec![c]])]);
    assert_eq!(
        reglas(&r.css)[1].1,
        "color:#ff0000;background:#ede8dc".to_string()
    );

    // Dos veces invertida (a mano, con colores intercambiados) es lo mismo que sin
    // invertir: los dos estilos comparten clase.
    let a = CeldaCaptura {
        fg: ROJO,
        bg: VERDE,
        ..celda("a")
    };
    let b = CeldaCaptura {
        fg: VERDE,
        bg: ROJO,
        invertido: true,
        ..celda("b")
    };
    let r = exportar_bien(&[captura_de_filas("flota", vec![vec![a, b]])]);
    assert_eq!(
        r.fragmentos["flota"],
        format!("{PRE}<span class=\"c0\">ab</span></pre>")
    );
}

#[test]
fn tenue_mezcla_con_el_fondo_y_oculta_iguala_el_texto_al_fondo() {
    let tenue = CeldaCaptura {
        fg: ColorCaptura::Rgb(200, 100, 50),
        bg: ColorCaptura::Rgb(100, 0, 10),
        tenue: true,
        ..celda("t")
    };
    let oculta = CeldaCaptura {
        fg: ROJO,
        bg: VERDE,
        oculta: true,
        ..celda("o")
    };
    // La mezcla va después de invertir: se mezcla el color que acaba de texto.
    let tenue_invertida = CeldaCaptura {
        fg: ColorCaptura::Rgb(200, 100, 50),
        bg: ColorCaptura::Rgb(100, 0, 10),
        tenue: true,
        invertido: true,
        ..celda("i")
    };
    let r = exportar_bien(&[captura_de_filas(
        "flota",
        vec![vec![tenue, oculta, tenue_invertida]],
    )]);
    let reglas = reglas(&r.css);
    // Tenue: ((200+100)/2, (100+0)/2, (50+10)/2) = (150, 50, 30).
    assert_eq!(reglas[1].1, "color:#96321e;background:#64000a");
    // Oculta: el texto toma el color del fondo.
    assert_eq!(reglas[2].1, "color:#00ff6a;background:#00ff6a");
    // Invertida y tenue: el texto que queda (100, 0, 10) se mezcla con el fondo
    // que queda (200, 100, 50).
    assert_eq!(reglas[3].1, "color:#96321e;background:#c86432");
}

// ---------------------------------------------------------------------------
// Paleta xterm
// ---------------------------------------------------------------------------

#[test]
fn indexado_usa_la_paleta_xterm() {
    let casos: [(u8, &str); 14] = [
        (0, "000000"),
        (1, "cd0000"),
        (7, "e5e5e5"),
        (8, "7f7f7f"),
        (12, "5c5cff"),
        (15, "ffffff"),
        (16, "000000"),
        (21, "0000ff"),
        (59, "5f5f5f"),
        (196, "ff0000"),
        (226, "ffff00"),
        (231, "ffffff"),
        (232, "080808"),
        (255, "eeeeee"),
    ];
    for (n, esperado) in casos {
        // Como texto.
        let r = exportar_bien(&[captura_de_filas(
            "flota",
            vec![vec![con_fg("x", ColorCaptura::Indexado(n))]],
        )]);
        assert_eq!(
            reglas(&r.css)[1].1,
            format!("color:#{esperado}"),
            "índice {n} como texto"
        );
        // Como fondo.
        let r = exportar_bien(&[captura_de_filas(
            "flota",
            vec![vec![con_bg("x", ColorCaptura::Indexado(n))]],
        )]);
        assert_eq!(
            reglas(&r.css)[1].1,
            format!("background:#{esperado}"),
            "índice {n} como fondo"
        );
    }

    // Los 256 índices dan un CSS válido y 256 colores bien formados.
    let celdas: Vec<CeldaCaptura> = (0..=255u8)
        .map(|n| con_fg("x", ColorCaptura::Indexado(n)))
        .collect();
    let r = exportar_bien(&[captura_de_filas("flota", vec![celdas])]);
    validar_css(&r.css).unwrap();
    for (selector, cuerpo) in reglas(&r.css).iter().skip(1) {
        let color = cuerpo.strip_prefix("color:#").expect("solo color");
        assert!(selector.starts_with(".c"));
        assert_eq!(color.len(), 6);
        assert!(color.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(color, color.to_ascii_lowercase());
    }
    // Hay colores repetidos en la paleta de 256 (el negro del 0 y el del cubo): un
    // mismo color es una misma clase.
    assert!(r.css.lines().count() < 256 + 1);
}

// ---------------------------------------------------------------------------
// HTML, filas y espacios
// ---------------------------------------------------------------------------

#[test]
fn el_html_sale_escapado() {
    let r = exportar_bien(&[captura_de_filas(
        "flota",
        vec![
            fila_de("<&\"'>"),
            vec![con_fg("<", ROJO), con_fg("&", ROJO), con_fg("\"", ROJO)],
            fila_de("a&amp;b <script>"),
        ],
    )]);
    let f = &r.fragmentos["flota"];
    assert_eq!(
        f,
        &format!(
            "{PRE}&lt;&amp;&quot;&#39;&gt;\n<span class=\"c0\">&lt;&amp;&quot;</span>\n\
             a&amp;amp;b &lt;script&gt;</pre>"
        )
    );
    assert!(!f.contains("<script"));
    validar_fragmento(f).unwrap();
    // Y lo que se ve es lo que se pintó.
    assert_eq!(texto_visible(f), "<&\"'>\n<&\"\na&amp;b <script>");
}

#[test]
fn la_salida_es_determinista() {
    let hacer = || {
        captura_de_filas(
            "flota",
            vec![
                vec![
                    con_fg("a", ROJO),
                    con_bg("b", VERDE),
                    celda("c"),
                    con_fg("d", AMBAR),
                ],
                vec![
                    CeldaCaptura {
                        negrita: true,
                        ..con_fg("e", VERDE)
                    },
                    con_fg("f", ROJO),
                ],
            ],
        )
    };
    let otra = captura("tuneles", &["una", "dos"]);
    let primera = exportar_bien(&[hacer(), otra.clone()]);
    for _ in 0..5 {
        assert_eq!(exportar_bien(&[hacer(), otra.clone()]), primera);
    }
    // Y con la entrada construida aparte, byte a byte.
    let copia = exportar(&[hacer(), otra], &cobertura(), &degradar).unwrap();
    assert_eq!(copia.css.as_bytes(), primera.css.as_bytes());
    for (id, fragmento) in &primera.fragmentos {
        assert_eq!(copia.fragmentos[id].as_bytes(), fragmento.as_bytes());
    }
    // Los fragmentos salen en el orden de sus ids.
    let ids: Vec<&String> = primera.fragmentos.keys().collect();
    assert_eq!(ids, vec!["flota", "tuneles"]);
}

#[test]
fn el_texto_de_una_captura_no_lleva_espacios_finales() {
    let c = captura("flota", &["  hola  ", "", "x   ", "   ", "  "]);
    assert_eq!(c.texto(), "  hola\n\nx");
    for linea in c.texto().lines() {
        assert_eq!(linea, linea.trim_end_matches(' '));
    }
    assert!(!c.texto().ends_with('\n'));

    // Una captura en blanco no tiene texto.
    assert_eq!(captura("flota", &["   ", ""]).texto(), "");
    assert_eq!(captura("flota", &[]).texto(), "");

    // Símbolos vacíos, de solo espacios y de controles son un espacio.
    let rara = captura_de_filas(
        "flota",
        vec![vec![
            celda("a"),
            celda(""),
            celda("  "),
            celda("\t"),
            celda("b"),
            celda("\u{7}"),
        ]],
    );
    assert_eq!(rara.texto(), "a   b");

    // El texto es el que se ve en el HTML, línea a línea.
    let c = captura_de_filas(
        "flota",
        vec![
            fila_de("┌──┐   "),
            vec![
                celda("│"),
                con_fg("a", ROJO),
                con_bg(" ", VERDE),
                con_bg(" ", VERDE),
                celda("│"),
                celda(" "),
            ],
            fila_de("└──┘"),
            fila_de(""),
            fila_de("   "),
        ],
    );
    let r = exportar_bien(std::slice::from_ref(&c));
    assert_eq!(
        lineas_visibles(&r.fragmentos["flota"]),
        c.texto().split('\n').collect::<Vec<_>>()
    );
    assert_eq!(c.texto(), "┌──┐\n│a  │\n└──┘");

    // En el HTML se pierden los espacios finales del estilo por defecto, pero no los
    // de un tramo con fondo (se ven).
    let f = &r.fragmentos["flota"];
    assert!(f.contains("┌──┐\n"), "{f}");
    assert!(f.contains("<span class=\"c1\">  </span>│\n"), "{f}");
    let h = captura("flota", &["ab   "]);
    assert_eq!(
        exportar_bien(&[h]).fragmentos["flota"],
        format!("{PRE}ab</pre>")
    );
    let h = captura_de_filas(
        "flota",
        vec![vec![celda("a"), con_bg(" ", VERDE), con_bg(" ", VERDE)]],
    );
    assert_eq!(
        exportar_bien(&[h]).fragmentos["flota"],
        format!("{PRE}a<span class=\"c0\">  </span></pre>")
    );
}

#[test]
fn una_primera_o_ultima_fila_vacia_se_escribe_con_un_espacio() {
    // Un `\n` justo tras `<pre>` lo ignoran los navegadores y uno justo antes de
    // `</pre>` no hace línea: sin el espacio, la captura perdería una fila.
    let r = exportar_bien(&[
        captura("flota", &["", "x"]),
        captura("archivos", &["x", ""]),
        captura("tuneles", &["a", "", "b"]),
        captura("servicios", &[""]),
        captura("deliberacion", &[]),
    ]);
    assert_eq!(r.fragmentos["flota"], format!("{PRE} \nx</pre>"));
    assert_eq!(r.fragmentos["archivos"], format!("{PRE}x\n </pre>"));
    assert_eq!(r.fragmentos["tuneles"], format!("{PRE}a\n\nb</pre>"));
    assert_eq!(r.fragmentos["servicios"], format!("{PRE} </pre>"));
    assert_eq!(r.fragmentos["deliberacion"], format!("{PRE}</pre>"));
    for f in r.fragmentos.values() {
        validar_fragmento(f).unwrap();
    }
}

#[test]
fn los_tramos_no_cruzan_filas_y_los_anchos_no_se_parten() {
    // Un carácter ancho ya viene sin su celda de continuación: una celda, un
    // símbolo. Un grafema de varios caracteres se queda entero.
    let mut cobertura = cobertura();
    cobertura.codigos.extend(['漢' as u32, 'e' as u32, 0x301]);
    let c = captura_de_filas(
        "flota",
        vec![
            vec![con_fg("漢", ROJO), con_fg("e\u{301}", ROJO)],
            vec![con_fg("x", ROJO)],
        ],
    );
    let r = exportar(&[c], &cobertura, &degradar).unwrap();
    assert_eq!(
        r.fragmentos["flota"],
        format!("{PRE}<span class=\"c0\">漢e\u{301}</span>\n<span class=\"c0\">x</span></pre>")
    );
}

// ---------------------------------------------------------------------------
// Glifos
// ---------------------------------------------------------------------------

#[test]
fn un_simbolo_sin_glifo_se_degrada_si_devuelve_un_caracter_con_glifo() {
    let r = exportar_bien(&[captura_de_filas(
        "flota",
        vec![vec![celda("a"), celda("⤴"), con_fg("⤴", ROJO)]],
    )]);
    // `>` va escapado.
    assert_eq!(
        r.fragmentos["flota"],
        format!("{PRE}a&gt;<span class=\"c0\">&gt;</span></pre>")
    );
    assert!(!r.fragmentos["flota"].contains('⤴'));

    // Un símbolo que ya tiene glifo no pasa por el degradador.
    let paso = std::cell::Cell::new(0);
    let cuenta = |s: &str| {
        paso.set(paso.get() + 1);
        degradar(s)
    };
    let r = exportar(&[captura("flota", &["a─│"])], &cobertura(), &cuenta).unwrap();
    assert_eq!(r.fragmentos["flota"], format!("{PRE}a─│</pre>"));
    assert_eq!(paso.get(), 0);

    // Un degradador que devuelve un espacio vale: es un carácter.
    let r = exportar(&[captura("flota", &["a↻b"])], &cobertura(), &|_| {
        " ".to_string()
    })
    .unwrap();
    assert_eq!(r.fragmentos["flota"], format!("{PRE}a b</pre>"));
}

#[test]
fn un_simbolo_sin_glifo_con_degradacion_larga_o_interrogacion_falla() {
    // Más de un carácter.
    let e = error_de(&[captura("flota", &["a↵"])]);
    assert!(e.contains("U+21B5"), "{e}");
    assert!(e.contains("enter"), "{e}");
    // `?`.
    let e = error_de(&[captura("flota", &["↻"])]);
    assert!(e.contains("U+21BB"), "{e}");
    assert!(e.contains('?'), "{e}");

    // Otras degradaciones que no valen.
    let casos: [(&str, &str); 7] = [
        ("vacía", ""),
        ("dos caracteres", "ab"),
        ("sin glifo", "é"),
        ("salto de línea", "\n"),
        ("control", "\u{7}"),
        ("tabulador", "\t"),
        ("grafema con marca", "e\u{301}"),
    ];
    for (nombre, devuelve) in casos {
        let r = exportar(&[captura("flota", &["↻"])], &cobertura(), &|_| {
            devuelve.to_string()
        });
        assert!(
            r.is_err(),
            "la degradación {nombre} ({devuelve:?}) debe fallar"
        );
    }

    // Un grafema con una parte sin glifo se degrada entero o falla entero.
    let c = captura_de_filas("flota", vec![vec![celda("e\u{301}")]]);
    assert!(exportar(std::slice::from_ref(&c), &cobertura(), &degradar).is_err());
    let r = exportar(&[c], &cobertura(), &|_| "e".to_string()).unwrap();
    assert_eq!(r.fragmentos["flota"], format!("{PRE}e</pre>"));
}

#[test]
fn un_invisible_que_falsea_la_lectura_falla_aunque_tenga_glifo() {
    // Aunque una cobertura lo anunciara, no puede salir en una captura: el error lo
    // dice con su ubicación y no es un «fallo interno» de la validación.
    let mut cobertura = cobertura();
    cobertura.codigos.extend([0x202E_u32, 0x200B]);
    let c = captura_de_filas(
        "flota",
        vec![fila_de("ab"), vec![celda("a"), celda("\u{202E}")]],
    );
    let e = exportar(&[c], &cobertura, &degradar)
        .expect_err("el invisible debe fallar")
        .to_string();
    assert!(
        e.contains("flota") && e.contains("fila 2") && e.contains("columna 2"),
        "{e}"
    );
    assert!(e.contains("U+202E"), "{e}");
    assert!(!e.contains("fallo interno"), "{e}");
    // Dentro de un símbolo con algo visible, igual.
    let c = captura_de_filas("flota", vec![vec![celda("a\u{200B}")]]);
    assert!(exportar(&[c], &cobertura, &degradar).is_err());
}

#[test]
fn el_error_de_glifo_dice_captura_fila_y_columna() {
    let mut filas = vec![fila_de("abc"), fila_de("def"), fila_de("ghi")];
    filas.push(vec![
        celda("a"),
        celda("b"),
        celda("c"),
        celda("d"),
        celda("e"),
        celda("↻"),
    ]);
    let c = captura_de_filas("tuneles", filas);
    let e = error_de(&[captura("flota", &["bien"]), c]);
    assert!(e.contains("tuneles"), "{e}");
    assert!(e.contains("fila 4"), "{e}");
    assert!(e.contains("columna 6"), "{e}");
    assert!(e.contains("U+21BB"), "{e}");
    assert!(e.contains('↻'), "{e}");
    // Dice que la numeración es desde 1 y qué devolvió el degradador.
    assert!(e.contains("desde 1"), "{e}");
    assert!(e.contains("\"?\""), "{e}");

    // El primer fallo es el que se cuenta, en orden de capturas, filas y columnas.
    let a = captura_de_filas("flota", vec![fila_de("x"), vec![celda("a"), celda("↵")]]);
    let b = captura_de_filas("archivos", vec![vec![celda("↻")]]);
    let e = error_de(&[a, b]);
    assert!(
        e.contains("flota") && e.contains("fila 2") && e.contains("columna 2"),
        "{e}"
    );
}

#[test]
fn los_controles_y_los_ids_raros_fallan_con_un_mensaje() {
    // Un símbolo visible con un salto de línea dentro no puede ir.
    let c = captura_de_filas("flota", vec![vec![celda("a\nb")]]);
    let e = error_de(&[c]);
    assert!(e.contains("U+000A") && e.contains("flota"), "{e}");
    // Un símbolo solo de controles y espacios es un espacio.
    let c = captura_de_filas(
        "flota",
        vec![vec![
            celda("a"),
            celda("\u{0}"),
            celda("\n"),
            celda("\u{a0}"),
            celda("b"),
        ]],
    );
    assert_eq!(
        exportar_bien(&[c]).fragmentos["flota"],
        format!("{PRE}a   b</pre>")
    );

    // Ids: válidos y repetidos.
    for malo in [
        "",
        "Flota",
        "a b",
        "../x",
        "a/b",
        "ñ",
        &"a".repeat(65),
        "a.html",
        "x\n",
    ] {
        let e = error_de(&[captura(malo, &["x"])]);
        assert!(e.contains("id"), "{malo:?}: {e}");
    }
    let e = error_de(&[captura("flota", &["x"]), captura("flota", &["y"])]);
    assert!(e.contains("repetido") && e.contains("flota"), "{e}");
    // Los de verdad, bien.
    for id in IDS {
        exportar_bien(&[captura(id, &["x"])]);
    }
    exportar_bien(&[]);
}

// ---------------------------------------------------------------------------
// Validación del CSS y de los fragmentos exportados
// ---------------------------------------------------------------------------

fn conjunto_variado() -> Vec<CapturaCeldas> {
    let estilos = [
        celda("x"),
        con_fg("x", ROJO),
        con_bg("x", VERDE),
        CeldaCaptura {
            negrita: true,
            fg: AMBAR,
            ..celda("x")
        },
        CeldaCaptura {
            invertido: true,
            ..celda("x")
        },
        CeldaCaptura {
            tenue: true,
            cursiva: true,
            ..celda("x")
        },
        CeldaCaptura {
            subrayado: true,
            bg: ColorCaptura::Indexado(236),
            ..celda("x")
        },
        CeldaCaptura {
            oculta: true,
            fg: ROJO,
            bg: VERDE,
            ..celda("x")
        },
    ];
    vec![
        captura_de_filas("flota", vec![estilos.to_vec(), fila_de("└──┘")]),
        captura_de_filas("archivos", vec![estilos.iter().rev().cloned().collect()]),
    ]
}

#[test]
fn el_css_exportado_pasa_validar_css() {
    for capturas in [vec![], vec![captura("flota", &["x"])], conjunto_variado()] {
        let r = exportar_bien(&capturas);
        validar_css(&r.css).unwrap_or_else(|e| panic!("{e}\n{}", r.css));
        // La base lleva los colores de la paleta fija y lo que evita que la
        // rejilla se descuadre.
        let base = r.css.lines().next().unwrap();
        for esperado in [
            "background:#0a0a0a",
            "color:#ede8dc",
            "font-variant-ligatures:none",
            "font-feature-settings:\"liga\" 0,\"calt\" 0",
            "white-space:pre",
            "overflow-x:auto",
            "margin:0",
            "line-height:",
            "font-family:\"JetBrains Mono\",\"MAGI Simbolos\"",
        ] {
            assert!(base.contains(esperado), "falta {esperado} en {base}");
        }
        // Un `\n` al final de cada regla, sin `style` ni nada de fuera.
        assert!(r.css.ends_with("}\n"));
        let minusculas = r.css.to_ascii_lowercase();
        for prohibido in ["url(", "@import", "expression(", "javascript:", "<", "\\"] {
            assert!(!minusculas.contains(prohibido), "{prohibido}");
        }
        // Los colores, siempre `#rrggbb` en minúsculas.
        for trozo in r.css.split('#').skip(1) {
            let hex: String = trozo.chars().take(6).collect();
            assert_eq!(hex.len(), 6);
            assert!(hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
        }
    }
}

#[test]
fn los_fragmentos_exportados_pasan_validar_fragmento() {
    let r = exportar_bien(&conjunto_variado());
    assert_eq!(r.fragmentos.len(), 2);
    for (id, f) in &r.fragmentos {
        validar_fragmento(f).unwrap_or_else(|e| panic!("{id}: {e}\n{f}"));
        assert!(f.starts_with(PRE) && f.ends_with("</pre>"));
        // Sin `style`, `id` ni nada que no sea la clase.
        for prohibido in ["style", "onclick", "href", "<script", "<a "] {
            assert!(!f.contains(prohibido), "{id}: {prohibido}");
        }
    }
    // Cada prefijo propio de un fragmento válido es inválido: nada a medias.
    let f = &r.fragmentos["flota"];
    for (i, _) in f.char_indices() {
        assert!(
            validar_fragmento(&f[..i]).is_err(),
            "el prefijo de {i} bytes no debería valer: {:?}",
            &f[..i]
        );
    }
}

#[test]
fn validar_fragmento_acepta_lo_correcto() {
    let buenos = [
        format!("{PRE}</pre>"),
        format!("{PRE}hola</pre>"),
        format!("{PRE}a\nb\n\nc</pre>"),
        format!("{PRE}<span class=\"c0\">a</span></pre>"),
        format!("{PRE}<span class=\"c0\"></span></pre>"),
        format!("{PRE}a<span class=\"c12\">b</span>c<span class=\"c3\">d</span></pre>"),
        format!("{PRE}&lt;&gt;&amp;&quot;&#39;</pre>"),
        format!("{PRE}<span class=\"c1\">&amp;&amp;</span></pre>"),
        // Comillas y `>` sin escapar son inertes en el texto.
        format!("{PRE}a > b \" c ' d</pre>"),
        format!("{PRE}┌──┐ 漢字 ⤴ é</pre>"),
        // Atributos en otro orden, mayúsculas en las etiquetas.
        "<pre aria-hidden=\"true\" class=\"captura\">x</pre>".to_string(),
        "<PRE CLASS=\"captura\" ARIA-HIDDEN=\"true\">x</PRE>".to_string(),
    ];
    for b in &buenos {
        validar_fragmento(b).unwrap_or_else(|e| panic!("{b:?}: {e}"));
    }
}

#[test]
fn validar_fragmento_rechaza_script_style_y_atributos_extra() {
    let casos: Vec<(&str, String)> = vec![
        ("script", format!("{PRE}<script>alert(1)</script></pre>")),
        (
            "script en mayúsculas",
            format!("{PRE}<SCRIPT>alert(1)</SCRIPT></pre>"),
        ),
        (
            "script con src",
            format!("{PRE}<script src=x></script></pre>"),
        ),
        (
            "script en un span",
            format!("{PRE}<span class=\"c1\"><script>x</script></span></pre>"),
        ),
        (
            "script antes",
            format!("<script>alert(1)</script>{PRE}x</pre>"),
        ),
        (
            "script después",
            format!("{PRE}x</pre><script>alert(1)</script>"),
        ),
        (
            "style",
            format!("{PRE}<style>pre{{display:none}}</style></pre>"),
        ),
        (
            "style en un span",
            format!("{PRE}<span class=\"c1\"><style>x</style></span></pre>"),
        ),
        (
            "atributo style en el pre",
            "<pre class=\"captura\" aria-hidden=\"true\" style=\"color:red\">x</pre>".to_string(),
        ),
        (
            "atributo style en un span",
            format!("{PRE}<span class=\"c1\" style=\"color:red\">x</span></pre>"),
        ),
        (
            "onclick en el pre",
            "<pre class=\"captura\" aria-hidden=\"true\" onclick=\"x()\">x</pre>".to_string(),
        ),
        (
            "onclick en un span",
            format!("{PRE}<span class=\"c1\" onclick=\"x()\">x</span></pre>"),
        ),
        (
            "onmouseover sin comillas",
            format!("{PRE}<span class=\"c1\" onmouseover=alert(1)>x</span></pre>"),
        ),
        (
            "id en el pre",
            "<pre class=\"captura\" aria-hidden=\"true\" id=\"x\">x</pre>".to_string(),
        ),
        (
            "id en un span",
            format!("{PRE}<span id=\"x\" class=\"c1\">x</span></pre>"),
        ),
        (
            "data- en el pre",
            "<pre class=\"captura\" aria-hidden=\"true\" data-x=\"1\">x</pre>".to_string(),
        ),
        (
            "atributo repetido",
            "<pre class=\"captura\" class=\"otra\" aria-hidden=\"true\">x</pre>".to_string(),
        ),
        (
            "atributo repetido en un span",
            format!("{PRE}<span class=\"c1\" class=\"c2\">x</span></pre>"),
        ),
        (
            "pre sin aria-hidden",
            "<pre class=\"captura\">x</pre>".to_string(),
        ),
        (
            "pre sin class",
            "<pre aria-hidden=\"true\">x</pre>".to_string(),
        ),
        ("pre sin atributos", "<pre>x</pre>".to_string()),
        (
            "pre con otra clase",
            "<pre class=\"otra\" aria-hidden=\"true\">x</pre>".to_string(),
        ),
        (
            "pre con dos clases",
            "<pre class=\"captura otra\" aria-hidden=\"true\">x</pre>".to_string(),
        ),
        (
            "aria-hidden falso",
            "<pre class=\"captura\" aria-hidden=\"false\">x</pre>".to_string(),
        ),
        (
            "clase con entidad",
            "<pre class=\"captu&#114;a\" aria-hidden=\"true\">x</pre>".to_string(),
        ),
        (
            "pre con autocierre",
            "<pre class=\"captura\" aria-hidden=\"true\"/>".to_string(),
        ),
        ("span sin clase", format!("{PRE}<span>x</span></pre>")),
        (
            "span con clase vacía",
            format!("{PRE}<span class=\"\">x</span></pre>"),
        ),
        (
            "span con clase que no es de estilo",
            format!("{PRE}<span class=\"rojo\">x</span></pre>"),
        ),
        (
            "span con c sin dígitos",
            format!("{PRE}<span class=\"c\">x</span></pre>"),
        ),
        (
            "span con dos clases",
            format!("{PRE}<span class=\"c1 c2\">x</span></pre>"),
        ),
        (
            "span con c mayúscula",
            format!("{PRE}<span class=\"C1\">x</span></pre>"),
        ),
        (
            "span con letras tras los dígitos",
            format!("{PRE}<span class=\"c1x\">x</span></pre>"),
        ),
        (
            "span con signo",
            format!("{PRE}<span class=\"c-1\">x</span></pre>"),
        ),
        (
            "span con demasiados dígitos",
            format!("{PRE}<span class=\"c1234567890\">x</span></pre>"),
        ),
        (
            "span con autocierre",
            format!("{PRE}<span class=\"c1\"/>x</pre>"),
        ),
        (
            "enlace",
            format!("{PRE}<a href=\"https://ejemplo.org\">x</a></pre>"),
        ),
        (
            "enlace javascript",
            format!("{PRE}<a href=\"javascript:alert(1)\">x</a></pre>"),
        ),
        ("img", format!("{PRE}<img src=x onerror=alert(1)></pre>")),
        ("svg", format!("{PRE}<svg onload=alert(1)></svg></pre>")),
        (
            "svg con script",
            format!("{PRE}<svg><script>alert(1)</script></svg></pre>"),
        ),
        ("math", format!("{PRE}<math><mi>x</mi></math></pre>")),
        (
            "iframe",
            format!("{PRE}<iframe src=\"https://ejemplo.org\"></iframe></pre>"),
        ),
        ("object", format!("{PRE}<object data=x></object></pre>")),
        ("embed", format!("{PRE}<embed src=x></pre>")),
        ("link", format!("{PRE}<link rel=stylesheet href=x></pre>")),
        (
            "meta",
            format!("{PRE}<meta http-equiv=refresh content=0></pre>"),
        ),
        ("base", format!("{PRE}<base href=x></pre>")),
        ("form", format!("{PRE}<form action=x><input></form></pre>")),
        ("button", format!("{PRE}<button>x</button></pre>")),
        ("textarea", format!("{PRE}<textarea></textarea></pre>")),
        ("title", format!("{PRE}<title>x</title></pre>")),
        ("noscript", format!("{PRE}<noscript>x</noscript></pre>")),
        ("template", format!("{PRE}<template>x</template></pre>")),
        ("br", format!("{PRE}a<br>b</pre>")),
        ("b", format!("{PRE}<b>x</b></pre>")),
        ("div", format!("{PRE}<div>x</div></pre>")),
        ("select", format!("{PRE}<select></select></pre>")),
        ("pre dentro del pre", format!("{PRE}<pre>x</pre></pre>")),
        (
            "span anidado",
            format!("{PRE}<span class=\"c1\"><span class=\"c2\">x</span></span></pre>"),
        ),
        (
            "span sin cerrar",
            format!("{PRE}<span class=\"c1\">x</pre>"),
        ),
        (
            "span sin cerrar al final",
            format!("{PRE}<span class=\"c1\">x"),
        ),
        ("cierre de span sobrante", format!("{PRE}x</span></pre>")),
        ("cierre de otra etiqueta", format!("{PRE}x</a></pre>")),
        ("cierre de script suelto", format!("{PRE}x</script></pre>")),
        ("pre sin cerrar", format!("{PRE}x")),
        (
            "pre sin cerrar con span",
            format!("{PRE}<span class=\"c1\">x</span>"),
        ),
        (
            "cierre del pre con atributos",
            format!("{PRE}x</pre class=\"captura\">"),
        ),
        ("cierre del pre con autocierre", format!("{PRE}x</pre/>")),
        ("cierre del pre sin abrir", "</pre>".to_string()),
        ("texto antes del pre", format!("x{PRE}y</pre>")),
        ("espacio antes del pre", format!(" {PRE}y</pre>")),
        ("salto de línea tras el pre", format!("{PRE}y</pre>\n")),
        ("texto tras el pre", format!("{PRE}y</pre>x")),
        ("dos pre", format!("{PRE}a</pre>{PRE}b</pre>")),
        ("comentario", format!("{PRE}<!-- x --></pre>")),
        ("comentario antes", format!("<!-- x -->{PRE}x</pre>")),
        (
            "comentario que esconde un cierre",
            format!("{PRE}<!--</pre>-->x</pre>"),
        ),
        ("doctype", format!("<!DOCTYPE html>{PRE}x</pre>")),
        ("instrucción", format!("{PRE}<?php x ?></pre>")),
        ("cdata", format!("{PRE}<![CDATA[x]]></pre>")),
        ("vacío", String::new()),
        ("solo espacios", "  \n".to_string()),
        ("solo texto", "hola".to_string()),
        ("solo un span", "<span class=\"c1\">x</span>".to_string()),
    ];
    for (nombre, html) in &casos {
        let r = validar_fragmento(html);
        assert!(r.is_err(), "«{nombre}» debería rechazarse: {html:?}");
    }

    // Y los mensajes dicen qué.
    let e = validar_fragmento(&format!("{PRE}x<script>y</script></pre>"))
        .unwrap_err()
        .to_string();
    assert!(e.contains("script"), "{e}");
    let e = validar_fragmento(&format!("{PRE}x<style>y</style></pre>"))
        .unwrap_err()
        .to_string();
    assert!(e.contains("style"), "{e}");
    let e =
        validar_fragmento("<pre class=\"captura\" aria-hidden=\"true\" onclick=\"x()\">x</pre>")
            .unwrap_err()
            .to_string();
    assert!(e.contains("onclick"), "{e}");
    let e = validar_fragmento(&format!(
        "{PRE}a\nb\n<span class=\"c1\" style=\"x\">c</span></pre>"
    ))
    .unwrap_err()
    .to_string();
    assert!(e.contains("style") && e.contains("línea 3"), "{e}");
    let e = validar_fragmento(&format!("{PRE}<a href=x>y</a></pre>"))
        .unwrap_err()
        .to_string();
    assert!(e.contains("<«a»>") || e.contains("«a»"), "{e}");

    // Más de 1 MiB, aunque sea todo texto válido.
    let enorme = format!("{PRE}{}</pre>", "a".repeat(1024 * 1024));
    let e = validar_fragmento(&enorme).unwrap_err().to_string();
    assert!(e.contains("1 MiB"), "{e}");
    let justo = format!("{PRE}{}</pre>", "a".repeat(1024 * 1024 - PRE.len() - 6));
    assert_eq!(justo.len(), 1024 * 1024);
    validar_fragmento(&justo).unwrap();
}

#[test]
fn validar_fragmento_rechaza_los_invisibles_que_falsean_la_lectura() {
    // Dirección del texto (U+202E lo invertiría), aislados, marcas, espacios de ancho
    // cero, separadores de línea y de párrafo, formato y marca de orden de bytes: un
    // fragmento versionado no puede esconder texto al revés.
    for raro in [
        '\u{202E}',
        '\u{202D}',
        '\u{202A}',
        '\u{202C}',
        '\u{2066}',
        '\u{2069}',
        '\u{200B}',
        '\u{200C}',
        '\u{200D}',
        '\u{200E}',
        '\u{200F}',
        '\u{2028}',
        '\u{2029}',
        '\u{2060}',
        '\u{206F}',
        '\u{FEFF}',
        '\u{061C}',
        '\u{E0000}',
        '\u{E0041}',
        '\u{E007F}',
    ] {
        let en_el_pre = format!("{PRE}a{raro}b</pre>");
        let en_un_span = format!("{PRE}<span class=\"c1\">a{raro}b</span></pre>");
        for html in [en_el_pre, en_un_span] {
            let e = validar_fragmento(&html)
                .expect_err(&format!("U+{:04X} debería rechazarse", raro as u32))
                .to_string();
            assert!(e.contains(&format!("U+{:04X}", raro as u32)), "{e}");
        }
    }
    // Lo de siempre se queda: acentos, cajas, símbolos y el espacio duro.
    validar_fragmento(&format!("{PRE}┌─┐ é ñ ⤴ ↵ « » — € \u{a0}</pre>")).unwrap();
}

#[test]
fn validar_fragmento_rechaza_entidades_raras_y_menor_sin_escapar() {
    let casos: Vec<(&str, String)> = vec![
        (
            "entidad hexadecimal",
            format!("{PRE}&#x3c;script&#x3e;</pre>"),
        ),
        (
            "entidad hexadecimal en mayúsculas",
            format!("{PRE}&#X3C;</pre>"),
        ),
        ("entidad decimal", format!("{PRE}&#60;</pre>")),
        (
            "entidad decimal del apóstrofo con cero",
            format!("{PRE}&#039;</pre>"),
        ),
        ("apóstrofo sin punto y coma", format!("{PRE}&#39</pre>")),
        ("entidad con nombre", format!("{PRE}&foo;</pre>")),
        ("entidad nbsp", format!("{PRE}&nbsp;</pre>")),
        ("entidad apos", format!("{PRE}&apos;</pre>")),
        ("amp sin punto y coma", format!("{PRE}&amp</pre>")),
        ("lt sin punto y coma", format!("{PRE}&lt x</pre>")),
        ("amp en mayúsculas", format!("{PRE}&AMP;</pre>")),
        ("ampersand suelto", format!("{PRE}a & b</pre>")),
        ("ampersand al final", format!("{PRE}a&</pre>")),
        ("ampersand doble", format!("{PRE}&&amp;</pre>")),
        ("entidad cortada", format!("{PRE}&am</pre>")),
        (
            "ampersand en un span",
            format!("{PRE}<span class=\"c1\">&x;</span></pre>"),
        ),
        ("menor sin escapar", format!("{PRE}a < b</pre>")),
        ("menor pegado al texto", format!("{PRE}a<b</pre>")),
        ("menor solo", format!("{PRE}<</pre>")),
        ("menor al final", format!("{PRE}a <")),
        ("menor y barra", format!("{PRE}a </ b</pre>")),
        ("menor con dígito", format!("{PRE}<3</pre>")),
        ("etiqueta sin cerrar", format!("{PRE}<span class=\"c1\"")),
        (
            "etiqueta sin cerrar con texto",
            format!("{PRE}<span class=\"c1\" x</pre>"),
        ),
        ("apertura sin nombre", format!("{PRE}<span</pre>")),
        (
            "comillas sin cerrar",
            format!("{PRE}<span class=\"c1>x</span></pre>"),
        ),
        (
            "menor en un span",
            format!("{PRE}<span class=\"c1\">a < b</span></pre>"),
        ),
        ("NUL", format!("{PRE}a\u{0}b</pre>")),
        ("retorno de carro", format!("{PRE}a\r\nb</pre>")),
        ("tabulador", format!("{PRE}a\tb</pre>")),
        ("escape", format!("{PRE}a\u{1b}[31mb</pre>")),
        ("DEL", format!("{PRE}a\u{7f}b</pre>")),
        ("NEL", format!("{PRE}a\u{85}b</pre>")),
    ];
    for (nombre, html) in &casos {
        let r = validar_fragmento(html);
        assert!(r.is_err(), "«{nombre}» debería rechazarse: {html:?}");
    }

    // Dice dónde: la línea.
    let e = validar_fragmento(&format!("{PRE}a\nb\nc &#x3c; d</pre>"))
        .unwrap_err()
        .to_string();
    assert!(e.contains("línea 3"), "{e}");
    assert!(e.contains("&#x3c;"), "{e}");
    let e = validar_fragmento(&format!("{PRE}a\nb\u{0}</pre>"))
        .unwrap_err()
        .to_string();
    assert!(e.contains("U+0000") && e.contains("línea 2"), "{e}");
}

#[test]
fn validar_css_acepta_lo_correcto() {
    let buenos = [
        "",
        "\n",
        ".captura{}",
        ".captura{background:#0a0a0a;color:#ede8dc}",
        ".captura { background : #0A0A0A ; color: #EDE8DC ; }",
        ".c0{color:#fff}\n.c1{background:#ffa000;font-weight:700}\n",
        ".c12{color:#ff0000;background:#00ff6a;font-weight:bold;font-style:italic;text-decoration:underline}",
        ".c3{text-decoration:underline line-through}",
        ".captura{font-family:\"JetBrains Mono\",\"MAGI Simbolos\",ui-monospace,monospace}",
        ".captura{font-variant-ligatures:none;font-feature-settings:\"liga\" 0,\"calt\" 0;tab-size:4}",
        ".captura{line-height:1.32;white-space:pre;overflow-x:auto;margin:0;padding:.75rem 1rem;display:block;font-size:.8125rem}",
        ".captura{margin:0 auto}",
        ".c0{color:transparent;background-color:#0a0a0a80}",
        ".c0{COLOR:#FFF}",
    ];
    for b in buenos {
        validar_css(b).unwrap_or_else(|e| panic!("{b:?}: {e}"));
    }
}

#[test]
fn validar_css_exige_a_cada_propiedad_su_tipo_de_valor() {
    // Una propiedad solo admite sus tipos: un color no es un tamaño ni una cadena, y
    // un tamaño no es un color.
    let malos: &[(&str, &str)] = &[
        ("color con un tamaño", ".c0{color:12px}"),
        ("color con una cadena", ".c0{color:\"red\"}"),
        ("color con dos colores", ".c0{color:#fff #000}"),
        ("color con dos palabras", ".c0{color:red blue}"),
        ("fondo con un tamaño", ".c0{background:1px}"),
        ("fondo con dos colores", ".c0{background-color:#fff #000}"),
        ("peso con un color", ".c0{font-weight:#fff}"),
        ("peso con una palabra rara", ".c0{font-weight:foo}"),
        ("estilo con un número", ".c0{font-style:12}"),
        ("decoración con un número", ".c0{text-decoration:1}"),
        ("tamaño con un color", ".captura{line-height:#fff}"),
        ("espacio con un número", ".captura{white-space:1}"),
        ("desborde con una palabra rara", ".captura{overflow-x:foo}"),
        ("tabulación con una palabra", ".captura{tab-size:red}"),
        ("tabulación con dos números", ".captura{tab-size:4 4}"),
        (
            "ligaduras con un número",
            ".captura{font-variant-ligatures:1}",
        ),
        (
            "etiqueta de fuente de cinco letras",
            ".captura{font-feature-settings:\"ligas\" 0}",
        ),
        (
            "etiqueta de fuente sin comillas",
            ".captura{font-feature-settings:liga 0}",
        ),
        (
            "valor de etiqueta que es un tamaño",
            ".captura{font-feature-settings:\"liga\" 1px}",
        ),
    ];
    for (nombre, css) in malos {
        assert!(
            validar_css(css).is_err(),
            "«{nombre}» debería rechazarse: {css:?}"
        );
    }

    // Un valor no puede tener más de 16 piezas, aunque cada una sea válida.
    let etiquetas = |n: usize| {
        (0..n)
            .map(|i| format!("\"li{:02}\" 1", i % 100))
            .collect::<Vec<_>>()
            .join(",")
    };
    validar_css(&format!(
        ".captura{{font-feature-settings:{}}}",
        etiquetas(8)
    ))
    .expect("16 piezas valen");
    let e = validar_css(&format!(
        ".captura{{font-feature-settings:{}}}",
        etiquetas(9)
    ))
    .expect_err("17 piezas no valen")
    .to_string();
    assert!(e.contains("16"), "{e}");
}

#[test]
fn validar_css_rechaza_url_import_y_llaves_anidadas() {
    let casos: &[(&str, &str)] = &[
        ("url", ".c0{background:url(https://ejemplo.org/x.png)}"),
        ("url en mayúsculas", ".c0{background:URL(x)}"),
        ("url con espacio", ".c0{background:url (x)}"),
        ("url con comillas", ".c0{background:url('x')}"),
        ("url con escape", ".c0{background:u\\72l(x)}"),
        ("url en color", ".c0{color:url(x)}"),
        ("image-set", ".c0{background:image-set(\"x\" 1x)}"),
        ("import", "@import url(x);.c0{color:#fff}"),
        ("import con cadena", "@import 'x.css';"),
        ("import dentro", ".c0{color:#fff;@import x}"),
        ("import como valor", ".c0{color:@import}"),
        ("media", "@media print{.c0{color:#fff}}"),
        ("font-face", "@font-face{font-family:x}"),
        ("charset", "@charset \"utf-8\";.c0{color:#fff}"),
        ("expression", ".c0{width:expression(alert(1))}"),
        (
            "expression en un valor permitido",
            ".c0{color:expression(alert(1))}",
        ),
        ("javascript", ".c0{background:javascript:alert(1)}"),
        (
            "javascript en un valor permitido",
            ".c0{color:javascript:alert(1)}",
        ),
        ("barra invertida", ".c0{color:\\72ed}"),
        ("menor", ".c0{color:<}"),
        ("regla anidada", ".c0{color:#fff;.c1{color:#000}}"),
        ("regla anidada con &", ".c0{&.c1{color:#000}}"),
        ("llave abierta en el valor", ".c0{color:{#fff}}"),
        ("llaves dentro de .captura", ".captura{.c0{color:#fff}}"),
        ("llave de más", ".c0{color:#fff}}"),
        ("llave de más con regla", ".c0{color:#fff}}.c1{color:#000}"),
        ("regla sin cerrar", ".c0{color:#fff"),
        ("regla sin cerrar con punto y coma", ".c0{color:#fff;"),
        ("sin llave", ".c0"),
        ("solo una llave", "{"),
        (
            "cierre colado en un valor",
            ".c0{color:#fff}.c1{color:#000;}}",
        ),
        (
            "punto y coma y cierre colados",
            ".c0{color:#fff;}body{color:red}",
        ),
        ("comentario", "/* x */.c0{color:#fff}"),
        ("comentario dentro", ".c0{color:#fff/* x */}"),
        (
            "comentario que esconde",
            ".c0{color:#fff;/*}*/background:url(x)}",
        ),
        ("comentario sin cerrar", ".c0{color:#fff}/*"),
        ("cdo", "<!--.c0{color:#fff}-->"),
        ("selector de etiqueta", "body{color:#fff}"),
        ("selector universal", "*{color:#fff}"),
        ("selector de id", "#x{color:#fff}"),
        ("selector compuesto con espacio", ".c0 .c1{color:#fff}"),
        ("selector con coma", ".c0,.c1{color:#fff}"),
        ("selector con hijo", ".captura>span{color:#fff}"),
        ("selector con seudoclase", ".c0:hover{color:#fff}"),
        ("selector con seudoelemento", ".captura::before{color:#fff}"),
        ("selector con atributo", ".c0[x]{color:#fff}"),
        ("selector doble", ".c0.c1{color:#fff}"),
        ("selector sin dígitos", ".c{color:#fff}"),
        ("selector con letras", ".c1a{color:#fff}"),
        ("selector parecido", ".captura2{color:#fff}"),
        ("selector en mayúsculas", ".Captura{color:#fff}"),
        ("selector con punto doble", "..c1{color:#fff}"),
        ("selector vacío", "{color:#fff}"),
        ("selector con salto de línea raro", ".c0\u{a0}{color:#fff}"),
        ("propiedad fuera de la lista", ".c0{position:fixed}"),
        ("propiedad content", ".c0{content:\"x\"}"),
        ("propiedad con prefijo", ".c0{-moz-binding:url(x)}"),
        ("propiedad background-image", ".c0{background-image:url(x)}"),
        ("propiedad sin dos puntos", ".c0{color}"),
        ("propiedad vacía", ".c0{:#fff}"),
        ("valor vacío", ".c0{color:}"),
        ("valor con !important", ".c0{color:#fff!important}"),
        ("color mal formado", ".c0{color:#zzzzzz}"),
        ("color de dos dígitos", ".c0{color:#ff}"),
        ("color con función", ".c0{color:rgb(1,2,3)}"),
        ("fuente con comilla simple", ".c0{font-family:'x'}"),
        ("fuente con punto y coma", ".captura{font-family:\"a;b\"}"),
        ("display flex", ".c0{display:flex}"),
    ];
    for (nombre, css) in casos {
        let r = validar_css(css);
        assert!(r.is_err(), "«{nombre}» debería rechazarse: {css:?}");
    }

    let casos_mas: &[(&str, &str)] = &[
        (
            "margen con cinco medidas",
            ".captura{margin:1px 2px 3px 4px 5px}",
        ),
        ("número mal formado", ".captura{line-height:1.2.3}"),
        ("unidad rara", ".captura{padding:1vmax}"),
        ("color en un tamaño", ".captura{font-size:#fff}"),
        ("espacios con no ASCII", ".captura{color:#fff\u{a0}}"),
        ("no ASCII en un valor", ".c0{font-family:\"é\"}"),
        ("no ASCII fuera de reglas", ".c0{color:#fff}é"),
        ("basura fuera de reglas", ".c0{color:#fff} x"),
    ];
    for (nombre, css) in casos_mas {
        assert!(
            validar_css(css).is_err(),
            "«{nombre}» debería rechazarse: {css:?}"
        );
    }

    // Mensajes que dicen qué.
    let e = validar_css(".c0{background:url(x)}")
        .unwrap_err()
        .to_string();
    assert!(e.contains("url("), "{e}");
    let e = validar_css("@import url(x);").unwrap_err().to_string();
    assert!(e.contains("@"), "{e}");
    let e = validar_css(".c0{color:#fff}\n.c1{color:#000;.c2{color:#111}}")
        .unwrap_err()
        .to_string();
    assert!(e.contains("anidadas") && e.contains("línea 2"), "{e}");
    let e = validar_css(".c0{position:fixed}").unwrap_err().to_string();
    assert!(e.contains("position"), "{e}");
    let e = validar_css("body{color:#fff}").unwrap_err().to_string();
    assert!(e.contains("body"), "{e}");

    // Más de 1 MiB.
    let enorme = format!(".c0{{color:#fff}}{}", " ".repeat(1024 * 1024));
    let e = validar_css(&enorme).unwrap_err().to_string();
    assert!(e.contains("1 MiB"), "{e}");
}

// ---------------------------------------------------------------------------
// Basura: nunca pánico
// ---------------------------------------------------------------------------

#[test]
fn ni_exportar_ni_validar_hacen_panico_con_basura() {
    let mut azar = Lcg(0x5eed_c0de_2026);

    // 1. Cadenas de piezas peligrosas para los dos validadores.
    let piezas: Vec<&str> = vec![
        "<",
        ">",
        "&",
        "&amp;",
        "&lt;",
        "&#x3c;",
        "&#",
        "\"",
        "'",
        "/",
        "\\",
        "{",
        "}",
        ";",
        ":",
        ",",
        "#",
        "@",
        "!",
        "(",
        ")",
        "=",
        "\n",
        "\r",
        "\t",
        " ",
        "\u{0}",
        "\u{85}",
        "é",
        "漢",
        "🙂",
        "e\u{301}",
        "\u{feff}",
        "\u{202e}",
        "<pre class=\"captura\" aria-hidden=\"true\">",
        "<pre",
        "</pre>",
        "</",
        "<span class=\"c1\">",
        "<span",
        "</span>",
        "<script>",
        "</script>",
        "<style>",
        "<!--",
        "-->",
        "<![CDATA[",
        "<?",
        "<!DOCTYPE",
        ".c1",
        ".c",
        ".captura",
        "color",
        "background",
        "url(",
        "@import",
        "expression(",
        "javascript:",
        "/*",
        "*/",
        "#fff",
        "#12345",
        "1.2.3",
        "px",
        "%",
        "-",
        "+",
        "--x",
        "0",
        "9999999999",
    ];
    for _ in 0..4000 {
        let n = azar.hasta(40);
        let mut s = String::new();
        for _ in 0..n {
            s.push_str(azar.elegir(&piezas));
        }
        let _ = validar_fragmento(&s);
        let _ = validar_css(&s);
        // Con la cabecera correcta, para llegar más hondo.
        let _ = validar_fragmento(&format!("{PRE}{s}"));
        let _ = validar_fragmento(&format!("{PRE}{s}</pre>"));
        let _ = validar_css(&format!(".c0{{{s}}}"));
        let _ = validar_css(&format!(".captura{{color:{s}}}"));
    }

    // 2. Mutaciones de un fragmento y un CSS válidos: un carácter menos, uno de más
    // o uno cambiado, en cualquier posición.
    let r = exportar_bien(&conjunto_variado());
    let fragmento = r.fragmentos["flota"].clone();
    let css = r.css.clone();
    let alfabeto = [
        '<', '>', '&', ';', '"', '\'', '{', '}', '\u{0}', 'é', '漢', ' ', '\n', '\\',
    ];
    for base in [fragmento.as_str(), css.as_str()] {
        let limites: Vec<usize> = base
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(base.len()))
            .collect();
        for _ in 0..1500 {
            let i = *azar.elegir(&limites);
            let c = *azar.elegir(&alfabeto);
            let mut mutada = String::from(base);
            match azar.hasta(3) {
                0 => mutada.insert(i, c),
                1 => {
                    if i < base.len() {
                        mutada.remove(i);
                    }
                }
                _ => {
                    if i < base.len() {
                        mutada.remove(i);
                        mutada.insert(i, c);
                    }
                }
            }
            let _ = validar_fragmento(&mutada);
            let _ = validar_css(&mutada);
        }
        // Todos los prefijos, en límites de carácter.
        for i in &limites {
            let _ = validar_fragmento(&base[..*i]);
            let _ = validar_css(&base[..*i]);
        }
    }

    // 3. Capturas al azar, con símbolos raros y degradadores que devuelven cualquier
    // cosa: o salen bien (y entonces lo exportado es válido) o fallan con un error.
    let simbolos: Vec<&str> = vec![
        "",
        " ",
        "  ",
        "a",
        "Z",
        "~",
        "<",
        ">",
        "&",
        "\"",
        "'",
        "─",
        "│",
        "█",
        "⤴",
        "↵",
        "↻",
        "é",
        "漢",
        "🙂",
        "e\u{301}",
        "\u{0}",
        "\u{7}",
        "\t",
        "\n",
        "\r\n",
        "a\nb",
        "\u{a0}",
        "\u{200b}",
        "\u{200d}",
        "\u{feff}",
        "\u{85}",
        "\u{2028}",
        "<script>",
        "&amp;",
        "abc",
        "\u{1b}[0m",
        "\u{10ffff}",
        "\u{fffd}",
    ];
    let degradadores: [fn(&str) -> String; 6] = [
        |_| String::new(),
        |_| "?".to_string(),
        |_| ">".to_string(),
        |_| "<".to_string(),
        |s| s.to_string(),
        |_| "\n".to_string(),
    ];
    let colores = [
        ColorCaptura::Reset,
        ROJO,
        VERDE,
        ColorCaptura::Indexado(0),
        ColorCaptura::Indexado(255),
        ColorCaptura::Indexado(100),
    ];
    for vuelta in 0..300 {
        let ncapturas = azar.hasta(4);
        let mut capturas = Vec::new();
        for k in 0..ncapturas {
            let nfilas = azar.hasta(5);
            let mut filas = Vec::new();
            for _ in 0..nfilas {
                let ncol = azar.hasta(12);
                let mut fila = Vec::new();
                for _ in 0..ncol {
                    fila.push(CeldaCaptura {
                        simbolo: azar.elegir(&simbolos).to_string(),
                        fg: *azar.elegir(&colores),
                        bg: *azar.elegir(&colores),
                        negrita: azar.hasta(4) == 0,
                        tenue: azar.hasta(4) == 0,
                        cursiva: azar.hasta(4) == 0,
                        subrayado: azar.hasta(4) == 0,
                        invertido: azar.hasta(4) == 0,
                        oculta: azar.hasta(6) == 0,
                    });
                }
                filas.push(fila);
            }
            // Algún id raro de vez en cuando.
            let id = if azar.hasta(10) == 0 {
                "ID raro".to_string()
            } else {
                format!("captura-{vuelta}-{k}")
            };
            capturas.push(CapturaCeldas { id, filas });
        }
        let degradador = degradadores[azar.hasta(degradadores.len())];
        for c in &capturas {
            let _ = c.texto();
        }
        if let Ok(r) = exportar(&capturas, &cobertura(), &degradador) {
            validar_css(&r.css).unwrap();
            for (id, f) in &r.fragmentos {
                validar_fragmento(f).unwrap_or_else(|e| panic!("{id}: {e}\n{f}"));
            }
        }
    }

    // 4. Cobertura con símbolos arbitrarios.
    let c = cobertura();
    for s in &simbolos {
        let _ = c.cubre(s);
    }
    assert!(Cobertura::default().cubre(""));
    assert!(Cobertura::default().cubre("  \t\n"));
    assert!(!Cobertura::default().cubre("a"));
    assert!(c.cubre("a─ b") && !c.cubre("a↻") && !c.cubre("é"));
}

// ---------------------------------------------------------------------------
// Las seis capturas de la portada
// ---------------------------------------------------------------------------

/// Una captura sintética de 100×30 con estilos variados: bordes, cabecera
/// invertida, texto de colores y fondos, y alguna fila vacía.
fn captura_sintetica(id: &str, semilla: u64) -> CapturaCeldas {
    let mut azar = Lcg(semilla);
    let colores = [
        ColorCaptura::Reset,
        ROJO,
        VERDE,
        AMBAR,
        ColorCaptura::Indexado(8),
        ColorCaptura::Indexado(236),
        ColorCaptura::Rgb(0x9a, 0x96, 0x8c),
    ];
    let letras: Vec<char> = "abcdefghijklmnopqrstuvwxyz0123456789 -_/.:│─┌┐└┘█<>&\"'"
        .chars()
        .collect();
    let mut filas = Vec::new();
    for y in 0..FILAS {
        let mut fila = Vec::new();
        // Tramos de longitud variable con el mismo estilo.
        let mut x = 0;
        while x < COLUMNAS {
            let largo = (1 + azar.hasta(12)).min(COLUMNAS - x);
            let estilo = CeldaCaptura {
                fg: *azar.elegir(&colores),
                bg: if y == 0 || azar.hasta(5) == 0 {
                    *azar.elegir(&colores)
                } else {
                    ColorCaptura::Reset
                },
                negrita: azar.hasta(6) == 0,
                tenue: azar.hasta(9) == 0,
                cursiva: azar.hasta(12) == 0,
                subrayado: azar.hasta(10) == 0,
                invertido: y == 0 || azar.hasta(15) == 0,
                oculta: azar.hasta(40) == 0,
                ..celda(" ")
            };
            for _ in 0..largo {
                fila.push(CeldaCaptura {
                    simbolo: azar.elegir(&letras).to_string(),
                    ..estilo.clone()
                });
            }
            x += largo;
        }
        // Algunas filas casi vacías, y la última (barra de estado) llena.
        if y % 7 == 3 {
            fila.clear();
            fila.extend(std::iter::repeat_with(|| celda(" ")).take(COLUMNAS));
        }
        filas.push(fila);
    }
    CapturaCeldas {
        id: id.to_string(),
        filas,
    }
}

#[test]
fn el_fragmento_de_ejemplo_pasa_la_validacion_con_las_seis_capturas() {
    let capturas: Vec<CapturaCeldas> = IDS
        .iter()
        .enumerate()
        .map(|(i, id)| captura_sintetica(id, 1000 + i as u64))
        .collect();
    for c in &capturas {
        assert_eq!(c.filas.len(), FILAS);
        assert!(c.filas.iter().all(|f| f.len() == COLUMNAS));
    }
    let r = exportar_bien(&capturas);

    // Las seis, con los ids de la portada.
    let ids: BTreeSet<&str> = r.fragmentos.keys().map(String::as_str).collect();
    assert_eq!(ids, IDS.iter().copied().collect::<BTreeSet<_>>());

    validar_css(&r.css).unwrap_or_else(|e| panic!("{e}"));
    let mut usadas: BTreeSet<String> = BTreeSet::new();
    for (c, id) in capturas.iter().zip(IDS) {
        let f = &r.fragmentos[id];
        validar_fragmento(f).unwrap_or_else(|e| panic!("{id}: {e}"));
        // Treinta filas, ni una más ni una menos.
        assert_eq!(f.split('\n').count(), FILAS, "{id}");
        // Y lo que se ve es lo que dice `texto()`.
        assert_eq!(
            lineas_visibles(f),
            c.texto().split('\n').collect::<Vec<_>>(),
            "{id}"
        );
        // Las clases del fragmento existen todas en el CSS.
        for trozo in f.split("<span class=\"").skip(1) {
            let clase = trozo.split('"').next().unwrap().to_string();
            usadas.insert(clase);
        }
    }
    let del_css: BTreeSet<String> = reglas(&r.css)
        .into_iter()
        .map(|(s, _)| s)
        .filter(|s| s != ".captura")
        .map(|s| s.trim_start_matches('.').to_string())
        .collect();
    assert!(!usadas.is_empty());
    assert_eq!(usadas, del_css, "cada clase del CSS se usa y viceversa");
    // Numeradas sin huecos desde 0.
    for n in 0..del_css.len() {
        assert!(del_css.contains(&format!("c{n}")), "falta c{n}");
    }
    // Un CSS compartido razonable.
    assert!(r.css.len() < 64 * 1024);
}
