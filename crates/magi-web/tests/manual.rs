//! `magi-web preparar-manual` contra su API pública: `SUMMARY.md`, anclas de
//! GitHub, enlaces que salen del manual, HTML en bruto, preprocesadores, lectura
//! de la etiqueta con git y escritura atómica de la salida.
//!
//! Sin red. Las pruebas con git crean un repositorio de verdad en una carpeta
//! temporal (sin tocar la configuración del equipo): si `git` no está, se omiten
//! con un aviso salvo que `CI` esté puesta, y entonces fallan. La que construye el
//! libro con mdBook solo corre con `MAGI_WEB_MDBOOK` (la ruta de `mdbook`); con
//! `MAGI_EXIGIR_MDBOOK=1` y sin ella, falla en vez de omitirse.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use magi_web::anclas::slugs_de;
use magi_web::error::{ErrorWeb, Resultado};
use magi_web::manual::{
    con_anclas, escapar_html_suelto, preparar_manual, rechazar_menor_que_en_titulos,
    rechazar_preprocesadores, rechazar_tablas_alineadas, reescribir_enlaces, summary,
    InformeManual, OpcionesManual, OrigenManual,
};
use pulldown_cmark::{Event, LinkType, Options, Parser, Tag};

const ETIQUETA: &str = "v1.6.1";
const GITHUB: &str = "https://github.com/HectorEspejo/magi";

// ---------------------------------------------------------------------------
// Utilidades
// ---------------------------------------------------------------------------

fn fijos() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fijos/manual")
}

fn libro_de_ejemplo() -> PathBuf {
    fijos().join("libro")
}

fn tema_de_ejemplo() -> PathBuf {
    fijos().join("tema")
}

fn raiz() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("raíz del repositorio")
}

/// Los mismos ajustes de Markdown que usa mdBook (y `manual.rs`).
fn opciones() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_GFM
        | Options::ENABLE_DEFINITION_LIST
}

/// El mensaje del error de un resultado que tiene que haber fallado.
fn fallo<T: std::fmt::Debug>(resultado: Resultado<T>) -> String {
    match resultado {
        Ok(v) => panic!("tenía que fallar y dio {v:?}"),
        Err(e) => e.to_string(),
    }
}

fn nombres(lista: &[&str]) -> Vec<String> {
    lista.iter().map(|s| (*s).to_string()).collect()
}

/// Los ids de los títulos tal como los ve el analizador de Markdown.
fn ids_de(markdown: &str) -> Vec<Option<String>> {
    Parser::new_ext(markdown, opciones())
        .filter_map(|e| match e {
            Event::Start(Tag::Heading { id, .. }) => Some(id.map(|i| i.to_string())),
            _ => None,
        })
        .collect()
}

/// Los destinos de enlaces en línea, imágenes y definiciones, en orden de aparición.
fn destinos_de(markdown: &str) -> Vec<String> {
    let mut analizador = Parser::new_ext(markdown, opciones()).into_offset_iter();
    let mut destinos = Vec::new();
    for (evento, _) in analizador.by_ref() {
        match evento {
            Event::Start(Tag::Link {
                link_type: LinkType::Inline,
                dest_url,
                ..
            })
            | Event::Start(Tag::Image {
                link_type: LinkType::Inline,
                dest_url,
                ..
            }) => destinos.push(dest_url.to_string()),
            _ => {}
        }
    }
    let mut definiciones: Vec<(usize, String)> = analizador
        .reference_definitions()
        .iter()
        .map(|(_, d)| (d.span.start, d.dest.to_string()))
        .collect();
    definiciones.sort();
    destinos.extend(definiciones.into_iter().map(|(_, d)| d));
    destinos
}

/// ¿Hay HTML en bruto (bloque o en línea) fuera de código?
fn hay_html(markdown: &str) -> bool {
    Parser::new_ext(markdown, opciones())
        .any(|e| matches!(e, Event::Html(_) | Event::InlineHtml(_)))
}

/// Reescribe como si fuera un capítulo del manual de ejemplo.
fn enlazar(markdown: &str) -> Resultado<String> {
    reescribir_enlaces(
        markdown,
        "tuneles.md",
        ETIQUETA,
        &nombres(&["primeros-pasos.md", "tuneles.md"]),
        "t.md",
    )
}

fn enlazado(markdown: &str) -> String {
    enlazar(markdown).unwrap_or_else(|e| panic!("{markdown:?} debía reescribirse: {e}"))
}

/// Todo lo que cuelga de `dir`, con su contenido (rutas relativas con `/`).
fn arbol_de(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn recorrer(raiz: &Path, dir: &Path, mapa: &mut BTreeMap<String, Vec<u8>>) {
        for entrada in fs::read_dir(dir).unwrap() {
            let entrada = entrada.unwrap();
            let ruta = entrada.path();
            let relativa = ruta
                .strip_prefix(raiz)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if entrada.file_type().unwrap().is_dir() {
                mapa.insert(format!("{relativa}/"), Vec::new());
                recorrer(raiz, &ruta, mapa);
            } else {
                mapa.insert(relativa, fs::read(&ruta).unwrap());
            }
        }
    }
    let mut mapa = BTreeMap::new();
    recorrer(dir, dir, &mut mapa);
    mapa
}

fn copiar_arbol(origen: &Path, destino: &Path) {
    fs::create_dir_all(destino).unwrap();
    for entrada in fs::read_dir(origen).unwrap() {
        let entrada = entrada.unwrap();
        let hacia = destino.join(entrada.file_name());
        if entrada.file_type().unwrap().is_dir() {
            copiar_arbol(&entrada.path(), &hacia);
        } else {
            fs::copy(entrada.path(), hacia).unwrap();
        }
    }
}

fn opciones_con_arbol(carpeta: &Path, salida: &Path) -> OpcionesManual {
    OpcionesManual {
        origen: OrigenManual::Arbol {
            carpeta: carpeta.to_path_buf(),
        },
        etiqueta: ETIQUETA.to_string(),
        tema: tema_de_ejemplo(),
        salida: salida.to_path_buf(),
    }
}

/// Una copia del manual de ejemplo para tocarla.
fn copia_del_libro(temporal: &Path) -> PathBuf {
    let carpeta = temporal.join("manual");
    copiar_arbol(&libro_de_ejemplo(), &carpeta);
    carpeta
}

fn preparar(carpeta: &Path, salida: &Path) -> Resultado<InformeManual> {
    preparar_manual(&opciones_con_arbol(carpeta, salida))
}

// ---------------------------------------------------------------------------
// SUMMARY.md
// ---------------------------------------------------------------------------

const README_DE_EJEMPLO: &str = "\
# Manual

Intro.

## Capítulos

- [Instalación, actualización y desinstalación](instalacion.md) — Requisitos
- [Primeros pasos y diagnóstico](primeros-pasos.md) — Primeros pasos
- [El `Include` de `~/.ssh/config`](ssh-config.md) — Cómo se integra

## Qué se ha probado

[`estado.md`](estado.md) lo dice.
";

fn ficheros_de_ejemplo() -> Vec<String> {
    nombres(&[
        "README.md",
        "estado.md",
        "instalacion.md",
        "primeros-pasos.md",
        "ssh-config.md",
    ])
}

#[test]
fn summary_sale_del_indice_con_estado_al_final() {
    let resultado = summary(README_DE_EJEMPLO, &ficheros_de_ejemplo()).unwrap();
    assert_eq!(
        resultado,
        "[Manual de MAGI](README.md)\n\
         \n\
         - [Instalación, actualización y desinstalación](instalacion.md)\n\
         - [Primeros pasos y diagnóstico](primeros-pasos.md)\n\
         - [El Include de ~/.ssh/config](ssh-config.md)\n\
         - [Qué se ha probado](estado.md)\n"
    );

    // El orden es el del índice, no el de los ficheros, y los ficheros que no son
    // `.md` (las imágenes) no cuentan.
    let mut revueltos = ficheros_de_ejemplo();
    revueltos.reverse();
    revueltos.push("captura.png".to_string());
    assert_eq!(summary(README_DE_EJEMPLO, &revueltos).unwrap(), resultado);

    // Con finales de línea de Windows sale lo mismo.
    let crlf = README_DE_EJEMPLO.replace('\n', "\r\n");
    assert_eq!(summary(&crlf, &ficheros_de_ejemplo()).unwrap(), resultado);
}

#[test]
fn el_indice_es_solo_la_seccion_capitulos() {
    // Una lista parecida fuera de «## Capítulos» (antes y después) o dentro de un
    // bloque de código no cuenta; el bloque acaba en el siguiente `## `.
    let readme = "\
# Manual

- [Antes](antes.md)

```md
## Capítulos
- [En código](codigo.md)
```

## Capítulos

Texto del índice.

- [Uno](uno.md)

```md
## Otro
- [Dentro](dentro.md)
```

### Subsección

- [Dos](dos.md)

## Otras guías

- [Después](despues.md)
";
    let ficheros = nombres(&["README.md", "estado.md", "uno.md", "dos.md"]);
    // `antes.md`, `codigo.md`, `dentro.md` y `despues.md` ni siquiera existen: no
    // pueden ser del índice.
    assert_eq!(
        summary(readme, &ficheros).unwrap(),
        "[Manual de MAGI](README.md)\n\n- [Uno](uno.md)\n- [Dos](dos.md)\n- [Qué se ha probado](estado.md)\n"
    );
}

#[test]
fn un_capitulo_fuera_del_indice_falla_diciendo_cual() {
    let mut ficheros = ficheros_de_ejemplo();
    // En un orden que no es el alfabético ni su inverso.
    ficheros.push("mmm.md".to_string());
    ficheros.push("zeta.md".to_string());
    ficheros.push("alfa.md".to_string());
    let mensaje = fallo(summary(README_DE_EJEMPLO, &ficheros));
    assert!(
        mensaje.contains("capítulo fuera del índice: alfa.md, mmm.md, zeta.md"),
        "{mensaje}"
    );

    // Uno solo.
    let mut ficheros = ficheros_de_ejemplo();
    ficheros.push("otro.md".to_string());
    let mensaje = fallo(summary(README_DE_EJEMPLO, &ficheros));
    assert!(
        mensaje.contains("capítulo fuera del índice: otro.md"),
        "{mensaje}"
    );

    // `README.md` y `estado.md` nunca sobran, pero `estado.md` tiene que existir.
    let sin_estado = nombres(&[
        "README.md",
        "instalacion.md",
        "primeros-pasos.md",
        "ssh-config.md",
    ]);
    let mensaje = fallo(summary(README_DE_EJEMPLO, &sin_estado));
    assert!(mensaje.contains("estado.md"), "{mensaje}");

    // Un `.md` con un nombre raro tampoco se cuela: sobra y se dice.
    let mut ficheros = ficheros_de_ejemplo();
    ficheros.push("Notas_Raras.md".to_string());
    let mensaje = fallo(summary(README_DE_EJEMPLO, &ficheros));
    assert!(mensaje.contains("Notas_Raras.md"), "{mensaje}");
}

#[test]
fn una_linea_enorme_en_el_indice_no_cuesta_tiempo_cuadratico() {
    // Una sola línea de 1,5 MB con medio millón de «](» y ningún `)`: buscar el cierre
    // en cada vuelta tardaba decenas de segundos.
    let readme = format!("## Capítulos\n\n- [{}\n", "](x".repeat(500_000));
    let inicio = std::time::Instant::now();
    let mensaje = fallo(summary(&readme, &ficheros_de_ejemplo()));
    assert!(mensaje.contains("README.md:3"), "{mensaje}");
    assert!(
        inicio.elapsed() < std::time::Duration::from_secs(5),
        "tardó {:?}",
        inicio.elapsed()
    );
    // Y con un `)` al final, igual.
    let readme = format!("## Capítulos\n\n- [{})\n", "](x".repeat(500_000));
    let inicio = std::time::Instant::now();
    let _ = summary(&readme, &ficheros_de_ejemplo());
    assert!(
        inicio.elapsed() < std::time::Duration::from_secs(5),
        "tardó {:?}",
        inicio.elapsed()
    );
}

#[test]
fn un_capitulo_del_indice_sin_fichero_falla() {
    let sin_primeros = nombres(&["README.md", "estado.md", "instalacion.md", "ssh-config.md"]);
    let mensaje = fallo(summary(README_DE_EJEMPLO, &sin_primeros));
    assert!(mensaje.contains("primeros-pasos.md"), "{mensaje}");
    assert!(mensaje.contains("no está"), "{mensaje}");
}

#[test]
fn el_indice_con_un_capitulo_repetido_falla() {
    let repetido = README_DE_EJEMPLO.replace(
        "- [El `Include` de `~/.ssh/config`](ssh-config.md)",
        "- [Otra vez](instalacion.md)",
    );
    let mensaje = fallo(summary(&repetido, &ficheros_de_ejemplo()));
    assert!(mensaje.contains("repetido"), "{mensaje}");
    assert!(mensaje.contains("instalacion.md"), "{mensaje}");

    // Y `estado.md` en el índice sería repetirlo: va siempre al final.
    let con_estado = README_DE_EJEMPLO.replace("(ssh-config.md)", "(estado.md)");
    let mensaje = fallo(summary(&con_estado, &ficheros_de_ejemplo()));
    assert!(mensaje.contains("estado.md"), "{mensaje}");
}

#[test]
fn el_indice_mal_formado_falla_diciendo_donde() {
    // Sin la sección.
    let mensaje = fallo(summary("# Manual\n\nSin índice.\n", &ficheros_de_ejemplo()));
    assert!(mensaje.contains("Capítulos"), "{mensaje}");
    // La sección dos veces.
    let doble = format!("{README_DE_EJEMPLO}\n## Capítulos\n\n- [Uno](uno.md)\n");
    let mensaje = fallo(summary(&doble, &ficheros_de_ejemplo()));
    assert!(mensaje.contains("más de una vez"), "{mensaje}");
    // Sin ningún capítulo.
    let mensaje = fallo(summary("## Capítulos\n\nNada.\n", &nombres(&["estado.md"])));
    assert!(mensaje.contains("ningún capítulo"), "{mensaje}");
    // Una entrada que no apunta a un `.md` del libro: no se pierde en silencio.
    for rara in [
        "- [Web](https://ejemplo.org/x.md)",
        "- [Mayúsculas](Uno.md)",
        "- [Guion bajo](uno_dos.md)",
        "- [Sin cerrar](uno.md",
        "- [Subcarpeta](a/uno.md)",
    ] {
        let readme = format!("## Capítulos\n\n- [Uno](uno.md)\n{rara}\n");
        let mensaje = fallo(summary(&readme, &nombres(&["estado.md", "uno.md"])));
        assert!(mensaje.contains("README.md:4"), "{rara}: {mensaje}");
    }
    // Un título que mezcla dos enlaces es ambiguo.
    let ambiguo = "## Capítulos\n\n- [A](https://ejemplo.org) y [B](uno.md)\n";
    let mensaje = fallo(summary(ambiguo, &nombres(&["estado.md", "uno.md"])));
    assert!(mensaje.contains("README.md:3"), "{mensaje}");
}

#[test]
fn los_titulos_del_indice_pierden_las_comillas_invertidas() {
    let readme = "\
## Capítulos

- [El `Include` de `~/.ssh/config`](a.md)
- [`magi sesiones` y `magi sesion`](b.md) — descripción con [enlace](otro.md)
- [Con ``doble`` comilla](c.md)
";
    let ficheros = nombres(&["estado.md", "a.md", "b.md", "c.md"]);
    let resultado = summary(readme, &ficheros).unwrap();
    assert!(!resultado.contains('`'), "{resultado}");
    assert!(
        resultado.contains("- [El Include de ~/.ssh/config](a.md)\n"),
        "{resultado}"
    );
    assert!(
        resultado.contains("- [magi sesiones y magi sesion](b.md)\n"),
        "{resultado}"
    );
    assert!(
        resultado.contains("- [Con doble comilla](c.md)\n"),
        "{resultado}"
    );
    // La descripción no se cuela en el título.
    assert!(!resultado.contains("descripción"), "{resultado}");
}

#[test]
fn los_titulos_del_indice_escapan_lo_que_rompe_un_enlace() {
    let readme = "## Capítulos\n\n- [A [b] c\\d *f* _g_ &h](a.md)\n";
    let resultado = summary(readme, &nombres(&["estado.md", "a.md"])).unwrap();
    assert!(
        resultado.contains("- [A \\[b\\] c\\\\d \\*f\\* \\_g\\_ \\&h](a.md)\n"),
        "{resultado}"
    );
    // Lo que no hace falta escapar no se escapa: `~`, `_` en medio de una palabra,
    // `&` suelto y `*` rodeado de espacios son texto en Markdown.
    let llano = "## Capítulos\n\n- [Snippets & `~/.ssh` con snake_case y 2 * 3](a.md)\n";
    let del_llano = summary(llano, &nombres(&["estado.md", "a.md"])).unwrap();
    assert!(
        del_llano.contains("- [Snippets & ~/.ssh con snake_case y 2 * 3](a.md)\n"),
        "{del_llano}"
    );
    // Y mdBook lo lee con su texto: el título sale entero.
    let mut texto = String::new();
    let mut dentro = false;
    for evento in Parser::new_ext(&resultado, opciones()) {
        match evento {
            Event::Start(Tag::Link { dest_url, .. }) if &*dest_url == "a.md" => dentro = true,
            Event::End(_) => dentro = false,
            Event::Text(t) | Event::Code(t) if dentro => texto.push_str(&t),
            Event::SoftBreak if dentro => texto.push(' '),
            _ => {}
        }
    }
    assert_eq!(texto, "A [b] c\\d *f* _g_ &h");

    // Un título vacío o que solo era código vacío falla, y uno con controles también.
    for malo in ["- [``](a.md)", "- [ ` `](a.md)"] {
        let readme = format!("## Capítulos\n\n{malo}\n");
        let mensaje = fallo(summary(&readme, &nombres(&["estado.md", "a.md"])));
        assert!(mensaje.contains("vacío"), "{malo}: {mensaje}");
    }
    let readme = "## Capítulos\n\n- [A\u{7}B](a.md)\n";
    let mensaje = fallo(summary(readme, &nombres(&["estado.md", "a.md"])));
    assert!(mensaje.contains("control"), "{mensaje}");
}

// ---------------------------------------------------------------------------
// Anclas
// ---------------------------------------------------------------------------

/// Quita los ` {#id}` que añade `con_anclas`.
fn sin_ids(texto: &str) -> String {
    let mut salida = String::new();
    let mut resto = texto;
    while let Some(i) = resto.find(" {#") {
        salida.push_str(&resto[..i]);
        let tras = &resto[i + 3..];
        let fin = tras.find('}').expect("un {#id} sin cerrar");
        resto = &tras[fin + 1..];
    }
    salida.push_str(resto);
    salida
}

#[test]
fn cada_titulo_recibe_su_id_de_github() {
    let original = "\
# Título uno

## Instalación

Texto.

## Instalación

### Instalación

## ÁRBOL y ÑANDÚ

## Con `código` y [enlace](x.md)

## ¿Qué es esto? (¡sí!)

### Cierre ATX ###

## Con cierre y blancos ##\x20\x20\x20

###### Seis

## El `Include` de `~/.ssh/config`

## Año 2026: ¿cómo?
";
    let salida = con_anclas(original, "t.md").unwrap();
    let esperados = [
        "título-uno",
        "instalación",
        "instalación-1",
        "instalación-2",
        "árbol-y-ñandú",
        "con-código-y-enlace",
        "qué-es-esto-sí",
        "cierre-atx",
        "con-cierre-y-blancos",
        "seis",
        "el-include-de-sshconfig",
        "año-2026-cómo",
    ];
    let ids = ids_de(&salida);
    assert_eq!(
        ids,
        esperados
            .iter()
            .map(|s| Some((*s).to_string()))
            .collect::<Vec<_>>(),
        "{salida}"
    );
    // Son exactamente los del algoritmo de repositorio_publico.
    assert_eq!(
        ids,
        slugs_de(original).into_iter().map(Some).collect::<Vec<_>>()
    );
    // Lo demás del texto no cambia: solo se añaden los ` {#id}`.
    assert_eq!(sin_ids(&salida), original);
    // Con los `#` de cierre, el id va al final de la línea (así lo lee mdBook).
    assert!(
        salida.contains("### Cierre ATX ### {#cierre-atx}\n"),
        "{salida}"
    );
    assert!(
        salida.contains("## Con cierre y blancos ## {#con-cierre-y-blancos}   \n"),
        "{salida}"
    );
    // Y a una pasada ya hecha no se le puede volver a pasar.
    let mensaje = fallo(con_anclas(&salida, "t.md"));
    assert!(mensaje.contains("atributos"), "{mensaje}");
}

#[test]
fn los_titulos_con_finales_de_linea_de_windows_tambien() {
    let original = "# Uno\r\n\r\n## Dos y tres\r\n\r\ntexto\r\n\r\n## Dos y tres\r\n";
    let salida = con_anclas(original, "t.md").unwrap();
    assert_eq!(
        ids_de(&salida),
        [
            Some("uno".to_string()),
            Some("dos-y-tres".to_string()),
            Some("dos-y-tres-1".to_string())
        ]
    );
    assert_eq!(sin_ids(&salida), original);
    assert!(salida.contains("# Uno {#uno}\r\n"), "{salida:?}");
}

#[test]
fn un_texto_sin_titulos_se_queda_como_esta() {
    for texto in ["", "solo texto\n", "sin salto final", "```\n# nada\n```\n"] {
        assert_eq!(con_anclas(texto, "t.md").unwrap(), texto);
    }
    // Un título al final sin salto de línea.
    assert_eq!(con_anclas("# Fin", "t.md").unwrap(), "# Fin {#fin}");
}

#[test]
fn los_titulos_en_bloques_de_codigo_no_se_tocan() {
    let original = "\
# Real uno

```sh
# esto es un comentario
## y esto
```

~~~text
# tilde
~~~

    # con sangría

Texto con `# en línea` y más.

``` rust
# [derive]
```

## Real dos
";
    let salida = con_anclas(original, "t.md").unwrap();
    assert_eq!(
        ids_de(&salida),
        [Some("real-uno".to_string()), Some("real-dos".to_string())]
    );
    assert_eq!(sin_ids(&salida), original);
    // Cada línea de código sigue igual, una a una.
    for linea in [
        "# esto es un comentario",
        "## y esto",
        "# tilde",
        "    # con sangría",
        "# [derive]",
    ] {
        assert!(
            salida.lines().any(|l| l == linea),
            "la línea de código {linea:?} cambió:\n{salida}"
        );
    }
}

#[test]
fn si_el_markdown_y_las_anclas_no_se_alinean_falla_en_vez_de_desalinear() {
    // Un `#` dentro de un bloque de HTML: el algoritmo de anclas lo cuenta como
    // título y el analizador no.
    let con_html = "# Uno\n\n<pre>\n# no es título\n</pre>\n\n## Dos\n";
    let mensaje = fallo(con_anclas(con_html, "t.md"));
    assert!(
        mensaje.contains("alinean") || mensaje.contains("no coincide") || mensaje.contains("no ve"),
        "{mensaje}"
    );
    // Una valla de cuatro comillas con tres dentro desordena el algoritmo de anclas.
    let vallas = "````md\n```\n# dentro\n```\n````\n\n# Fuera\n";
    let mensaje = fallo(con_anclas(vallas, "t.md"));
    assert!(mensaje.contains("t.md:"), "{mensaje}");
    // Un título con sangría o dentro de una cita o una lista no lo ve GitHub igual.
    for texto in [
        "   ## Con sangría\n",
        "> ## En una cita\n",
        "- ## En una lista\n",
    ] {
        let mensaje = fallo(con_anclas(texto, "t.md"));
        assert!(mensaje.contains("t.md:1"), "{texto:?}: {mensaje}");
    }
    // Aunque el número de títulos y su texto coincidan por casualidad (uno en una
    // cita que GitHub no cuenta y otro igual en un bloque de HTML que solo cuenta el
    // algoritmo de anclas), el que no está en la primera columna se rechaza.
    let casual = "> ## A\n\n<pre>\n## A\n</pre>\n";
    let mensaje = fallo(con_anclas(casual, "t.md"));
    assert!(mensaje.contains("t.md:1"), "{mensaje}");
    assert!(
        mensaje.contains("sangría") || mensaje.contains("cita"),
        "{mensaje}"
    );
    // Un título sin espacio tras las almohadillas (con tabulador) tampoco.
    let mensaje = fallo(con_anclas("#\tUno\n", "t.md"));
    assert!(mensaje.contains("t.md:1"), "{mensaje}");
    // Un título vacío no tiene ancla.
    let mensaje = fallo(con_anclas("##\n", "t.md"));
    assert!(mensaje.contains("t.md:1"), "{mensaje}");
}

#[test]
fn un_titulo_subrayado_no_se_admite() {
    let mensaje = fallo(con_anclas(
        "Título\n======\n\ntexto\n\nOtro\n-----\n",
        "t.md",
    ));
    assert!(mensaje.contains("subrayado"), "{mensaje}");
    assert!(mensaje.contains("t.md:1"), "{mensaje}");
    let mensaje = fallo(con_anclas("# Bien\n\nOtro\n---\n", "t.md"));
    assert!(mensaje.contains("t.md:3"), "{mensaje}");
}

#[test]
fn un_titulo_con_id_propio_falla() {
    for titulo in [
        "## Con id {#mio}",
        "## Con clase {.clase}",
        "## Con atributo {clave=valor}",
        "## Todo {#a .b c=d}",
    ] {
        let texto = format!("# Uno\n\n{titulo}\n");
        let mensaje = fallo(con_anclas(&texto, "capitulo.md"));
        assert!(mensaje.contains("capitulo.md:3"), "{titulo}: {mensaje}");
        assert!(mensaje.contains("atributos"), "{titulo}: {mensaje}");
    }
    // Una llave que no es un atributo válido es solo texto y no estorba.
    let salida = con_anclas("## Con {llaves} sueltas {#}\n", "t.md").unwrap();
    assert_eq!(ids_de(&salida), [Some("con-llaves-sueltas-".to_string())]);
}

#[test]
fn dos_titulos_sin_ancla_posible_o_con_el_mismo_id_fallan() {
    let mensaje = fallo(con_anclas("# Uno\n\n## ???\n", "t.md"));
    assert!(mensaje.contains("t.md:3"), "{mensaje}");
    assert!(mensaje.contains("sin letras ni cifras"), "{mensaje}");
    // «A», «A» y «A-1»: el tercero se llamaría igual que el segundo.
    let mensaje = fallo(con_anclas("# A\n\n# A\n\n# A-1\n", "t.md"));
    assert!(mensaje.contains("mismo id"), "{mensaje}");
}

#[test]
fn los_ids_coinciden_con_el_algoritmo_de_repositorio_publico() {
    // Los capítulos de ejemplo y el manual real del repositorio, uno a uno.
    let mut carpetas = vec![libro_de_ejemplo(), raiz().join("docs/manual")];
    let mut cuenta = 0;
    while let Some(carpeta) = carpetas.pop() {
        for entrada in fs::read_dir(&carpeta).unwrap() {
            let ruta = entrada.unwrap().path();
            if ruta.extension().is_none_or(|e| e != "md") {
                continue;
            }
            let original = fs::read_to_string(&ruta).unwrap();
            let nombre = ruta.file_name().unwrap().to_string_lossy().into_owned();
            let salida = con_anclas(&original, &nombre).unwrap_or_else(|e| panic!("{nombre}: {e}"));
            assert_eq!(
                ids_de(&salida),
                slugs_de(&original)
                    .into_iter()
                    .map(Some)
                    .collect::<Vec<_>>(),
                "{nombre}"
            );
            assert_eq!(sin_ids(&salida), original, "{nombre}");
            cuenta += 1;
        }
    }
    assert!(cuenta >= 20, "solo se comprobaron {cuenta} ficheros");
}

// ---------------------------------------------------------------------------
// Enlaces
// ---------------------------------------------------------------------------

#[test]
fn los_enlaces_de_fuera_van_a_github_en_la_etiqueta() {
    let casos = [
        // Un fichero: blob.
        (
            "[a](../formato-sincronizacion.md)",
            "[a](https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/formato-sincronizacion.md)",
        ),
        (
            "[a](../publicar-android.md)",
            "[a](https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/publicar-android.md)",
        ),
        (
            "[a](../../SECURITY.md)",
            "[a](https://github.com/HectorEspejo/magi/blob/v1.6.1/SECURITY.md)",
        ),
        (
            "[a](../../CONTRIBUTING.md)",
            "[a](https://github.com/HectorEspejo/magi/blob/v1.6.1/CONTRIBUTING.md)",
        ),
        // Una carpeta (con barra final): tree.
        (
            "[a](../../packaging/termux/)",
            "[a](https://github.com/HectorEspejo/magi/tree/v1.6.1/packaging/termux/)",
        ),
        (
            "[a](../../android/)",
            "[a](https://github.com/HectorEspejo/magi/tree/v1.6.1/android/)",
        ),
        // Sin barra final no se sabe si es carpeta: blob, que GitHub redirige.
        (
            "[a](../../android)",
            "[a](https://github.com/HectorEspejo/magi/blob/v1.6.1/android)",
        ),
        // La raíz del repositorio.
        (
            "[a](../../)",
            "[a](https://github.com/HectorEspejo/magi/tree/v1.6.1)",
        ),
        (
            "[a](../..)",
            "[a](https://github.com/HectorEspejo/magi/tree/v1.6.1)",
        ),
        // Con fragmento y con consulta.
        (
            "[a](../formato-sincronizacion.md#formato)",
            "[a](https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/formato-sincronizacion.md#formato)",
        ),
        (
            "[a](../../src/lib.rs#L10-L20)",
            "[a](https://github.com/HectorEspejo/magi/blob/v1.6.1/src/lib.rs#L10-L20)",
        ),
        (
            "[a](../../SECURITY.md?plain=1)",
            "[a](https://github.com/HectorEspejo/magi/blob/v1.6.1/SECURITY.md?plain=1)",
        ),
        (
            "[a](../../SECURITY.md?plain=1#L3)",
            "[a](https://github.com/HectorEspejo/magi/blob/v1.6.1/SECURITY.md?plain=1#L3)",
        ),
        (
            "[a](../../SECURITY.md?a=1&b=2)",
            "[a](https://github.com/HectorEspejo/magi/blob/v1.6.1/SECURITY.md?a=1&amp;b=2)",
        ),
        // Una carpeta con fragmento.
        (
            "[a](../../android/#sección)",
            "[a](https://github.com/HectorEspejo/magi/tree/v1.6.1/android/#secci%C3%B3n)",
        ),
        // `./`, `..` intermedios y letras que no son ASCII.
        (
            "[a](./../formato-sincronizacion.md)",
            "[a](https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/formato-sincronizacion.md)",
        ),
        (
            "[a](../../docs/./x/../formato-sincronizacion.md)",
            "[a](https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/formato-sincronizacion.md)",
        ),
        (
            "[a](../túnel.md)",
            "[a](https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/t%C3%BAnel.md)",
        ),
        (
            "[a](../t%C3%BAnel.md)",
            "[a](https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/t%C3%BAnel.md)",
        ),
        // Otros ficheros de docs/manual/ que no son capítulos ni imágenes: GitHub.
        (
            "[a](datos.csv)",
            "[a](https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/manual/datos.csv)",
        ),
        (
            "[a](carpeta/)",
            "[a](https://github.com/HectorEspejo/magi/tree/v1.6.1/docs/manual/carpeta/)",
        ),
        (
            "[a](./)",
            "[a](https://github.com/HectorEspejo/magi/tree/v1.6.1/docs/manual/)",
        ),
    ];
    for (entrada, esperado) in casos {
        assert_eq!(enlazado(entrada), esperado, "{entrada}");
    }

    // La etiqueta sale en todos y no depende de nada más.
    let otra = reescribir_enlaces(
        "[a](../../SECURITY.md)",
        "tuneles.md",
        "v2.0.10",
        &nombres(&["tuneles.md"]),
        "t.md",
    )
    .unwrap();
    assert_eq!(
        otra,
        "[a](https://github.com/HectorEspejo/magi/blob/v2.0.10/SECURITY.md)"
    );
}

#[test]
fn los_readme_van_a_la_portada() {
    assert_eq!(enlazado("[← README](../../README.md)"), "[← README](../)");
    assert_eq!(
        enlazado("[English](../../README.en.md)"),
        "[English](../en/)"
    );
    assert_eq!(
        enlazado("[a](../../README.md#instalación) [b](../../README.en.md#install)"),
        "[a](../#instalaci%C3%B3n) [b](../en/#install)"
    );
    assert_eq!(enlazado("[a](../../README.md?x=1)"), "[a](../?x=1)");
    assert_eq!(enlazado("[a](./../../README.md)"), "[a](../)");
    // Una línea del manual real.
    assert_eq!(
        enlazado("[← README](../../README.md) · [English README](../../README.en.md)"),
        "[← README](../) · [English README](../en/)"
    );
    // Los README de otras carpetas no son la portada.
    assert_eq!(
        enlazado("[a](../../android/README.md)"),
        "[a](https://github.com/HectorEspejo/magi/blob/v1.6.1/android/README.md)"
    );
}

#[test]
fn los_enlaces_entre_capitulos_se_dejan() {
    for tal_cual in [
        "[a](tuneles.md)",
        "[a](primeros-pasos.md)",
        "[a](tuneles.md#túnel-local-y-remoto)",
        "[a](tuneles.md#sincronización-del-inventario)",
        "[a](primeros-pasos.md#instalación-1)",
        "[a](estado.md)",
        "[a](estado.md#fila)",
        "[a](tuneles.md?x=1#y)",
        "[a](<tuneles.md>)",
        "[a](tuneles.md \"con título\")",
        "[a](#solo-un-fragmento)",
        "[a](#túnel)",
        "[a]()",
        "[a](https://ejemplo.org/x?y=1&z=2#w)",
        "[a](http://127.0.0.1:8080/)",
        "[a](mailto:usuario@ejemplo.org)",
        "[a](tel:+34000000000)",
        "<https://ejemplo.org/>",
        "<usuario@ejemplo.org>",
        "texto (../../README.md) sin enlace",
        "[no es enlace] (../../README.md)",
    ] {
        assert_eq!(enlazado(tal_cual), tal_cual);
    }
    // Normalizados: la misma página escrita de otra forma, o el índice del manual.
    assert_eq!(enlazado("[a](./tuneles.md#x)"), "[a](tuneles.md#x)");
    assert_eq!(enlazado("[a](../manual/tuneles.md)"), "[a](tuneles.md)");
    assert_eq!(enlazado("[a](tuneles%2Emd)"), "[a](tuneles.md)");
    // mdBook publica `README.md` como `index.html` y dejaría el enlace en
    // `README.html`, que no existe: el índice del manual se enlaza por `index.md`.
    assert_eq!(
        enlazado("[← Índice del manual](README.md)"),
        "[← Índice del manual](index.md)"
    );
    assert_eq!(
        enlazado("[a](./README.md#capítulos)"),
        "[a](index.md#cap%C3%ADtulos)"
    );
    assert_eq!(enlazado("[a](<README.md>)"), "[a](index.md)");
}

#[test]
fn un_md_de_docs_manual_que_no_es_capitulo_es_un_error() {
    for malo in [
        "[a](otro.md)",
        "[a](Tuneles.md)",
        "[a](index.md)",
        "[a](sub/otro.md)",
        "[a](../manual/otro.md#x)",
        "[a](SUMMARY.md)",
    ] {
        let mensaje = fallo(enlazar(malo));
        assert!(mensaje.contains("t.md:1"), "{malo}: {mensaje}");
        assert!(mensaje.contains("capítulo"), "{malo}: {mensaje}");
    }
}

#[test]
fn un_enlace_que_se_sale_del_repositorio_falla() {
    for (malo, trozo) in [
        ("[a](../../../etc/passwd)", "raíz del repositorio"),
        ("[a](../../..)", "raíz del repositorio"),
        ("[a](../../../)", "raíz del repositorio"),
        ("[a](../../../../x.md)", "raíz del repositorio"),
        ("[a](../a/../../../x.md)", "raíz del repositorio"),
        ("[a](/etc/passwd)", "absoluta"),
        ("[a](//ejemplo.org/x)", "absoluta"),
        ("[a](<../x\\y.md>)", "control"),
        ("[a](<../x\u{7}y.md>)", "control"),
        ("[a](<../x\ty.md>)", "control"),
        ("[a](../a%2Fb.md)", "mal formado"),
        ("[a](../a%5Cb.md)", "mal formado"),
        ("[a](../a%00b.md)", "mal formado"),
        ("[a](../a%zz.md)", "mal formado"),
        ("[a](../a//b.md)", "no es una ruta válida"),
        ("[a](?solo=consulta)", "sin ruta"),
        ("[a](javascript:alert(1))", "javascript"),
        ("[a](JavaScript:alert(1))", "javascript"),
        ("[a](data:text/html,x)", "data"),
        ("[a](vbscript:x)", "vbscript"),
        ("[a](1a:b)", "esquema"),
        ("[a](:x)", "esquema"),
    ] {
        let mensaje = fallo(enlazar(&format!("uno\n\n{malo}\n")));
        assert!(mensaje.contains("t.md:3"), "{malo}: {mensaje}");
        assert!(mensaje.to_lowercase().contains(trozo), "{malo}: {mensaje}");
    }
    // La línea es la del enlace aunque haya muchas antes y el enlace sea de varias líneas.
    let mensaje = fallo(enlazar("a\nb\nc\n\n[d](\n  ../../../x\n)\n"));
    assert!(mensaje.contains("t.md:6"), "{mensaje}");
}

#[test]
fn las_etiquetas_raras_no_llegan_a_las_urls() {
    for mala in [
        "",
        "main",
        "1.6.1",
        "v1.6.1-rc.1",
        "v1.6.1/../x",
        "v1.6",
        "v1.6.1 ",
        "android-v0.2.0",
    ] {
        let r = reescribir_enlaces("[a](../../SECURITY.md)", "x.md", mala, &[], "t.md");
        assert!(r.is_err(), "{mala:?}");
    }
}

#[test]
fn los_enlaces_en_codigo_no_se_tocan() {
    let original = "\
Código en línea: `[a](../../README.md)` y ``[b](../x.md)``.

```md
[c](../../SECURITY.md)
[d]: ../../README.md
![e](../../x.png)
```

~~~
[f](../formato-sincronizacion.md)
~~~

    [g](../../android/)

    [h]: ../../README.en.md

<!-- [i](../../README.md) -->

Fuera de código sí: [j](../../README.md).
";
    let salida = enlazado(original);
    let cambios: Vec<(&str, &str)> = original
        .lines()
        .zip(salida.lines())
        .filter(|(a, b)| a != b)
        .collect();
    assert_eq!(
        cambios,
        [(
            "Fuera de código sí: [j](../../README.md).",
            "Fuera de código sí: [j](../)."
        )]
    );
    assert_eq!(original.lines().count(), salida.lines().count());
}

#[test]
fn las_definiciones_de_enlace_tambien_se_reescriben() {
    let original = "\
Usa [el formato][f], [seguridad][] y [portada] o ![logo][img].

[f]: ../formato-sincronizacion.md \"El formato\"
[seguridad]: <../../SECURITY.md#reportar>
[PORTADA]:
   ../../README.md
[img]: logo.png
[capitulo]: tuneles.md#x
[sin usar]: ../../android/
";
    let salida = enlazado(original);
    assert_eq!(
        salida,
        "\
Usa [el formato][f], [seguridad][] y [portada] o ![logo][img].

[f]: https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/formato-sincronizacion.md \"El formato\"
[seguridad]: https://github.com/HectorEspejo/magi/blob/v1.6.1/SECURITY.md#reportar
[PORTADA]:
   ../
[img]: logo.png
[capitulo]: tuneles.md#x
[sin usar]: https://github.com/HectorEspejo/magi/tree/v1.6.1/android/
"
    );
    // El analizador las lee con sus destinos nuevos.
    let destinos = destinos_de(&salida);
    assert!(destinos.contains(&"../".to_string()), "{destinos:?}");
    assert!(
        destinos.iter().all(|d| !d.starts_with("../../")),
        "{destinos:?}"
    );

    // Una definición que sale de la raíz falla con su línea; una imagen por
    // referencia que apunta fuera de docs/manual/, también.
    let mensaje = fallo(enlazar("a\n\n[x]: ../../../fuera.md\n"));
    assert!(mensaje.contains("t.md:3"), "{mensaje}");
    let mensaje = fallo(enlazar("![logo][img]\n\n[img]: ../../assets/logo.png\n"));
    assert!(mensaje.contains("t.md:3"), "{mensaje}");
    assert!(mensaje.contains("imagen"), "{mensaje}");
    // La misma definición usada como enlace sí es un fichero del repositorio.
    assert!(enlazar("[logo][img]\n\n[img]: ../../assets/logo.png\n").is_ok());
    // Un `.md` que no es capítulo, en una definición, también falla.
    let mensaje = fallo(enlazar("[x]\n\n[x]: otro.md\n"));
    assert!(mensaje.contains("t.md:3"), "{mensaje}");
}

#[test]
fn destinos_con_angulos_comillas_y_parentesis() {
    let casos = [
        // Ángulos con espacios y título.
        (
            "[a](<../../docs/dos palabras.md> \"título\")",
            "[a](https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/dos%20palabras.md \"título\")",
        ),
        // Paréntesis en el destino, equilibrados o escapados.
        (
            "[a](../../docs/a(b).md)",
            "[a](https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/a%28b%29.md)",
        ),
        (
            "[a](../../docs/a\\(b.md)",
            "[a](https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/a%28b.md)",
        ),
        (
            "[a](<../../docs/a)b.md>)",
            "[a](https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/a%29b.md)",
        ),
        // El título, con cualquiera de sus tres formas, se conserva tal cual.
        (
            "[a](../../SECURITY.md \"un título (con paréntesis)\")",
            "[a](https://github.com/HectorEspejo/magi/blob/v1.6.1/SECURITY.md \"un título (con paréntesis)\")",
        ),
        (
            "[a](../../SECURITY.md 'otro \"así\"')",
            "[a](https://github.com/HectorEspejo/magi/blob/v1.6.1/SECURITY.md 'otro \"así\"')",
        ),
        (
            "[a](../../SECURITY.md (de paréntesis))",
            "[a](https://github.com/HectorEspejo/magi/blob/v1.6.1/SECURITY.md (de paréntesis))",
        ),
        // Blancos alrededor y destino en otra línea.
        (
            "[a](  ../../SECURITY.md  )",
            "[a](  https://github.com/HectorEspejo/magi/blob/v1.6.1/SECURITY.md  )",
        ),
        (
            "[a](\n../../SECURITY.md\n\"t\"\n)",
            "[a](\nhttps://github.com/HectorEspejo/magi/blob/v1.6.1/SECURITY.md\n\"t\"\n)",
        ),
        // Texto del enlace con corchetes, código y una imagen dentro.
        (
            "[a [b] c](../../SECURITY.md)",
            "[a [b] c](https://github.com/HectorEspejo/magi/blob/v1.6.1/SECURITY.md)",
        ),
        (
            "[a `]` c](../../SECURITY.md)",
            "[a `]` c](https://github.com/HectorEspejo/magi/blob/v1.6.1/SECURITY.md)",
        ),
        (
            "[a\\] b](../../SECURITY.md)",
            "[a\\] b](https://github.com/HectorEspejo/magi/blob/v1.6.1/SECURITY.md)",
        ),
        (
            "[![logo](logo.png)](../../SECURITY.md)",
            "[![logo](logo.png)](https://github.com/HectorEspejo/magi/blob/v1.6.1/SECURITY.md)",
        ),
        // Entidades en el destino: `&amp;` es `&`, y sale como `&amp;`.
        (
            "[a](../../docs/a&amp;b.md)",
            "[a](https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/a%26b.md)",
        ),
        (
            "[a](../../docs/a.md?x=1&amp;y=2#z)",
            "[a](https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/a.md?x=1&amp;y=2#z)",
        ),
        // Varios enlaces seguidos y en la misma línea.
        (
            "[a](../../SECURITY.md)[b](../../android/) y [c](tuneles.md) y [d](../../README.md)",
            "[a](https://github.com/HectorEspejo/magi/blob/v1.6.1/SECURITY.md)[b](https://github.com/HectorEspejo/magi/tree/v1.6.1/android/) y [c](tuneles.md) y [d](../)",
        ),
        // En una tabla, una lista, una cita y una nota al pie.
        (
            "| a | b |\n|---|---|\n| [c](../../SECURITY.md) | x |\n",
            "| a | b |\n|---|---|\n| [c](https://github.com/HectorEspejo/magi/blob/v1.6.1/SECURITY.md) | x |\n",
        ),
        (
            "- uno\n  - [a](../../SECURITY.md)\n> [b](../../README.md)\n",
            "- uno\n  - [a](https://github.com/HectorEspejo/magi/blob/v1.6.1/SECURITY.md)\n> [b](../)\n",
        ),
        (
            "Nota[^1].\n\n[^1]: Mira [esto](../../SECURITY.md).\n",
            "Nota[^1].\n\n[^1]: Mira [esto](https://github.com/HectorEspejo/magi/blob/v1.6.1/SECURITY.md).\n",
        ),
        // En un título.
        (
            "## Ver [la guía](../publicar.md)\n",
            "## Ver [la guía](https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/publicar.md)\n",
        ),
    ];
    for (entrada, esperado) in casos {
        assert_eq!(enlazado(entrada), esperado, "{entrada:?}");
    }
    // Y el resultado vuelve a analizarse con los destinos que se esperan.
    let salida = enlazado("[a](<../../docs/dos palabras.md> \"t\")");
    assert_eq!(
        destinos_de(&salida),
        ["https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/dos%20palabras.md"]
    );
}

#[test]
fn las_imagenes_locales_se_dejan_y_las_demas_fallan() {
    assert_eq!(enlazado("![a](captura.png)"), "![a](captura.png)");
    assert_eq!(
        enlazado("![a](./captura.png \"t\")"),
        "![a](captura.png \"t\")"
    );
    assert_eq!(enlazado("![a](../manual/captura.png)"), "![a](captura.png)");
    assert_eq!(enlazado("![a](img/captura.png)"), "![a](img/captura.png)");
    assert_eq!(
        enlazado("[![a](captura.png)](../../SECURITY.md)")
            .matches("captura.png")
            .count(),
        1
    );
    for (mala, trozo) in [
        ("![a](../../assets/logo.png)", "fuera de docs/manual"),
        ("![a](../captura.png)", "fuera de docs/manual"),
        ("![a](../../README.md)", "README"),
        ("![a](https://ejemplo.org/x.png)", "fuera"),
        ("![a](data:image/png;base64,AAAA)", "data"),
        ("![a](#ancla)", "fragmento"),
        ("![a]()", "sin ruta"),
        ("![a](tuneles.md)", ".md"),
        ("![a](documento.pdf)", "no es una imagen"),
        ("![a](carpeta/)", "no es una imagen"),
    ] {
        let mensaje = fallo(enlazar(&format!("\n{mala}\n")));
        assert!(mensaje.contains("t.md:2"), "{mala}: {mensaje}");
        assert!(mensaje.contains(trozo), "{mala}: {mensaje}");
    }
}

#[test]
fn muchas_imagenes_locales_no_cuestan_tiempo_cuadratico() {
    // Cada imagen local anota su línea: contarla desde el principio del texto en cada
    // una dejaba el trabajo en minutos con un capítulo de 2 MiB de imágenes.
    let texto = "![a](captura.png)\n".repeat(60_000);
    let inicio = std::time::Instant::now();
    let salida = enlazado(&texto);
    assert_eq!(salida, texto);
    assert!(
        inicio.elapsed() < std::time::Duration::from_secs(10),
        "tardó {:?}",
        inicio.elapsed()
    );
}

#[test]
fn una_imagen_que_falta_dice_su_linea_aunque_haya_muchas() {
    // La línea sale de una sola pasada por el texto: que siga siendo la correcta.
    let temporal = tempfile::tempdir().unwrap();
    let carpeta = copia_del_libro(temporal.path());
    let capitulo = carpeta.join("tuneles.md");
    let mut texto = fs::read_to_string(&capitulo).unwrap();
    let antes = texto.lines().count();
    texto.push_str("\nUna imagen que no existe:\n\n![a](falta.png)\n");
    fs::write(&capitulo, &texto).unwrap();
    let mensaje = fallo(preparar(&carpeta, &temporal.path().join("libro")));
    assert!(
        mensaje.contains(&format!("tuneles.md:{}", antes + 4)),
        "{mensaje}"
    );
    assert!(mensaje.contains("falta.png"), "{mensaje}");
}

#[test]
fn los_autoenlaces_no_llevan_esquemas_que_ejecuten() {
    // `<javascript:…>` es un autoenlace válido para Markdown: tiene que fallar igual
    // que `[a](javascript:…)`, con su línea.
    for malo in [
        "<javascript:alert(1)>",
        "<JavaScript:alert(1)>",
        "<data:text/html,x>",
        "<vbscript:x>",
    ] {
        let mensaje = fallo(enlazar(&format!("uno\n\ntexto {malo} más\n")));
        assert!(mensaje.contains("t.md:3"), "{malo}: {mensaje}");
        assert!(mensaje.contains("autoenlace"), "{malo}: {mensaje}");
    }
    // Los demás autoenlaces se dejan tal cual, y los de código ni se miran.
    let original = "\
Web <https://ejemplo.org/a?b=1&c=2>, correo <mailto:a@ejemplo.org> y <a@ejemplo.org>.

En código: `<javascript:alert(1)>`.

```
<javascript:alert(1)>
```
";
    assert_eq!(enlazado(original), original);
}

#[test]
fn una_imagen_con_un_nombre_raro_no_parte_el_destino() {
    // El nombre se descodifica para decidir, pero lo que se escribe es siempre un
    // destino entero: nada de espacios ni paréntesis sueltos (la imagen no existirá y
    // `preparar_manual` falla, pero `reescribir_enlaces` no deja Markdown roto).
    assert_eq!(enlazado("![a](mi%20captura.png)"), "![a](mi%20captura.png)");
    assert_eq!(
        enlazado("[x]:![a](foo.png)%20>>foo.png"),
        "[x]:%21%5Ba%5D%28foo.png%29%20%3E%3Efoo.png"
    );
    assert_eq!(
        enlazado("[ver](./mi%28captura.png)"),
        "[ver](mi%28captura.png)"
    );
}

// ---------------------------------------------------------------------------
// HTML en bruto y preprocesadores
// ---------------------------------------------------------------------------

#[test]
fn el_html_suelto_se_escapa_y_el_codigo_no() {
    let original = "\
Conecta con <host> o con <uuid>, <equipo> y <comando> sueltos.

Una <kbd>tecla</kbd> y un salto<br>de línea.

<div class=\"x\">
bloque con <b>negrita</b>

otro párrafo
</div>

| tabla | <host> |
|---|---|
| a | b |

Código: `<host>` y ``<a> <b>``.

```sh
magi conectar <host> --nombre <nombre>
```

~~~
<pre>
~~~

    <host> con sangría

Autoenlaces que no se tocan: <https://ejemplo.org/x> y <usuario@ejemplo.org>.

Ya escapado: \\<host> y una comparación 1 < 2 > 0.
";
    let salida = escapar_html_suelto(original, "t.md").unwrap();
    assert!(!hay_html(&salida), "{salida}");
    assert!(
        salida.contains("Conecta con \\<host> o con \\<uuid>, \\<equipo> y \\<comando> sueltos."),
        "{salida}"
    );
    assert!(
        salida.contains("Una \\<kbd>tecla\\</kbd> y un salto\\<br>de línea."),
        "{salida}"
    );
    // Un bloque de HTML de varias líneas se escapa entero.
    assert!(
        salida.contains(
            "\\<div class=\"x\">\nbloque con \\<b>negrita\\</b>\n\notro párrafo\n\\</div>\n"
        ),
        "{salida}"
    );
    assert!(salida.contains("| tabla | \\<host> |"), "{salida}");
    // El código, igual.
    for intacto in [
        "Código: `<host>` y ``<a> <b>``.",
        "magi conectar <host> --nombre <nombre>",
        "<pre>",
        "    <host> con sangría",
        "Autoenlaces que no se tocan: <https://ejemplo.org/x> y <usuario@ejemplo.org>.",
        "Ya escapado: \\<host> y una comparación 1 < 2 > 0.",
    ] {
        assert!(
            salida.lines().any(|l| l == intacto),
            "{intacto:?} cambió:\n{salida}"
        );
    }
    // Es idempotente: un segundo paso no cambia nada.
    assert_eq!(escapar_html_suelto(&salida, "t.md").unwrap(), salida);
    // Y no hay nada que hacer en un texto sin HTML.
    assert_eq!(
        escapar_html_suelto("solo texto\n", "t.md").unwrap(),
        "solo texto\n"
    );
}

#[test]
fn dentro_de_un_bloque_de_html_no_se_duplica_una_barra_que_ya_escapa() {
    // En un bloque, `\<b>` era texto de HTML; al pasar a Markdown, la barra ya
    // escapa el `<` y una segunda lo dejaría vivo. Con dos barras (una barra
    // escapada) el `<` sí está vivo y se escapa.
    let original = "<div>\nuna \\<b> dentro y \\\\<i> doble\n</div>\n";
    let salida = escapar_html_suelto(original, "t.md").unwrap();
    assert_eq!(
        salida,
        "\\<div>\nuna \\<b> dentro y \\\\\\<i> doble\n\\</div>\n"
    );
    assert!(!hay_html(&salida));
}

#[test]
fn el_html_escapado_se_ve_como_texto_y_no_se_traga_la_linea() {
    // Antes de escapar, `<host>` abría una etiqueta y el analizador la pasaba tal
    // cual; después es texto con su `<` y su `>`.
    let original = "Conecta a <host> y sigue el texto de la línea.\n";
    assert!(hay_html(original));
    let salida = escapar_html_suelto(original, "t.md").unwrap();
    let texto: String = Parser::new_ext(&salida, opciones())
        .filter_map(|e| match e {
            Event::Text(t) => Some(t.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(texto, "Conecta a <host> y sigue el texto de la línea.");
}

#[test]
fn los_comentarios_html_se_quitan() {
    let original = "\
Antes <!-- en línea --> después.

<!-- uno solo -->

Texto.

<!-- varias
líneas con <host> dentro
-->

Fin <!-- otro --> y <host>.

```html
<!-- esto es código y se queda -->
```

`<!-- también -->`
";
    let salida = escapar_html_suelto(original, "t.md").unwrap();
    assert!(!salida.contains("en línea"), "{salida}");
    assert!(!salida.contains("uno solo"), "{salida}");
    assert!(!salida.contains("varias"), "{salida}");
    assert!(!salida.contains("otro -->"), "{salida}");
    assert!(salida.contains("Antes  después."), "{salida}");
    assert!(salida.contains("Fin  y \\<host>."), "{salida}");
    // Lo de dentro de código se queda.
    assert!(
        salida.contains("<!-- esto es código y se queda -->"),
        "{salida}"
    );
    assert!(salida.contains("`<!-- también -->`"), "{salida}");
    assert!(!hay_html(&salida));
    // Los párrafos siguen siendo los mismos: un comentario solo en su línea deja
    // una línea en blanco, no une lo de antes con lo de después.
    let parrafos = |texto: &str| {
        Parser::new_ext(texto, opciones())
            .filter(|e| matches!(e, Event::Start(Tag::Paragraph)))
            .count()
    };
    assert_eq!(parrafos(&salida), 4);
    let pegado = escapar_html_suelto("uno\n<!-- c -->\ndos\n", "t.md").unwrap();
    assert_eq!(parrafos(&pegado), 2, "{pegado:?}");

    // Los casos raros del estándar.
    for comentario in ["<!-->", "<!--->", "<!---->", "<!-- a -- b -->"] {
        let texto = format!("a {comentario} b\n");
        assert_eq!(
            escapar_html_suelto(&texto, "t.md").unwrap(),
            "a  b\n",
            "{comentario}"
        );
    }
    // Un comentario de bloque sin cerrar se comería el resto del documento.
    let mensaje = fallo(escapar_html_suelto(
        "uno\n\n<!-- sin cerrar\n\ndos\n",
        "t.md",
    ));
    assert!(mensaje.contains("t.md:3"), "{mensaje}");
    assert!(mensaje.contains("sin cerrar"), "{mensaje}");
}

#[test]
fn el_html_dentro_de_citas_listas_y_scripts_se_escapa_igual() {
    // Dentro de una cita, de una lista, de una tabla, de un bloque `<script>` con
    // líneas en blanco y de una definición: todo pasa a texto y nada se pierde.
    for (entrada, esperado) in [
        (
            "> <div>\n> texto <b>x</b>\n> </div>\n>\n> más\n",
            "> \\<div>\n> texto \\<b>x\\</b>\n> \\</div>\n>\n> más\n",
        ),
        (
            "- uno\n\n  <div>x</div>\n\n1. <b>negrita</b> dentro\n",
            "- uno\n\n  \\<div>x\\</div>\n\n1. \\<b>negrita\\</b> dentro\n",
        ),
        (
            "<script>\nalert(1)\n\nx < y\n</script>\n\nfin\n",
            "\\<script>\nalert(1)\n\nx \\< y\n\\</script>\n\nfin\n",
        ),
        (
            "Término\n: definición con <host>\n",
            "Término\n: definición con \\<host>\n",
        ),
        ("| a |\n|---|\n| <br> |\n", "| a |\n|---|\n| \\<br> |\n"),
        // Los comentarios dentro de contenedores dejan el contenedor en pie.
        ("- <!-- c -->\n- b\n", "- \n- b\n"),
        ("> <!-- a\n> b -->\n> texto\n", "> \n> texto\n"),
    ] {
        let salida = escapar_html_suelto(entrada, "t.md").unwrap();
        assert_eq!(salida, esperado, "{entrada:?}");
        assert!(!hay_html(&salida), "{entrada:?}");
    }
}

#[test]
fn escapar_el_html_no_puede_crear_titulos_nuevos() {
    // Dentro de un bloque `<div>` una línea `# x` es texto de HTML; al escapar el
    // bloque se volvería un título sin su id.
    let texto = "<div>\n# no es título\n</div>\n";
    let mensaje = fallo(escapar_html_suelto(texto, "t.md"));
    assert!(mensaje.contains("títulos"), "{mensaje}");
}

#[test]
fn las_llaves_de_preprocesador_se_rechazan_incluso_en_codigo() {
    for malo in [
        "{{#include otro.md}}",
        "{{#playground ejemplo.rs}}",
        "{{#title Otro}}",
        "{{ #include otro.md }}",
        "{{\t#include x}}",
        "{{\n#include x}}",
        "{{{#include x}}}",
        "\\{{#include x}}",
        "`{{#include x}}`",
        "```\n{{#include x}}\n```",
        "    {{#include x}}",
        "<!-- {{#include x}} -->",
        "## Título {{#include x}}",
    ] {
        let texto = format!("uno\ndos\n{malo}\n");
        let mensaje = fallo(rechazar_preprocesadores(&texto, "capitulo.md"));
        assert!(
            mensaje.contains("capitulo.md:") && mensaje.contains("{{#"),
            "{malo:?}: {mensaje}"
        );
    }
    // La línea es la de la primera aparición.
    let mensaje = fallo(rechazar_preprocesadores(
        "a\nb\nc {{#x}}\nd {{#y}}\n",
        "capitulo.md",
    ));
    assert!(mensaje.contains("capitulo.md:3"), "{mensaje}");
    // Lo que no lo es: variables de snippets, llaves sueltas, una sola llave.
    for bien in [
        "{{servicio}}",
        "{{servicio:nginx}}",
        "{{.Names}}",
        "`{{host}}` y {{ nombre }}",
        "{#id}",
        "{ #no }",
        "{{",
        "{{ ",
        "{{}}",
        "# {x}",
        "",
    ] {
        rechazar_preprocesadores(bien, "t.md").unwrap_or_else(|e| panic!("{bien:?}: {e}"));
    }
}

#[test]
fn una_tabla_alineada_se_rechaza_con_su_linea() {
    // mdBook escribe `style="text-align: …"` en estas celdas y la CSP lo bloquea.
    for fila in ["|:--|--|", "| --- | ---: |", "|:-:|---|"] {
        let texto = format!("uno\n\n| a | b |\n{fila}\n| 1 | 2 |\n");
        let mensaje = fallo(rechazar_tablas_alineadas(&texto, "capitulo.md"));
        assert!(
            mensaje.contains("capitulo.md:3") && mensaje.contains("alineación"),
            "{fila:?}: {mensaje}"
        );
    }
    // Sin alineación, y el texto parecido dentro de código, valen.
    for bien in [
        "| a | b |\n|---|---|\n| 1 | 2 |\n",
        "| a | b |\n| --- | --- |\n| 1 | 2 |\n",
        "```\n|:--|--:|\n```\n",
        "texto con `|:--|` y :-: sueltos\n",
        "",
    ] {
        rechazar_tablas_alineadas(bien, "t.md").unwrap_or_else(|e| panic!("{bien:?}: {e}"));
    }
}

#[test]
fn preparar_manual_rechaza_una_tabla_alineada_y_no_toca_la_salida_anterior() {
    let temporal = tempfile::tempdir().unwrap();
    let salida = temporal.path().join("libro");
    preparar(&libro_de_ejemplo(), &salida).unwrap();
    let antes = fs::read(salida.join("src/SUMMARY.md")).unwrap();

    let arbol = temporal.path().join("manual");
    copiar_arbol(&libro_de_ejemplo(), &arbol);
    let capitulo = arbol.join("tuneles.md");
    let mut texto = fs::read_to_string(&capitulo).unwrap();
    texto.push_str("\n| a | b |\n|:--|--:|\n| 1 | 2 |\n");
    fs::write(&capitulo, texto).unwrap();

    let e = preparar(&arbol, &salida).expect_err("una tabla alineada tiene que parar");
    let mensaje = e.to_string();
    assert!(
        mensaje.contains("tuneles.md") && mensaje.contains("alineación"),
        "{mensaje}"
    );
    assert_eq!(fs::read(salida.join("src/SUMMARY.md")).unwrap(), antes);
}

// ---------------------------------------------------------------------------
// El flujo entero
// ---------------------------------------------------------------------------

#[test]
fn el_manual_de_ejemplo_se_prepara_entero() {
    let temporal = tempfile::tempdir().unwrap();
    let salida = temporal.path().join("libro");
    let informe = preparar(&libro_de_ejemplo(), &salida).unwrap();

    assert_eq!(informe.capitulos, ["primeros-pasos.md", "tuneles.md"]);
    assert_eq!(
        informe.ficheros,
        [
            "README.md",
            "SUMMARY.md",
            "estado.md",
            "primeros-pasos.md",
            "tuneles.md"
        ]
    );
    // El tema y el libro.
    assert!(salida.join("book.toml").is_file());
    assert!(salida.join("theme/css/ejemplo.css").is_file());
    assert_eq!(
        fs::read(salida.join("book.toml")).unwrap(),
        fs::read(tema_de_ejemplo().join("book.toml")).unwrap()
    );
    // Solo lo que tiene que haber.
    let ficheros: BTreeSet<String> = arbol_de(&salida)
        .into_keys()
        .filter(|r| !r.ends_with('/'))
        .collect();
    let esperados: BTreeSet<String> = [
        "book.toml",
        "src/README.md",
        "src/SUMMARY.md",
        "src/estado.md",
        "src/primeros-pasos.md",
        "src/tuneles.md",
        "theme/css/ejemplo.css",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    assert_eq!(ficheros, esperados);

    let summary = fs::read_to_string(salida.join("src/SUMMARY.md")).unwrap();
    assert_eq!(
        summary,
        "[Manual de MAGI](README.md)\n\n- [Primeros pasos](primeros-pasos.md)\n- [Los túneles y el Include](tuneles.md)\n- [Qué se ha probado](estado.md)\n"
    );

    let primeros = fs::read_to_string(salida.join("src/primeros-pasos.md")).unwrap();
    // Anclas, enlaces y HTML a la vez.
    assert!(
        primeros.contains("## Instalación {#instalación}\n"),
        "{primeros}"
    );
    assert!(
        primeros.contains("## Instalación {#instalación-1}\n"),
        "{primeros}"
    );
    assert!(
        primeros.contains("## ÁRBOL y ÑANDÚ {#árbol-y-ñandú}\n"),
        "{primeros}"
    );
    assert!(primeros.contains("con \\<host> suelto"), "{primeros}");
    assert!(primeros.contains("[el formato](https://github.com/HectorEspejo/magi/blob/v1.6.1/docs/formato-sincronizacion.md#formato)"), "{primeros}");
    assert!(
        primeros.contains(
            "[paquete](https://github.com/HectorEspejo/magi/tree/v1.6.1/packaging/termux/)"
        ),
        "{primeros}"
    );
    assert!(
        primeros.contains("[README](../) y [English](../en/#install)"),
        "{primeros}"
    );
    assert!(primeros.contains("[referencia]: https://github.com/HectorEspejo/magi/blob/v1.6.1/SECURITY.md \"Seguridad\""), "{primeros}");
    // El código se queda como estaba, incluidas las llaves y el `<host>`.
    assert!(
        primeros.contains("magi conectar <host> --nombre {{servicio}}\n"),
        "{primeros}"
    );
    assert!(
        primeros.contains("\n# esto no es un título\n"),
        "{primeros}"
    );
    assert!(primeros.contains("\n## ni esto\n"), "{primeros}");
    assert!(
        primeros.contains("    # ni esto, que lleva sangría\n"),
        "{primeros}"
    );
    assert!(primeros.contains("`[no](../../README.md)`"), "{primeros}");
    assert!(!primeros.contains("se quita"), "{primeros}");
    assert!(!hay_html(&primeros));

    let readme = fs::read_to_string(salida.join("src/README.md")).unwrap();
    assert!(
        readme.contains("[← README](../) · [English README](../en/)"),
        "{readme}"
    );
    assert!(
        readme.contains("# Manual de ejemplo {#manual-de-ejemplo}"),
        "{readme}"
    );
    let tuneles = fs::read_to_string(salida.join("src/tuneles.md")).unwrap();
    assert!(
        tuneles.contains("[portada del manual](index.md)"),
        "{tuneles}"
    );
    assert!(
        tuneles.contains("[primeros pasos](primeros-pasos.md#instalación-1)"),
        "{tuneles}"
    );

    // Los ids de cada capítulo, exactamente los del algoritmo de GitHub sobre el original.
    for nombre in ["README.md", "estado.md", "primeros-pasos.md", "tuneles.md"] {
        let original = fs::read_to_string(libro_de_ejemplo().join(nombre)).unwrap();
        let preparado = fs::read_to_string(salida.join("src").join(nombre)).unwrap();
        assert_eq!(
            ids_de(&preparado),
            slugs_de(&original)
                .into_iter()
                .map(Some)
                .collect::<Vec<_>>(),
            "{nombre}"
        );
    }

    // Una segunda vez, sobre la salida anterior, da lo mismo.
    let antes = arbol_de(&salida);
    preparar(&libro_de_ejemplo(), &salida).unwrap();
    assert_eq!(arbol_de(&salida), antes);
    // Y no deja carpetas de trabajo al lado.
    let vecinos: Vec<String> = fs::read_dir(temporal.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(vecinos, ["libro"]);
}

/// Todo lo que sale de una página como destino tiene que ser de los tipos que
/// espera el libro: capítulo (con o sin ancla), fragmento, `../` o `../en/`, una
/// URL de GitHub en la etiqueta o una externa con esquema.
fn destino_esperado(destino: &str) -> Result<(), String> {
    if destino.is_empty() || destino.starts_with('#') {
        return Ok(());
    }
    if destino == "../" || destino.starts_with("../#") || destino.starts_with("../?") {
        return Ok(());
    }
    if destino == "../en/" || destino.starts_with("../en/#") || destino.starts_with("../en/?") {
        return Ok(());
    }
    for tipo in ["blob", "tree"] {
        let prefijo = format!("{GITHUB}/{tipo}/{ETIQUETA}");
        if destino == prefijo || destino.starts_with(&format!("{prefijo}/")) {
            return Ok(());
        }
    }
    if destino.starts_with("https://") || destino.starts_with("mailto:") {
        return Ok(());
    }
    let sin_resto = destino.split(['#', '?']).next().unwrap_or("");
    let es_pagina = sin_resto.strip_suffix(".md").is_some_and(|n| {
        !n.is_empty()
            && n.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    });
    if es_pagina && !sin_resto.contains('/') {
        return Ok(());
    }
    Err(format!("destino inesperado: {destino:?}"))
}

#[test]
fn el_manual_del_repositorio_se_prepara() {
    let carpeta = raiz().join("docs/manual");
    let temporal = tempfile::tempdir().unwrap();
    let salida = temporal.path().join("libro");
    let informe = preparar(&carpeta, &salida).unwrap();

    // Los 17 capítulos del índice, en su orden, y las demás páginas.
    assert_eq!(informe.capitulos.len(), 17, "{:?}", informe.capitulos);
    let readme = fs::read_to_string(carpeta.join("README.md")).unwrap();
    let indice = readme
        .split("## Capítulos")
        .nth(1)
        .and_then(|r| r.split("\n## ").next())
        .unwrap();
    let del_indice: Vec<String> = indice
        .lines()
        .filter_map(|l| l.strip_prefix("- ["))
        .filter_map(|l| l.split_once("](").map(|(_, r)| r))
        .filter_map(|r| r.split_once(')').map(|(f, _)| f.to_string()))
        .collect();
    assert_eq!(informe.capitulos, del_indice);
    assert_eq!(informe.ficheros.len(), 17 + 3, "{:?}", informe.ficheros);
    assert!(informe.ficheros.contains(&"README.md".to_string()));
    assert!(informe.ficheros.contains(&"estado.md".to_string()));
    assert!(informe.ficheros.contains(&"SUMMARY.md".to_string()));

    let summary = fs::read_to_string(salida.join("src/SUMMARY.md")).unwrap();
    assert_eq!(summary.lines().count(), 2 + 17 + 1, "{summary}");
    for capitulo in &informe.capitulos {
        assert!(summary.contains(&format!("]({capitulo})\n")), "{capitulo}");
    }
    assert!(
        summary.ends_with("- [Qué se ha probado](estado.md)\n"),
        "{summary}"
    );
    assert!(!summary.contains('`'), "{summary}");

    let mut enlaces_que_salen = 0usize;
    let mut enlaces_nuevos = 0usize;
    for nombre in &informe.ficheros {
        let preparado = fs::read_to_string(salida.join("src").join(nombre)).unwrap();
        if nombre == "SUMMARY.md" {
            continue;
        }
        let original = fs::read_to_string(carpeta.join(nombre)).unwrap();

        // Ningún enlace relativo que salga de docs/manual/ sobrevive: de los `](../`
        // solo quedan `](../)` y `](../en/)` (la portada), con o sin ancla.
        for (i, _) in preparado.match_indices("](../") {
            let resto = &preparado[i..];
            assert!(
                ["](../)", "](../en/)", "](../#", "](../en/#"]
                    .iter()
                    .any(|ok| resto.starts_with(ok)),
                "{nombre}: queda un enlace relativo que sale de docs/manual/: {}",
                &resto[..resto.len().min(60)]
            );
        }
        let nuevo = |d: &str| {
            d == "../"
                || d.starts_with("../#")
                || d == "../en/"
                || d.starts_with("../en/#")
                || d.starts_with(&format!("{GITHUB}/blob/{ETIQUETA}/"))
                || d.starts_with(&format!("{GITHUB}/tree/{ETIQUETA}/"))
        };
        for destino in destinos_de(&preparado) {
            destino_esperado(&destino).unwrap_or_else(|e| panic!("{nombre}: {e}"));
        }
        let salen = destinos_de(&original)
            .iter()
            .filter(|d| d.starts_with("../"))
            .count();
        let ahora = destinos_de(&preparado).iter().filter(|d| nuevo(d)).count();
        let antes = destinos_de(&original).iter().filter(|d| nuevo(d)).count();
        assert_eq!(ahora - antes, salen, "{nombre}: enlaces que salen");
        enlaces_que_salen += salen;
        enlaces_nuevos += ahora - antes;
        // Los ids son exactamente los del algoritmo de GitHub sobre el original.
        assert_eq!(
            ids_de(&preparado),
            slugs_de(&original)
                .into_iter()
                .map(Some)
                .collect::<Vec<_>>(),
            "{nombre}"
        );
        // Ningún HTML en bruto fuera de código y ningún `<host>` sin escapar.
        assert!(!hay_html(&preparado), "{nombre}");
        // Sin ningún `{{#` y con los bloques de código intactos.
        assert!(!preparado.contains("{{#"), "{nombre}");
    }
    // Cada fichero ya comprobó que todos sus enlaces que salen de docs/manual/ se
    // reescribieron (`ahora - antes == salen`). El total solo sube con el manual
    // (hoy salen enlaces de README.md, sincronizacion.md, termux.md, app-android.md y
    // estado.md): que no baje de 13 prueba que la prueba sigue viendo enlaces.
    assert!(
        enlaces_que_salen >= 13,
        "enlaces que salen de docs/manual/: {enlaces_que_salen}"
    );
    assert_eq!(enlaces_nuevos, enlaces_que_salen);

    // `<host>` aparece en el manual: en la prosa sale escapado y en el código, igual.
    let servicios = fs::read_to_string(salida.join("src/servicios.md")).unwrap();
    assert!(servicios.contains("\\<host>"), "servicios.md");
    let tuneles = fs::read_to_string(salida.join("src/tuneles.md")).unwrap();
    assert!(
        tuneles.contains("magi tunel activar <host> <nombre>"),
        "tuneles.md"
    );
    // Los enlaces conocidos del manual.
    let indice = fs::read_to_string(salida.join("src/README.md")).unwrap();
    assert!(
        indice.contains("[← README](../) · [English README](../en/)"),
        "README.md"
    );
    assert!(
        indice.contains(&format!(
            "]({GITHUB}/blob/{ETIQUETA}/docs/formato-sincronizacion.md)"
        )),
        "README.md"
    );
    assert!(
        indice.contains(&format!("]({GITHUB}/blob/{ETIQUETA}/SECURITY.md)")),
        "README.md"
    );
    let termux = fs::read_to_string(salida.join("src/termux.md")).unwrap();
    assert!(
        termux.contains(&format!("]({GITHUB}/tree/{ETIQUETA}/packaging/termux/)")),
        "termux.md"
    );
    let android = fs::read_to_string(salida.join("src/app-android.md")).unwrap();
    assert!(
        android.contains(&format!("]({GITHUB}/tree/{ETIQUETA}/android/)")),
        "app-android.md"
    );
    let tuneles = fs::read_to_string(salida.join("src/tuneles.md")).unwrap();
    // «← Índice del manual» pasa por `index.md`.
    assert!(!tuneles.contains("](README.md)"), "tuneles.md");
    assert!(
        tuneles.contains("[← Índice del manual](index.md)"),
        "tuneles.md"
    );
}

// ---------------------------------------------------------------------------
// Imágenes
// ---------------------------------------------------------------------------

const PNG_MINIMO: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

#[test]
fn una_imagen_del_manual_se_copia_con_el_libro() {
    let temporal = tempfile::tempdir().unwrap();
    let carpeta = copia_del_libro(temporal.path());
    fs::write(carpeta.join("captura-1.png"), PNG_MINIMO).unwrap();
    fs::write(carpeta.join("logo.gif"), b"GIF89a\x01\0\x01\0").unwrap();
    let mut texto = fs::read_to_string(carpeta.join("tuneles.md")).unwrap();
    texto.push_str("\n![Captura](captura-1.png) y ![Logo](./logo.gif \"t\").\n");
    fs::write(carpeta.join("tuneles.md"), &texto).unwrap();

    let salida = temporal.path().join("libro");
    let informe = preparar(&carpeta, &salida).unwrap();
    assert!(informe.ficheros.contains(&"captura-1.png".to_string()));
    assert!(informe.ficheros.contains(&"logo.gif".to_string()));
    assert_eq!(
        fs::read(salida.join("src/captura-1.png")).unwrap(),
        PNG_MINIMO
    );
    let tuneles = fs::read_to_string(salida.join("src/tuneles.md")).unwrap();
    assert!(
        tuneles.contains("![Captura](captura-1.png) y ![Logo](logo.gif \"t\")."),
        "{tuneles}"
    );
    // Las imágenes no son capítulos.
    assert_eq!(informe.capitulos, ["primeros-pasos.md", "tuneles.md"]);
}

#[test]
fn una_imagen_que_no_viaja_o_que_no_lo_es_falla() {
    let temporal = tempfile::tempdir().unwrap();

    // Referenciada y ausente: el enlace saldría roto.
    let carpeta = copia_del_libro(&temporal.path().join("a"));
    let mut texto = fs::read_to_string(carpeta.join("tuneles.md")).unwrap();
    texto.push_str("\n![Falta](falta.png)\n");
    fs::write(carpeta.join("tuneles.md"), &texto).unwrap();
    let mensaje = fallo(preparar(&carpeta, &temporal.path().join("a/libro")));
    assert!(mensaje.contains("falta.png"), "{mensaje}");
    assert!(mensaje.contains("tuneles.md:"), "{mensaje}");

    // En una subcarpeta: no se copian las carpetas.
    let carpeta = copia_del_libro(&temporal.path().join("b"));
    let mut texto = fs::read_to_string(carpeta.join("tuneles.md")).unwrap();
    texto.push_str("\n![Dentro](img/captura.png)\n");
    fs::write(carpeta.join("tuneles.md"), &texto).unwrap();
    fs::create_dir(carpeta.join("img")).unwrap();
    fs::write(carpeta.join("img/captura.png"), PNG_MINIMO).unwrap();
    let mensaje = fallo(preparar(&carpeta, &temporal.path().join("b/libro")));
    assert!(mensaje.contains("img"), "{mensaje}");

    // Una imagen que no es lo que dice su extensión.
    for (nombre, contenido) in [
        ("falsa.png", &b"esto no es un PNG"[..]),
        ("vacia.gif", &b""[..]),
    ] {
        let carpeta = copia_del_libro(&temporal.path().join(nombre));
        fs::write(carpeta.join(nombre), contenido).unwrap();
        let mensaje = fallo(preparar(
            &carpeta,
            &temporal.path().join(nombre).join("libro"),
        ));
        assert!(mensaje.contains(nombre), "{nombre}: {mensaje}");
    }
}

/// Revisión MD-01: un SVG suelto se ejecuta con el origen de la web (la CSP del `<meta>`
/// no lo alcanza) y una lista negra de texto no lo cierra (`<s:script>`, entidades de
/// DTD, `<h:script>`…). No se acepta ninguno, sea inocente o no.
#[test]
fn un_svg_no_se_acepta_en_el_manual() {
    let temporal = tempfile::tempdir().unwrap();
    for (nombre, contenido) in [
        (
            "logo.svg",
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"1\" height=\"1\"/></svg>",
        ),
        (
            "prefijo.svg",
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:s=\"http://www.w3.org/2000/svg\"><s:script>alert(1)</s:script></svg>",
        ),
        (
            "entidad.svg",
            "<!DOCTYPE svg [<!ENTITY e \"&#60;script&#62;alert(1)&#60;/script&#62;\">]><svg xmlns=\"http://www.w3.org/2000/svg\">&e;</svg>",
        ),
        ("mala.svg", "<svg onload=\"alert(1)\"></svg>"),
    ] {
        let carpeta = copia_del_libro(&temporal.path().join(nombre));
        fs::write(carpeta.join(nombre), contenido).unwrap();
        let mensaje = fallo(preparar(
            &carpeta,
            &temporal.path().join(nombre).join("libro"),
        ));
        assert!(
            mensaje.contains(nombre) && mensaje.contains("nombre de fichero no admitido"),
            "{nombre}: {mensaje}"
        );
    }
    // Ni siquiera enlazado desde un capítulo.
    let carpeta = copia_del_libro(&temporal.path().join("enlazado"));
    fs::write(carpeta.join("logo.svg"), "<svg xmlns=\"x\"/>").unwrap();
    let mut texto = fs::read_to_string(carpeta.join("tuneles.md")).unwrap();
    texto.push_str("\n![Logo](logo.svg)\n");
    fs::write(carpeta.join("tuneles.md"), &texto).unwrap();
    let mensaje = fallo(preparar(&carpeta, &temporal.path().join("enlazado/libro")));
    assert!(mensaje.contains("logo.svg"), "{mensaje}");
}

/// Revisión MD-02: el buscador de mdBook 0.5.4 pone el título de cada resultado con
/// `innerHTML`; un «<» en un título (crudo, como entidad o dentro de código) volvería
/// a ser marcado, y un `<meta http-equiv=refresh>` redirigiría al abrir la búsqueda.
#[test]
fn un_titulo_con_menor_que_se_rechaza_en_cualquiera_de_sus_formas() {
    for titulo in [
        "## Zorro <meta http-equiv=refresh content=0;url=http://127.0.0.1/>",
        "## Zorro &lt;meta http-equiv=refresh content=0;url=http://127.0.0.1/&gt;",
        "## Zorro `<meta http-equiv=refresh content=0;url=http://127.0.0.1/>`",
        "# Un <host>",
        "### Con *<énfasis>*",
    ] {
        let texto = format!("intro\n\n{titulo}\n\ncuerpo\n");
        let mensaje = fallo(rechazar_menor_que_en_titulos(&texto, "capitulo.md"));
        assert!(
            mensaje.contains("capitulo.md:3") && mensaje.contains("«<»"),
            "{titulo}: {mensaje}"
        );
    }
    // Lo que sí vale: «>», flechas, y el «<» en el cuerpo (que mdBook sí escapa).
    for bien in [
        "## A -> B\n",
        "## Mayor que > menor\n",
        "# Título\n\nel host es <host> y `<uuid>`\n",
        "```\n## <no es un título>\n```\n",
        "",
    ] {
        rechazar_menor_que_en_titulos(bien, "t.md").unwrap_or_else(|e| panic!("{bien:?}: {e}"));
    }
}

#[test]
fn un_titulo_del_indice_con_menor_que_se_rechaza() {
    let readme = "# Manual\n\n## Capítulos\n\n- [Cap <meta http-equiv=refresh content=0;url=x>](tuneles.md) — x\n";
    let mensaje = fallo(summary(readme, &nombres(&["estado.md", "tuneles.md"])));
    assert!(
        mensaje.contains("README.md") && mensaje.contains("«<»"),
        "{mensaje}"
    );
}

/// Revisión MD-05: un capítulo con el nombre de una página de mdBook pisaría la portada
/// del manual (`index.md`, `readme.md`) o el relleno de la barra lateral (`toc.md`).
#[test]
fn un_capitulo_no_puede_llamarse_como_una_pagina_de_mdbook() {
    let temporal = tempfile::tempdir().unwrap();
    for nombre in ["index.md", "readme.md", "toc.md", "print.md", "404.md"] {
        let carpeta = copia_del_libro(&temporal.path().join(nombre));
        fs::write(carpeta.join(nombre), "# Pisa una página\n").unwrap();
        let mensaje = fallo(preparar(
            &carpeta,
            &temporal.path().join(nombre).join("libro"),
        ));
        assert!(
            mensaje.contains(nombre) && mensaje.contains("nombre de fichero no admitido"),
            "{nombre}: {mensaje}"
        );
    }
}

/// Revisión MD-08: un salto de línea que es solo CR convertía el capítulo entero en un
/// único título (`encabezados_de` lee por `\n` y `\r\n`) y se publicaba roto sin avisar.
#[test]
fn un_salto_de_linea_solo_cr_se_rechaza_con_su_posicion() {
    let temporal = tempfile::tempdir().unwrap();
    let carpeta = copia_del_libro(&temporal.path().join("cr"));
    fs::write(carpeta.join("tuneles.md"), "# Túneles\r\r## Alta\rtexto\r").unwrap();
    let mensaje = fallo(preparar(&carpeta, &temporal.path().join("cr/libro")));
    assert!(
        mensaje.contains("tuneles.md") && mensaje.contains("CR"),
        "{mensaje}"
    );
    // CRLF sigue valiendo.
    let carpeta = copia_del_libro(&temporal.path().join("crlf"));
    fs::write(
        carpeta.join("tuneles.md"),
        "# Túneles\r\n\r\n## Alta\r\ntexto\r\n",
    )
    .unwrap();
    preparar(&carpeta, &temporal.path().join("crlf/libro")).expect("CRLF vale");
}

// ---------------------------------------------------------------------------
// Nombres, topes y entradas raras (árbol de trabajo)
// ---------------------------------------------------------------------------

#[test]
fn un_nombre_de_fichero_raro_falla() {
    let temporal = tempfile::tempdir().unwrap();
    for (i, nombre) in [
        "Notas.md",
        "a_b.md",
        "a b.md",
        "x.sh",
        "x.md.orig",
        "x.md~",
        ".oculto.md",
        "ñ.md",
        "README.MD",
        "readme.md",
        "SUMMARY.md",
        "summary.md",
        "a\nb.md",
        "datos",
        "x.bmp",
    ]
    .iter()
    .enumerate()
    {
        let base = temporal.path().join(format!("caso{i}"));
        let carpeta = copia_del_libro(&base);
        fs::write(carpeta.join(nombre), "# Raro\n").unwrap();
        let salida = base.join("libro");
        let mensaje = fallo(preparar(&carpeta, &salida));
        let visible = nombre.replace('\n', " ");
        assert!(
            mensaje.contains(&visible),
            "{nombre:?}: el error no nombra el fichero: {mensaje}"
        );
        assert!(!salida.exists(), "{nombre:?}: no tenía que escribir nada");
    }
}

#[test]
fn un_summary_md_en_el_indice_tampoco_se_admite() {
    // Aunque el índice lo nombre (y no sea «fuera del índice»), `SUMMARY.md` lo genera
    // `preparar-manual`: en un sistema de ficheros que no distingue mayúsculas
    // pisaría al suyo.
    let temporal = tempfile::tempdir().unwrap();
    let carpeta = copia_del_libro(temporal.path());
    let readme = fs::read_to_string(carpeta.join("README.md")).unwrap();
    let con_summary = readme.replacen(
        "## Capítulos\n",
        "## Capítulos\n\n- [Resumen](summary.md)\n",
        1,
    );
    assert_ne!(
        readme, con_summary,
        "el README de ejemplo no tiene la sección"
    );
    fs::write(carpeta.join("README.md"), con_summary).unwrap();
    fs::write(carpeta.join("summary.md"), "# Resumen\n").unwrap();
    let salida = temporal.path().join("libro");
    let mensaje = fallo(preparar(&carpeta, &salida));
    assert!(mensaje.contains("SUMMARY.md"), "{mensaje}");
    assert!(!salida.exists());
}

#[test]
fn las_carpetas_y_los_enlaces_simbolicos_de_un_arbol_fallan() {
    let temporal = tempfile::tempdir().unwrap();
    // Una carpeta dentro de docs/manual/.
    let base = temporal.path().join("carpeta");
    let carpeta = copia_del_libro(&base);
    fs::create_dir(carpeta.join("subcarpeta")).unwrap();
    let mensaje = fallo(preparar(&carpeta, &base.join("libro")));
    assert!(mensaje.contains("subcarpeta"), "{mensaje}");

    // Un enlace simbólico a un capítulo (nunca se sigue) o a algo fuera.
    for (i, destino) in ["README.md", "/etc/hostname", "inexistente.md"]
        .iter()
        .enumerate()
    {
        let base = temporal.path().join(format!("enlace{i}"));
        let carpeta = copia_del_libro(&base);
        std::os::unix::fs::symlink(destino, carpeta.join("enlace.md")).unwrap();
        let mensaje = fallo(preparar(&carpeta, &base.join("libro")));
        assert!(mensaje.contains("enlace.md"), "{destino}: {mensaje}");
        assert!(mensaje.contains("simbólico"), "{destino}: {mensaje}");
    }
    // La propia carpeta como enlace simbólico.
    let base = temporal.path().join("raiz");
    let real = copia_del_libro(&base);
    let enlace = base.join("enlace");
    std::os::unix::fs::symlink(&real, &enlace).unwrap();
    let mensaje = fallo(preparar(&enlace, &base.join("libro")));
    assert!(
        mensaje.contains("simbólico") || mensaje.contains("enlace"),
        "{mensaje}"
    );
}

#[test]
fn los_topes_de_tamano_y_de_cantidad_se_respetan() {
    let temporal = tempfile::tempdir().unwrap();

    // Un fichero de más de 2 MiB.
    let base = temporal.path().join("grande");
    let carpeta = copia_del_libro(&base);
    let mut grande = fs::read_to_string(carpeta.join("tuneles.md")).unwrap();
    while grande.len() <= 2 * 1024 * 1024 {
        grande.push_str("Relleno de ejemplo para pasar el tope del fichero.\n");
    }
    fs::write(carpeta.join("tuneles.md"), grande).unwrap();
    let mensaje = fallo(preparar(&carpeta, &base.join("libro")));
    assert!(mensaje.contains("tuneles.md"), "{mensaje}");
    assert!(mensaje.contains("tope"), "{mensaje}");

    // Más de 64 ficheros.
    let base = temporal.path().join("muchos");
    let carpeta = copia_del_libro(&base);
    for i in 0..70 {
        fs::write(carpeta.join(format!("extra-{i}.png")), PNG_MINIMO).unwrap();
    }
    let mensaje = fallo(preparar(&carpeta, &base.join("libro")));
    assert!(mensaje.contains("64"), "{mensaje}");
}

#[test]
fn un_fichero_que_no_es_utf8_o_lleva_nul_falla() {
    let temporal = tempfile::tempdir().unwrap();
    for (nombre, contenido) in [
        ("latin1.md", &b"# T\xedtulo\n"[..]),
        ("nulo.md", &b"# Titulo\n\0\n"[..]),
    ] {
        let base = temporal.path().join(nombre);
        let carpeta = copia_del_libro(&base);
        fs::write(carpeta.join(nombre), contenido).unwrap();
        let mut readme = fs::read_to_string(carpeta.join("README.md")).unwrap();
        readme = readme.replace(
            "- [Los `túneles` y el `Include`](tuneles.md)",
            &format!("- [Raro]({nombre})\n- [Los `túneles` y el `Include`](tuneles.md)"),
        );
        fs::write(carpeta.join("README.md"), readme).unwrap();
        let mensaje = fallo(preparar(&carpeta, &base.join("libro")));
        assert!(mensaje.contains(nombre), "{nombre}: {mensaje}");
    }
}

#[test]
fn un_capitulo_fuera_del_indice_para_la_preparacion() {
    let temporal = tempfile::tempdir().unwrap();
    let carpeta = copia_del_libro(temporal.path());
    fs::write(carpeta.join("suelto.md"), "# Suelto\n").unwrap();
    let salida = temporal.path().join("libro");
    let mensaje = fallo(preparar(&carpeta, &salida));
    assert!(
        mensaje.contains("capítulo fuera del índice: suelto.md"),
        "{mensaje}"
    );
    assert!(!salida.exists());
}

#[test]
fn cualquier_error_de_un_capitulo_dice_fichero_y_linea() {
    let temporal = tempfile::tempdir().unwrap();
    for (i, (malo, trozo)) in [
        ("[x](../../../fuera.md)", "tuneles.md:"),
        ("{{#include otro.md}}", "tuneles.md:"),
        ("## Con id {#mio}", "tuneles.md:"),
        ("Título\n======", "tuneles.md:"),
        ("<!-- sin cerrar", "tuneles.md:"),
    ]
    .iter()
    .enumerate()
    {
        let base = temporal.path().join(format!("caso{i}"));
        let carpeta = copia_del_libro(&base);
        let mut texto = fs::read_to_string(carpeta.join("tuneles.md")).unwrap();
        texto.push_str(&format!("\n{malo}\n"));
        fs::write(carpeta.join("tuneles.md"), &texto).unwrap();
        let salida = base.join("libro");
        let mensaje = fallo(preparar(&carpeta, &salida));
        assert!(mensaje.contains(trozo), "{malo}: {mensaje}");
        assert!(
            mensaje.contains("docs/manual/tuneles.md:"),
            "{malo}: {mensaje}"
        );
        assert!(!salida.exists(), "{malo}");
    }
}

#[test]
fn una_etiqueta_rara_se_rechaza() {
    let temporal = tempfile::tempdir().unwrap();
    for (i, mala) in [
        "v1.6.1-rc.1",
        "1.6.1",
        "v1.6",
        "v1.6.1 ",
        "v1.6.1 --upload-pack=x",
        "v1.6.1/../x",
        "../v1.6.1",
        "refs/tags/v1.6.1",
        "-v1.6.1",
        "v01.6.1",
        "main",
        "",
        "android-v0.2.0",
    ]
    .iter()
    .enumerate()
    {
        let salida = temporal.path().join(format!("libro{i}"));
        for origen in [
            OrigenManual::Arbol {
                carpeta: libro_de_ejemplo(),
            },
            OrigenManual::Etiqueta {
                repositorio: raiz(),
            },
        ] {
            let mensaje = fallo(preparar_manual(&OpcionesManual {
                origen,
                etiqueta: (*mala).to_string(),
                tema: tema_de_ejemplo(),
                salida: salida.clone(),
            }));
            assert!(mensaje.contains("--etiqueta"), "{mala:?}: {mensaje}");
            assert!(!salida.exists(), "{mala:?}");
        }
    }
}

// ---------------------------------------------------------------------------
// Con git: la etiqueta, no el árbol de trabajo
// ---------------------------------------------------------------------------

/// `git` sin la configuración del equipo (firmas, ganchos) y con una identidad
/// inventada.
fn git(dir: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .current_dir(dir)
        .args([
            "-c",
            "user.name=usuario",
            "-c",
            "user.email=usuario@ejemplo.org",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "tag.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("HOME", dir)
        .env("XDG_CONFIG_HOME", dir)
        .output()
        .unwrap_or_else(|e| panic!("no se pudo ejecutar git: {e}"))
}

fn git_bien(dir: &Path, args: &[&str]) -> String {
    let salida = git(dir, args);
    assert!(
        salida.status.success(),
        "git {args:?} falló: {}",
        String::from_utf8_lossy(&salida.stderr)
    );
    String::from_utf8_lossy(&salida.stdout).trim().to_string()
}

/// ¿Hay `git`? Sin él, la prueba se omite con un aviso, salvo con `CI` puesta:
/// entonces falla.
fn hay_git() -> bool {
    let hay = Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|s| s.status.success());
    if !hay {
        assert!(
            std::env::var_os("CI").is_none(),
            "en CI tiene que haber git"
        );
        eprintln!("aviso: sin git, prueba omitida");
    }
    hay
}

/// Un repositorio con el manual de ejemplo en `docs/manual/`, confirmado y con la
/// etiqueta `v1.6.1`. `antes` puede añadir cosas antes de confirmar.
fn repositorio_con_manual(antes: impl FnOnce(&Path, &Path)) -> (tempfile::TempDir, PathBuf) {
    let temporal = tempfile::tempdir().unwrap();
    let repo = temporal.path().join("repo");
    fs::create_dir(&repo).unwrap();
    git_bien(&repo, &["init", "-q"]);
    git_bien(&repo, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    let manual = repo.join("docs/manual");
    copiar_arbol(&libro_de_ejemplo(), &manual);
    antes(&repo, &manual);
    git_bien(&repo, &["add", "-A"]);
    git_bien(&repo, &["commit", "-q", "-m", "manual de ejemplo"]);
    git_bien(&repo, &["tag", ETIQUETA]);
    (temporal, repo)
}

fn opciones_con_etiqueta(repo: &Path, salida: &Path) -> OpcionesManual {
    OpcionesManual {
        origen: OrigenManual::Etiqueta {
            repositorio: repo.to_path_buf(),
        },
        etiqueta: ETIQUETA.to_string(),
        tema: tema_de_ejemplo(),
        salida: salida.to_path_buf(),
    }
}

#[test]
fn desde_una_etiqueta_con_git() {
    if !hay_git() {
        return;
    }
    let (temporal, repo) = repositorio_con_manual(|_, manual| {
        // Un ejecutable también es un fichero normal.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let ruta = manual.join("estado.md");
            let mut permisos = fs::metadata(&ruta).unwrap().permissions();
            permisos.set_mode(0o755);
            fs::set_permissions(&ruta, permisos).unwrap();
        }
        fs::write(manual.join("captura.png"), PNG_MINIMO).unwrap();
    });
    // Lo que venga después de la etiqueta (un commit y cambios sin confirmar) no
    // cuenta: se lee lo publicado.
    fs::write(
        repo.join("docs/manual/tuneles.md"),
        "# Cambiado\n\nOtro texto.\n",
    )
    .unwrap();
    git_bien(&repo, &["add", "-A"]);
    git_bien(&repo, &["commit", "-q", "-m", "después de la etiqueta"]);
    fs::write(
        repo.join("docs/manual/primeros-pasos.md"),
        "# Sin confirmar\n",
    )
    .unwrap();
    fs::write(repo.join("docs/manual/extra.md"), "# Sin confirmar\n").unwrap();

    let salida = temporal.path().join("libro");
    let informe = preparar_manual(&opciones_con_etiqueta(&repo, &salida)).unwrap();
    assert_eq!(informe.capitulos, ["primeros-pasos.md", "tuneles.md"]);
    assert!(informe.ficheros.contains(&"captura.png".to_string()));
    assert!(!informe.ficheros.contains(&"extra.md".to_string()));
    let tuneles = fs::read_to_string(salida.join("src/tuneles.md")).unwrap();
    assert!(tuneles.contains("# Túneles {#túneles}"), "{tuneles}");
    assert_eq!(
        fs::read(salida.join("src/captura.png")).unwrap(),
        PNG_MINIMO
    );

    // Es lo mismo que leer el árbol tal como estaba al etiquetar.
    let original = temporal.path().join("original");
    copiar_arbol(&libro_de_ejemplo(), &original);
    fs::write(original.join("captura.png"), PNG_MINIMO).unwrap();
    let del_arbol = temporal.path().join("del-arbol");
    preparar(&original, &del_arbol).unwrap();
    assert_eq!(arbol_de(&salida), arbol_de(&del_arbol));
}

#[test]
fn desde_una_etiqueta_anotada_y_con_el_repositorio_relativo() {
    if !hay_git() {
        return;
    }
    let (temporal, repo) = repositorio_con_manual(|_, _| {});
    // La misma etiqueta, pero anotada.
    git_bien(&repo, &["tag", "-d", ETIQUETA]);
    git_bien(&repo, &["tag", "-a", ETIQUETA, "-m", "versión de ejemplo"]);
    let salida = temporal.path().join("libro");
    let informe = preparar_manual(&opciones_con_etiqueta(&repo, &salida)).unwrap();
    assert_eq!(informe.capitulos.len(), 2);
    // Una rama con el mismo nombre que la etiqueta no la sustituye.
    git_bien(&repo, &["checkout", "-q", "-b", "v1.6.2"]);
    fs::write(repo.join("docs/manual/extra.md"), "# Rama\n").unwrap();
    git_bien(&repo, &["add", "-A"]);
    git_bien(&repo, &["commit", "-q", "-m", "rama"]);
    let mut o = opciones_con_etiqueta(&repo, &temporal.path().join("rama"));
    o.etiqueta = "v1.6.2".to_string();
    let mensaje = fallo(preparar_manual(&o));
    assert!(mensaje.contains("v1.6.2"), "{mensaje}");
    assert!(mensaje.contains("no existe"), "{mensaje}");
}

#[test]
fn una_etiqueta_que_no_existe_falla_diciendo_cual() {
    if !hay_git() {
        return;
    }
    let (temporal, repo) = repositorio_con_manual(|_, _| {});
    let salida = temporal.path().join("libro");
    let mut o = opciones_con_etiqueta(&repo, &salida);
    o.etiqueta = "v9.9.9".to_string();
    let mensaje = fallo(preparar_manual(&o));
    assert!(mensaje.contains("v9.9.9"), "{mensaje}");
    assert!(mensaje.contains("no existe"), "{mensaje}");
    assert!(!salida.exists());

    // Una etiqueta sin docs/manual/ (anterior a la 9b).
    let (temporal, repo) = repositorio_con_manual(|repo, manual| {
        fs::remove_dir_all(manual).unwrap();
        fs::write(repo.join("README.md"), "# Sin manual\n").unwrap();
    });
    let mensaje = fallo(preparar_manual(&opciones_con_etiqueta(
        &repo,
        &temporal.path().join("libro"),
    )));
    assert!(mensaje.contains("v1.6.1"), "{mensaje}");
    assert!(mensaje.contains("docs/manual"), "{mensaje}");

    // Una carpeta que no es un repositorio, o que no existe.
    let vacia = temporal.path().join("vacia");
    fs::create_dir(&vacia).unwrap();
    let mensaje = fallo(preparar_manual(&opciones_con_etiqueta(
        &vacia,
        &temporal.path().join("otro"),
    )));
    assert!(mensaje.contains(ETIQUETA), "{mensaje}");
    let mensaje = fallo(preparar_manual(&opciones_con_etiqueta(
        &temporal.path().join("no-existe"),
        &temporal.path().join("otro"),
    )));
    assert!(mensaje.contains("no-existe"), "{mensaje}");
}

#[cfg(unix)]
#[test]
fn una_etiqueta_con_enlace_simbolico_o_submodulo_falla() {
    if !hay_git() {
        return;
    }
    // Un enlace simbólico (modo 120000) con un nombre válido.
    let (temporal, repo) = repositorio_con_manual(|_, manual| {
        std::os::unix::fs::symlink("README.md", manual.join("enlace.md")).unwrap();
    });
    let salida = temporal.path().join("libro");
    let mensaje = fallo(preparar_manual(&opciones_con_etiqueta(&repo, &salida)));
    assert!(mensaje.contains("enlace.md"), "{mensaje}");
    assert!(mensaje.contains("120000"), "{mensaje}");
    assert!(!salida.exists());

    // Un enlace a algo de fuera del repositorio, con nombre de imagen.
    let (temporal, repo) = repositorio_con_manual(|_, manual| {
        std::os::unix::fs::symlink("/etc/hostname", manual.join("foto.png")).unwrap();
    });
    let mensaje = fallo(preparar_manual(&opciones_con_etiqueta(
        &repo,
        &temporal.path().join("libro"),
    )));
    assert!(mensaje.contains("foto.png"), "{mensaje}");
    assert!(mensaje.contains("120000"), "{mensaje}");

    // Un submódulo (modo 160000): nunca se baja nada.
    let (temporal, repo) = repositorio_con_manual(|_, _| {});
    git_bien(
        &repo,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            "160000,1234567890123456789012345678901234567890,docs/manual/modulo",
        ],
    );
    git_bien(&repo, &["commit", "-q", "-m", "submódulo"]);
    git_bien(&repo, &["tag", "-f", ETIQUETA]);
    let mensaje = fallo(preparar_manual(&opciones_con_etiqueta(
        &repo,
        &temporal.path().join("libro"),
    )));
    assert!(mensaje.contains("modulo"), "{mensaje}");
    assert!(mensaje.contains("160000"), "{mensaje}");

    // Una carpeta (040000).
    let (temporal, repo) = repositorio_con_manual(|_, manual| {
        fs::create_dir(manual.join("img")).unwrap();
        fs::write(manual.join("img/captura.png"), PNG_MINIMO).unwrap();
    });
    let mensaje = fallo(preparar_manual(&opciones_con_etiqueta(
        &repo,
        &temporal.path().join("libro"),
    )));
    assert!(mensaje.contains("img"), "{mensaje}");
    assert!(mensaje.contains("040000"), "{mensaje}");
}

#[cfg(unix)]
#[test]
fn un_nombre_raro_en_una_etiqueta_falla() {
    if !hay_git() {
        return;
    }
    for nombre in [
        "Notas.md",
        "a_b.md",
        "x.sh",
        "a b.md",
        "a\nb.md",
        "ñ.md",
        "SUMMARY.md",
    ] {
        let (temporal, repo) = repositorio_con_manual(|_, manual| {
            fs::write(manual.join(nombre), "# Raro\n").unwrap();
        });
        let salida = temporal.path().join("libro");
        let mensaje = fallo(preparar_manual(&opciones_con_etiqueta(&repo, &salida)));
        let visible = nombre.replace('\n', " ");
        assert!(mensaje.contains(&visible), "{nombre:?}: {mensaje}");
        assert!(!salida.exists(), "{nombre:?}");
    }
}

#[test]
fn una_etiqueta_con_demasiados_ficheros_falla_sin_leerlos() {
    if !hay_git() {
        return;
    }
    let (temporal, repo) = repositorio_con_manual(|_, manual| {
        for i in 0..70 {
            fs::write(manual.join(format!("extra-{i}.png")), PNG_MINIMO).unwrap();
        }
    });
    let salida = temporal.path().join("libro");
    let mensaje = fallo(preparar_manual(&opciones_con_etiqueta(&repo, &salida)));
    assert!(mensaje.contains("64"), "{mensaje}");
    assert!(!salida.exists());
}

#[test]
fn una_etiqueta_con_un_fichero_demasiado_grande_falla_sin_leerlo() {
    if !hay_git() {
        return;
    }
    let (temporal, repo) = repositorio_con_manual(|_, manual| {
        let mut grande = String::new();
        while grande.len() <= 2 * 1024 * 1024 {
            grande.push_str("Relleno de ejemplo para pasar el tope del fichero.\n");
        }
        fs::write(manual.join("tuneles.md"), grande).unwrap();
    });
    let mensaje = fallo(preparar_manual(&opciones_con_etiqueta(
        &repo,
        &temporal.path().join("libro"),
    )));
    assert!(mensaje.contains("tuneles.md"), "{mensaje}");
    assert!(mensaje.contains("tope"), "{mensaje}");
}

// ---------------------------------------------------------------------------
// La salida: sin pisar las entradas ni dejarla a medias
// ---------------------------------------------------------------------------

#[test]
fn la_salida_no_pisa_las_entradas() {
    let temporal = tempfile::tempdir().unwrap();
    let carpeta = copia_del_libro(temporal.path());
    let tema = temporal.path().join("tema");
    copiar_arbol(&tema_de_ejemplo(), &tema);
    let opciones = |salida: &Path| OpcionesManual {
        origen: OrigenManual::Arbol {
            carpeta: carpeta.clone(),
        },
        etiqueta: ETIQUETA.to_string(),
        tema: tema.clone(),
        salida: salida.to_path_buf(),
    };
    let antes_carpeta = arbol_de(&carpeta);
    let antes_tema = arbol_de(&tema);

    for (que, salida) in [
        ("el tema", tema.clone()),
        ("dentro del tema", tema.join("theme")),
        ("dentro del tema (nueva)", tema.join("nueva")),
        ("el manual", carpeta.clone()),
        ("dentro del manual", carpeta.join("libro")),
        ("la carpeta que lo contiene", temporal.path().to_path_buf()),
        ("con ..", temporal.path().join("a/../libro")),
        ("sin nombre", PathBuf::from("/")),
    ] {
        let mensaje = fallo(preparar_manual(&opciones(&salida)));
        assert!(mensaje.contains("--salida"), "{que}: {mensaje}");
        assert_eq!(arbol_de(&carpeta), antes_carpeta, "{que}");
        assert_eq!(arbol_de(&tema), antes_tema, "{que}");
    }
    assert!(!temporal.path().join("libro").exists());

    // Una carpeta que existe, no está vacía y no es un libro preparado antes: no se
    // sustituye (`--salida docs` no puede borrar `docs/`).
    let ajena = temporal.path().join("ajena");
    fs::create_dir_all(ajena.join("sub")).unwrap();
    fs::write(ajena.join("importante.txt"), "no me borres").unwrap();
    let antes = arbol_de(&ajena);
    let mensaje = fallo(preparar_manual(&opciones(&ajena)));
    assert!(mensaje.contains("no está vacía"), "{mensaje}");
    assert_eq!(arbol_de(&ajena), antes);
    // Un fichero o un enlace simbólico como salida.
    let fichero = temporal.path().join("fichero");
    fs::write(&fichero, "x").unwrap();
    assert!(preparar_manual(&opciones(&fichero)).is_err());
    assert_eq!(fs::read(&fichero).unwrap(), b"x");
    #[cfg(unix)]
    {
        let enlace = temporal.path().join("enlace");
        std::os::unix::fs::symlink(&ajena, &enlace).unwrap();
        assert!(preparar_manual(&opciones(&enlace)).is_err());
        assert_eq!(arbol_de(&ajena), antes);
    }

    // Una vacía sí, y un libro preparado antes también.
    let vacia = temporal.path().join("vacia");
    fs::create_dir(&vacia).unwrap();
    preparar_manual(&opciones(&vacia)).unwrap();
    assert!(vacia.join("src/SUMMARY.md").is_file());
    preparar_manual(&opciones(&vacia)).unwrap();
}

#[test]
fn la_salida_de_una_etiqueta_no_pisa_el_repositorio() {
    if !hay_git() {
        return;
    }
    let (temporal, repo) = repositorio_con_manual(|_, _| {});
    let antes = arbol_de(&repo.join("docs"));
    for (que, salida) in [
        ("el repositorio", repo.clone()),
        ("lo que lo contiene", temporal.path().to_path_buf()),
        ("dentro de .git", repo.join(".git/libro")),
    ] {
        let mensaje = fallo(preparar_manual(&opciones_con_etiqueta(&repo, &salida)));
        assert!(mensaje.contains("--salida"), "{que}: {mensaje}");
    }
    assert!(repo.join(".git/HEAD").is_file());
    // `docs/manual` del propio repositorio tampoco: no está vacía ni es un libro.
    let mensaje = fallo(preparar_manual(&opciones_con_etiqueta(
        &repo,
        &repo.join("docs/manual"),
    )));
    assert!(
        mensaje.contains("--salida") || mensaje.contains("no está vacía"),
        "{mensaje}"
    );
    assert_eq!(arbol_de(&repo.join("docs")), antes);
    // Dentro del repositorio, en una carpeta propia (como `target/web/libro`), sí.
    let propia = repo.join("target/web/libro");
    preparar_manual(&opciones_con_etiqueta(&repo, &propia)).unwrap();
    assert!(propia.join("src/SUMMARY.md").is_file());
}

#[test]
fn un_fallo_no_deja_la_salida_a_medias() {
    let temporal = tempfile::tempdir().unwrap();
    let carpeta = copia_del_libro(temporal.path());
    let salida = temporal.path().join("libro");
    preparar(&carpeta, &salida).unwrap();
    let anterior = arbol_de(&salida);
    assert!(!anterior.is_empty());

    let sin_restos = || {
        let vecinos: Vec<String> = fs::read_dir(temporal.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        let mut esperados = vec!["libro".to_string(), "manual".to_string()];
        esperados.sort();
        let mut vistos = vecinos;
        vistos.sort();
        assert_eq!(vistos, esperados, "han quedado restos junto a la salida");
    };

    // 1. Falla al leer o adaptar (antes de escribir nada).
    fs::write(carpeta.join("suelto.md"), "# Suelto\n").unwrap();
    assert!(preparar(&carpeta, &salida).is_err());
    assert_eq!(arbol_de(&salida), anterior);
    sin_restos();
    fs::remove_file(carpeta.join("suelto.md")).unwrap();

    // 2. Falla al adaptar un capítulo concreto (los demás ya estaban listos).
    let original = fs::read_to_string(carpeta.join("tuneles.md")).unwrap();
    fs::write(
        carpeta.join("tuneles.md"),
        format!("{original}\n[x](../../../fuera)\n"),
    )
    .unwrap();
    assert!(preparar(&carpeta, &salida).is_err());
    assert_eq!(arbol_de(&salida), anterior);
    sin_restos();
    fs::write(carpeta.join("tuneles.md"), original).unwrap();

    // 3. Falla al escribir, ya con la carpeta de trabajo a medias (el tema no está).
    let mut o = opciones_con_arbol(&carpeta, &salida);
    let sin_tema = temporal.path().join("sin-tema");
    fs::create_dir(&sin_tema).unwrap();
    o.tema = sin_tema;
    let mensaje = fallo(preparar_manual(&o));
    assert!(mensaje.contains("book.toml"), "{mensaje}");
    assert_eq!(arbol_de(&salida), anterior);
    let vecinos: BTreeSet<String> = fs::read_dir(temporal.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        vecinos,
        ["libro", "manual", "sin-tema"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        "han quedado restos junto a la salida"
    );

    // 4. Y con todo bien, la sustituye.
    fs::write(carpeta.join("tuneles.md"), "# Nuevo\n\nOtro capítulo.\n").unwrap();
    preparar(&carpeta, &salida).unwrap();
    assert_ne!(arbol_de(&salida), anterior);
    assert!(fs::read_to_string(salida.join("src/tuneles.md"))
        .unwrap()
        .contains("# Nuevo {#nuevo}"));
}

#[test]
fn el_tema_se_copia_sin_enlaces_simbolicos() {
    let temporal = tempfile::tempdir().unwrap();
    let carpeta = copia_del_libro(temporal.path());
    let salida = temporal.path().join("libro");

    // Se copia tal cual: `book.toml` y todo `theme/`.
    let tema = temporal.path().join("tema");
    copiar_arbol(&tema_de_ejemplo(), &tema);
    fs::write(
        tema.join("theme/head.hbs"),
        "<meta name=\"x\" content=\"y\">\n",
    )
    .unwrap();
    fs::create_dir_all(tema.join("theme/fonts")).unwrap();
    fs::write(tema.join("theme/fonts/f.woff2"), b"wOF2\0\0").unwrap();
    // Lo que no es ni `book.toml` ni `theme/` no viaja.
    fs::write(tema.join("README.md"), "# Notas del tema\n").unwrap();
    let mut o = opciones_con_arbol(&carpeta, &salida);
    o.tema = tema.clone();
    preparar_manual(&o).unwrap();
    for ruta in [
        "theme/css/ejemplo.css",
        "theme/head.hbs",
        "theme/fonts/f.woff2",
        "book.toml",
    ] {
        assert_eq!(
            fs::read(salida.join(ruta)).unwrap(),
            fs::read(tema.join(ruta)).unwrap(),
            "{ruta}"
        );
    }
    assert!(!salida.join("README.md").exists());
    let con_tema = arbol_de(&salida);

    // Sin book.toml, o con él como enlace simbólico, falla.
    #[cfg(unix)]
    {
        type Estropear = Box<dyn Fn(&Path)>;
        let casos: Vec<(&str, Estropear)> = vec![
            (
                "sin book.toml",
                Box::new(|t| fs::remove_file(t.join("book.toml")).unwrap()),
            ),
            (
                "book.toml enlazado",
                Box::new(|t| {
                    let real = t.join("real.toml");
                    fs::rename(t.join("book.toml"), &real).unwrap();
                    std::os::unix::fs::symlink(&real, t.join("book.toml")).unwrap();
                }),
            ),
            (
                "un fichero de theme enlazado",
                Box::new(|t| {
                    std::os::unix::fs::symlink("/etc/hostname", t.join("theme/css/enlace.css"))
                        .unwrap();
                }),
            ),
            (
                "una carpeta de theme enlazada",
                Box::new(|t| {
                    std::os::unix::fs::symlink(t.join("theme/css"), t.join("theme/otra")).unwrap();
                }),
            ),
            (
                "theme enlazado",
                Box::new(|t| {
                    let real = t.join("theme-real");
                    fs::rename(t.join("theme"), &real).unwrap();
                    std::os::unix::fs::symlink(&real, t.join("theme")).unwrap();
                }),
            ),
            (
                "un nombre raro en theme",
                Box::new(|t| fs::write(t.join("theme/a\nb.css"), "x").unwrap()),
            ),
            (
                "un fichero enorme en theme",
                Box::new(|t| {
                    fs::write(t.join("theme/enorme.css"), vec![b'x'; 5 * 1024 * 1024]).unwrap()
                }),
            ),
        ];
        for (i, (que, estropear)) in casos.iter().enumerate() {
            let malo = temporal.path().join(format!("tema-malo-{i}"));
            copiar_arbol(&tema_de_ejemplo(), &malo);
            estropear(&malo);
            let mut o = opciones_con_arbol(&carpeta, &salida);
            o.tema = malo;
            let mensaje = fallo(preparar_manual(&o));
            assert!(!mensaje.is_empty(), "{que}");
            // Y la salida anterior sigue como estaba.
            assert_eq!(arbol_de(&salida), con_tema, "{que}");
        }
    }
    // Un tema que no existe.
    let mut o = opciones_con_arbol(&carpeta, &salida);
    o.tema = temporal.path().join("no-hay");
    assert!(preparar_manual(&o).is_err());
    assert_eq!(arbol_de(&salida), con_tema);
}

#[test]
fn el_tema_tiene_topes_de_ficheros_de_niveles_y_de_tamano_total() {
    let temporal = tempfile::tempdir().unwrap();
    let carpeta = copia_del_libro(temporal.path());
    let salida = temporal.path().join("libro");
    let con_tema_malo = |nombre: &str, estropear: &dyn Fn(&Path)| {
        let tema = temporal.path().join(nombre);
        copiar_arbol(&tema_de_ejemplo(), &tema);
        estropear(&tema);
        let mut o = opciones_con_arbol(&carpeta, &salida);
        o.tema = tema;
        fallo(preparar_manual(&o))
    };

    // Demasiados ficheros.
    let mensaje = con_tema_malo("muchos", &|t| {
        for i in 0..260 {
            fs::write(t.join(format!("theme/f{i}.css")), "x").unwrap();
        }
    });
    assert!(mensaje.contains("256"), "{mensaje}");

    // Demasiados niveles.
    let mensaje = con_tema_malo("hondo", &|t| {
        let mut ruta = t.join("theme");
        for i in 0..9 {
            ruta = ruta.join(format!("n{i}"));
        }
        fs::create_dir_all(&ruta).unwrap();
        fs::write(ruta.join("f.css"), "x").unwrap();
    });
    assert!(mensaje.contains("niveles"), "{mensaje}");

    // Cada fichero cabe, pero todos juntos no.
    let mensaje = con_tema_malo("pesado", &|t| {
        for i in 0..9 {
            fs::write(
                t.join(format!("theme/g{i}.bin")),
                vec![b'x'; 4 * 1024 * 1024],
            )
            .unwrap();
        }
    });
    assert!(mensaje.contains("tope"), "{mensaje}");

    // Un `book.toml` enorme.
    let mensaje = con_tema_malo("book-grande", &|t| {
        fs::write(t.join("book.toml"), vec![b'#'; 2 * 1024 * 1024]).unwrap();
    });
    assert!(mensaje.contains("tope"), "{mensaje}");
    assert!(!salida.exists());
}

#[test]
fn el_tema_real_de_la_web_si_existe_se_acepta() {
    // `web/manual/` lo crea el tema (S3-P3B): si ya está, tiene que servir.
    let tema = raiz().join("web/manual");
    if !tema.join("book.toml").is_file() {
        eprintln!("aviso: aún no hay web/manual/book.toml");
        return;
    }
    let temporal = tempfile::tempdir().unwrap();
    let mut o = opciones_con_arbol(&libro_de_ejemplo(), &temporal.path().join("libro"));
    o.tema = tema;
    preparar_manual(&o).unwrap();
}

// ---------------------------------------------------------------------------
// mdBook de verdad (solo con MAGI_WEB_MDBOOK)
// ---------------------------------------------------------------------------

#[test]
fn el_libro_preparado_lo_acepta_mdbook_y_los_enlaces_funcionan() {
    let Some(mdbook) = std::env::var_os("MAGI_WEB_MDBOOK").map(PathBuf::from) else {
        // Con MAGI_EXIGIR_MDBOOK=1 (como MAGI_EXIGIR_DEB en la prueba del paquete de
        // Termux) la ausencia de mdBook es un fallo, no una omisión.
        assert!(
            std::env::var_os("MAGI_EXIGIR_MDBOOK").is_none(),
            "MAGI_EXIGIR_MDBOOK está puesta y falta MAGI_WEB_MDBOOK (la ruta de mdbook)"
        );
        eprintln!("aviso: sin MAGI_WEB_MDBOOK, prueba omitida");
        return;
    };
    let temporal = tempfile::tempdir().unwrap();
    let salida = temporal.path().join("libro");
    preparar(&libro_de_ejemplo(), &salida).unwrap();
    let html = temporal.path().join("html");
    let resultado = Command::new(&mdbook)
        .arg("build")
        .arg(&salida)
        .arg("-d")
        .arg(&html)
        .output()
        .expect("no se pudo lanzar mdbook");
    assert!(
        resultado.status.success(),
        "mdbook falló: {}",
        String::from_utf8_lossy(&resultado.stderr)
    );
    let pagina = |nombre: &str| -> String {
        let texto = fs::read_to_string(html.join(nombre)).unwrap();
        let ini = texto.find("<main>").expect("<main>");
        let fin = texto.find("</main>").expect("</main>");
        texto[ini..fin].to_string()
    };
    let hrefs = |pagina: &str| -> Vec<String> {
        pagina
            .split("href=\"")
            .skip(1)
            .filter_map(|r| r.split_once('"').map(|(h, _)| h.to_string()))
            .collect()
    };

    let primeros = pagina("primeros-pasos.html");
    // Los ids de GitHub llegan al HTML, también los repetidos y los acentuados.
    for id in [
        "instalación",
        "instalación-1",
        "árbol-y-ñandú",
        "código",
        "enlaces",
    ] {
        assert!(
            primeros.contains(&format!("id=\"{id}\"")),
            "falta el id {id}"
        );
    }
    let enlaces = hrefs(&primeros);
    // Entre capítulos, con su ancla, en el mismo formato.
    assert!(
        enlaces.contains(&"tuneles.html#túnel-local-y-remoto".to_string()),
        "{enlaces:?}"
    );
    assert!(
        enlaces.contains(&"#instalación-1".to_string())
            || enlaces.contains(&"#instalación".to_string())
    );
    // Los de fuera.
    assert!(enlaces.contains(&"../".to_string()), "{enlaces:?}");
    assert!(
        enlaces.contains(&"../en/#install".to_string()),
        "{enlaces:?}"
    );
    assert!(
        enlaces
            .iter()
            .any(|h| h == "https://github.com/HectorEspejo/magi/tree/v1.6.1/packaging/termux/"),
        "{enlaces:?}"
    );
    // `<host>` se ve como texto y no se traga la línea.
    assert!(
        primeros.contains("&lt;host&gt; suelto en la prosa"),
        "{primeros}"
    );
    assert!(primeros.contains("&lt;uuid&gt; aquí"), "{primeros}");
    // El código con `{{servicio}}` y `<host>` sale tal cual.
    assert!(primeros.contains("{{servicio}}"));

    // Y que todo enlace a otra página del libro resuelve: el índice llega a
    // `index.html`, no a `README.html`.
    let tuneles = pagina("tuneles.html");
    let enlaces = hrefs(&tuneles);
    assert!(enlaces.contains(&"index.html".to_string()), "{enlaces:?}");
    assert!(!enlaces.iter().any(|h| h.contains("README")), "{enlaces:?}");
    for pagina_html in [
        "index.html",
        "primeros-pasos.html",
        "tuneles.html",
        "estado.html",
    ] {
        let enlaces = hrefs(&pagina(pagina_html));
        for h in enlaces {
            if h.starts_with("http")
                || h.starts_with("mailto:")
                || h.starts_with('#')
                || h.starts_with("../")
            {
                continue;
            }
            let (ruta, ancla) = h
                .split_once('#')
                .map_or((h.as_str(), None), |(r, a)| (r, Some(a)));
            let ruta = if ruta.is_empty() { pagina_html } else { ruta };
            let destino = fs::read_to_string(html.join(ruta)).unwrap_or_else(|_| {
                panic!("{pagina_html}: el enlace {h} no llega a ningún fichero")
            });
            if let Some(ancla) = ancla {
                assert!(
                    destino.contains(&format!("id=\"{ancla}\"")),
                    "{pagina_html}: el enlace {h} no llega a ningún id"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Basura
// ---------------------------------------------------------------------------

/// Un generador congruencial lineal (Knuth): determinista y sin dependencias.
struct Lcg(u64);

impl Lcg {
    fn siguiente(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn por_debajo_de(&mut self, tope: usize) -> usize {
        (self.siguiente() % tope.max(1) as u64) as usize
    }
}

const TROZOS: &[&str] = &[
    "[",
    "]",
    "(",
    ")",
    "<",
    ">",
    "`",
    "``",
    "```",
    "~~~",
    "\\",
    "#",
    "## ",
    "# ",
    "{#x}",
    "{{#",
    "{{",
    "}}",
    "&amp;",
    "&",
    "!",
    "![",
    "\n",
    "\r\n",
    "\n\n",
    "\"",
    "'",
    "<!--",
    "-->",
    "<!-- x -->",
    "<host>",
    "](",
    "](../x.md)",
    "](../../README.md)",
    "](#a)",
    "[x]: ",
    "[x]: ../../README.md\n",
    "    ",
    "> ",
    "- ",
    "1. ",
    "\t",
    "\0",
    "ñ",
    "🙂",
    "../../",
    "../",
    "https://",
    "mailto:",
    "<https://x.org>",
    "|",
    "|---|",
    ":",
    "[^1]",
    "[^1]: ",
    "---",
    "===",
    "*",
    "_",
    "~",
    "{.c}",
    "{k=v}",
];

fn mutar(texto: &str, azar: &mut Lcg) -> String {
    let mut chars: Vec<char> = texto.chars().collect();
    for _ in 0..1 + azar.por_debajo_de(6) {
        if chars.is_empty() {
            chars.extend("# x\n".chars());
        }
        let pos = azar.por_debajo_de(chars.len() + 1);
        match azar.por_debajo_de(4) {
            0 => {
                let fin = (pos + 1 + azar.por_debajo_de(20)).min(chars.len());
                chars.drain(pos.min(fin)..fin);
            }
            1 | 2 => {
                let trozo = TROZOS[azar.por_debajo_de(TROZOS.len())];
                for (i, c) in trozo.chars().enumerate() {
                    chars.insert((pos + i).min(chars.len()), c);
                }
            }
            _ => chars.truncate(pos),
        }
    }
    chars.into_iter().collect()
}

#[test]
fn ni_summary_ni_los_enlaces_hacen_panico_con_basura() {
    // Capítulos reales y de ejemplo como punto de partida.
    let mut base: Vec<String> = Vec::new();
    for ruta in [
        libro_de_ejemplo().join("primeros-pasos.md"),
        libro_de_ejemplo().join("README.md"),
        raiz().join("docs/manual/tuneles.md"),
        raiz().join("docs/manual/ssh-config.md"),
        raiz().join("docs/manual/README.md"),
        raiz().join("docs/manual/seguridad.md"),
    ] {
        let texto = fs::read_to_string(&ruta).unwrap();
        // Un trozo de cada uno, para que cada iteración sea corta.
        base.push(texto.chars().take(3000).collect());
    }
    let capitulos = nombres(&["primeros-pasos.md", "tuneles.md"]);
    let ficheros = nombres(&["README.md", "estado.md", "primeros-pasos.md", "tuneles.md"]);

    let mut azar = Lcg(0x4D41_4749_3963);
    let (mut anclas_ok, mut enlaces_ok, mut html_ok, mut summary_ok) = (0, 0, 0, 0);
    let (mut anclas_err, mut enlaces_err, mut html_err) = (0, 0, 0);
    for i in 0..1500 {
        let texto = mutar(&base[i % base.len()], &mut azar);

        let _ = rechazar_preprocesadores(&texto, "x.md");

        match con_anclas(&texto, "x.md") {
            Ok(salida) => {
                anclas_ok += 1;
                assert_eq!(
                    ids_de(&salida),
                    slugs_de(&texto).into_iter().map(Some).collect::<Vec<_>>(),
                    "ids distintos con {texto:?}"
                );
            }
            Err(e) => {
                anclas_err += 1;
                assert!(!e.to_string().is_empty());
            }
        }

        match reescribir_enlaces(&texto, "tuneles.md", ETIQUETA, &capitulos, "x.md") {
            Ok(salida) => {
                enlaces_ok += 1;
                for destino in destinos_de(&salida) {
                    // Lo que ya tenía esquema no se toca: sirve cualquiera. Lo demás
                    // tiene que ser de los tipos que espera el libro (o una imagen
                    // del manual).
                    let con_esquema = destino
                        .split(['/', '?', '#'])
                        .next()
                        .is_some_and(|p| p.contains(':'));
                    if con_esquema {
                        assert!(
                            !destino.to_ascii_lowercase().starts_with("javascript:"),
                            "{destino:?} con {texto:?}"
                        );
                        continue;
                    }
                    let es_imagen_local = destino.rsplit_once('.').is_some_and(|(_, e)| {
                        ["png", "jpg", "jpeg", "gif", "webp"]
                            .contains(&e.split(['#', '?']).next().unwrap_or(""))
                    });
                    let es_index = destino == "index.md"
                        || destino.starts_with("index.md#")
                        || destino.starts_with("index.md?");
                    assert!(
                        es_index || es_imagen_local || destino_esperado(&destino).is_ok(),
                        "destino inesperado {destino:?} con {texto:?}"
                    );
                }
            }
            Err(e) => {
                enlaces_err += 1;
                assert!(!e.to_string().is_empty());
            }
        }

        match escapar_html_suelto(&texto, "x.md") {
            Ok(salida) => {
                html_ok += 1;
                assert!(!hay_html(&salida), "queda HTML con {texto:?}");
                assert_eq!(
                    escapar_html_suelto(&salida, "x.md").unwrap(),
                    salida,
                    "no es idempotente con {texto:?}"
                );
            }
            Err(e) => {
                html_err += 1;
                assert!(!e.to_string().is_empty());
            }
        }

        // El índice, con la basura como README.
        if summary(&texto, &ficheros).is_ok() {
            summary_ok += 1;
        }
    }
    // La prueba no es vacía: hay de todo.
    assert!(
        anclas_ok > 50 && anclas_err > 10,
        "{anclas_ok}/{anclas_err}"
    );
    assert!(
        enlaces_ok > 50 && enlaces_err > 10,
        "{enlaces_ok}/{enlaces_err}"
    );
    assert!(html_ok > 50, "{html_ok}/{html_err}");
    assert!(summary_ok > 0, "ni un README con índice entre {summary_ok}");

    // Entradas extremas, una a una.
    let enorme = "[".repeat(5000) + &"](x)".repeat(5000);
    let _ = reescribir_enlaces(&enorme, "tuneles.md", ETIQUETA, &capitulos, "x.md");
    let _ = con_anclas(&"# ".repeat(3000), "x.md");
    let _ = escapar_html_suelto(&"<a>".repeat(5000), "x.md");
    let _ = rechazar_preprocesadores(&"{{".repeat(5000), "x.md");
    let _ = summary(
        &("## Capítulos\n".to_string() + &"- [a](b.md)\n".repeat(2000)),
        &ficheros,
    );
    for rara in [
        "", "\0", "\u{feff}", "\r", "[", "](", "![", "[]()", "[](<", "[x](<a", "[x]:", "<!--",
        "\\", "`", "{{#",
    ] {
        let _ = rechazar_preprocesadores(rara, "x.md");
        let _ = con_anclas(rara, "x.md");
        let _ = reescribir_enlaces(rara, "tuneles.md", ETIQUETA, &capitulos, "x.md");
        let _ = escapar_html_suelto(rara, "x.md");
        let _ = summary(rara, &ficheros);
    }
    // Con un nombre de capítulo raro tampoco hay pánico.
    for rara in ["", "..", "/x", "a/../b.md", "a\0b.md", "a/b.md"] {
        let _ = reescribir_enlaces("[a](../x.md)", rara, ETIQUETA, &capitulos, "x.md");
    }
}

// ---------------------------------------------------------------------------
// Que las pruebas no se vuelvan vacías
// ---------------------------------------------------------------------------

#[test]
fn los_errores_son_de_dato_raro_y_dicen_donde() {
    // Todo lo que falla por un dato del manual es `DatoRaro` con su sitio.
    let errores = [
        con_anclas("## Con id {#mio}\n", "docs/manual/x.md").unwrap_err(),
        enlazar("[a](/x)").unwrap_err(),
        escapar_html_suelto("<!-- sin cerrar", "docs/manual/x.md").unwrap_err(),
        rechazar_preprocesadores("{{#x}}", "docs/manual/x.md").unwrap_err(),
        summary("sin índice", &[]).unwrap_err(),
    ];
    for e in errores {
        match e {
            ErrorWeb::DatoRaro { donde, que } => {
                assert!(!donde.is_empty());
                assert!(!que.is_empty());
            }
            otro => panic!("tenía que ser DatoRaro: {otro}"),
        }
    }
}

/// Revisión ING-01: con `GIT_DIR` (o `GIT_WORK_TREE`…) puesto, `git -C <repo>` mira otro
/// repositorio sin avisar y el manual saldría de otra etiqueta con el mismo nombre. El
/// binario lo quita del entorno antes de lanzar `git`.
#[test]
fn una_variable_git_dir_del_entorno_no_desvia_la_lectura_de_la_etiqueta() {
    if !hay_git() {
        return;
    }
    let (temporal_a, repo_a) = repositorio_con_manual(|_, _| {});
    let (_temporal_b, repo_b) = repositorio_con_manual(|_, manual| {
        let ruta = manual.join("tuneles.md");
        let mut texto = fs::read_to_string(&ruta).unwrap();
        texto.push_str("\nMARCA-DE-B-NO-DE-A\n");
        fs::write(&ruta, texto).unwrap();
    });
    let salida = temporal_a.path().join("libro");
    let resultado = Command::new(env!("CARGO_BIN_EXE_magi-web"))
        .args(["preparar-manual", "--etiqueta", ETIQUETA, "--repositorio"])
        .arg(&repo_a)
        .arg("--tema")
        .arg(tema_de_ejemplo())
        .arg("--salida")
        .arg(&salida)
        .env("GIT_DIR", repo_b.join(".git"))
        .env("GIT_WORK_TREE", &repo_b)
        .output()
        .unwrap();
    assert!(
        resultado.status.success(),
        "{}",
        String::from_utf8_lossy(&resultado.stderr)
    );
    let tuneles = fs::read_to_string(salida.join("src/tuneles.md")).unwrap();
    assert!(
        !tuneles.contains("MARCA-DE-B-NO-DE-A"),
        "se leyó el repositorio B, no el pedido"
    );
}
