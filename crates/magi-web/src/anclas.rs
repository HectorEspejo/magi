//! Anclas de los títulos de un markdown, calculadas como las calcula GitHub.
//!
//! Es el algoritmo que usaba `repositorio_publico` (D452): vive aquí para que solo
//! haya una copia, y la prueba lo usa desde `magi-web`. `preparar-manual` pone a
//! cada título del manual su `{#id}` con estas mismas funciones, de modo que los
//! enlaces `capitulo.md#ancla` funcionan igual en GitHub y en la web.

use std::collections::HashMap;

/// Los encabezados de un markdown (fuera de bloques de código): `(nivel, texto)`.
pub fn encabezados_de(texto: &str) -> Vec<(usize, String)> {
    let mut dentro = false;
    let mut res = Vec::new();
    for linea in texto.lines() {
        let l = linea.trim_start();
        if l.starts_with("```") || l.starts_with("~~~") {
            dentro = !dentro;
            continue;
        }
        if dentro {
            continue;
        }
        let almohadillas = linea.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&almohadillas) && linea.chars().nth(almohadillas) == Some(' ') {
            let texto = linea[almohadillas..]
                .trim()
                .trim_end_matches('#')
                .trim()
                .to_string();
            res.push((almohadillas, texto));
        }
    }
    res
}

/// El slug de GitHub de un encabezado, sin el sufijo de duplicados.
pub fn slug_base(texto: &str) -> String {
    // Los textos de enlaces `[a](b)` cuentan como `a`.
    let mut limpio = String::new();
    let mut chars = texto.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '`' {
            continue;
        }
        if c == '[' {
            let mut interior = String::new();
            let mut cerrado = false;
            for d in chars.by_ref() {
                if d == ']' {
                    cerrado = true;
                    break;
                }
                interior.push(d);
            }
            if cerrado && chars.peek() == Some(&'(') {
                for d in chars.by_ref() {
                    if d == ')' {
                        break;
                    }
                }
                limpio.push_str(&interior);
            } else {
                limpio.push('[');
                limpio.push_str(&interior);
                if cerrado {
                    limpio.push(']');
                }
            }
            continue;
        }
        limpio.push(c);
    }
    limpio
        .trim()
        .to_lowercase()
        .chars()
        .filter_map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                Some(c)
            } else if c.is_whitespace() {
                Some('-')
            } else {
                None
            }
        })
        .collect()
}

/// Todos los slugs de un markdown, con el sufijo `-1`, `-2` de los duplicados.
pub fn slugs_de(texto: &str) -> Vec<String> {
    let mut vistos: HashMap<String, usize> = HashMap::new();
    encabezados_de(texto)
        .into_iter()
        .map(|(_, t)| {
            let base = slug_base(&t);
            let n = vistos.entry(base.clone()).or_insert(0);
            let slug = if *n == 0 {
                base.clone()
            } else {
                format!("{base}-{n}")
            };
            *n += 1;
            slug
        })
        .collect()
}
