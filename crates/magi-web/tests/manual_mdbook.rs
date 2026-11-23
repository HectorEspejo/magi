//! El tema de `web/manual/` con mdBook de verdad: construye el libro de prueba de
//! `tests/fijos/libro/` con `web/manual/book.toml` y `web/manual/theme/` y mira lo que
//! sale (informe §6.4, §7.10 y §8).
//!
//! Necesita mdBook 0.5.4 (la versión para la que está hecho el tema, que depende de
//! sus ids y de su estructura): el de `MAGI_WEB_MDBOOK` o, si no está, el `mdbook` del
//! `PATH`, y solo si dice exactamente `mdbook v0.5.4`. Sin él, las pruebas se
//! omiten con un aviso (solo se ve con `--nocapture`) salvo con
//! `MAGI_EXIGIR_MDBOOK=1`, que las hace fallar. El job `web` de `ci.yml` las ejecuta
//! con esa variable y el mdBook fijado.
//!
//! El libro de prueba se construye una sola vez y lo comparten las pruebas; la última
//! construye el manual real (`docs/manual/`) y exige un sitio sin hallazgos.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use magi_web::comprobar::{comprobar_sitio, ListaPermitida};
use magi_web::construir::{construir, OpcionesConstruir};
use magi_web::csp;
use magi_web::datos::Plataformas;
use magi_web::enlaces::{clasificar, Destino};
use magi_web::html::{tokenizar, Token};
use magi_web::manual::{preparar_manual, OpcionesManual, OrigenManual};
use magi_web::manual_html::{adaptar_fragmento, adaptar_pagina, pie_del_manual};
use magi_web::ordenes::ejecutar_con_plazo;
use magi_web::paginas::Entorno;
use magi_web::plantillas::Plantillas;
use magi_web::textos::Textos;
use sha2::{Digest, Sha256};

const VERSION_DE_MDBOOK: &str = "mdbook v0.5.4";

fn raiz() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("raíz del repositorio")
}

fn web() -> PathBuf {
    raiz().join("web")
}

fn fijos() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fijos")
}

fn leer(ruta: &Path) -> String {
    fs::read_to_string(ruta).unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", ruta.display()))
}

/// El `mdbook` que se puede usar, o por qué no hay ninguno.
fn buscar_mdbook() -> Result<String, String> {
    let candidato = match std::env::var("MAGI_WEB_MDBOOK") {
        Ok(ruta) if !ruta.is_empty() => ruta,
        _ => "mdbook".to_string(),
    };
    let salida = ejecutar_con_plazo(
        &candidato,
        &["--version"],
        &[],
        Duration::from_secs(20),
        4096,
    )
    .map_err(|e| format!("no hay un mdbook utilizable ({candidato}): {e}"))?;
    let dice = String::from_utf8_lossy(&salida.salida).trim().to_string();
    if salida.exito() && dice == VERSION_DE_MDBOOK {
        Ok(candidato)
    } else {
        Err(format!(
            "{candidato} dice «{dice}» y el tema es para «{VERSION_DE_MDBOOK}»"
        ))
    }
}

/// El libro de prueba, ya construido.
struct Libro {
    /// Carpeta del libro (la mantiene viva mientras corran las pruebas).
    _raiz: tempfile::TempDir,
    /// `book/`: lo que escribe mdBook.
    salida: PathBuf,
}

fn copiar(origen: &Path, destino: &Path) {
    fs::create_dir_all(destino).unwrap();
    for e in fs::read_dir(origen).unwrap() {
        let e = e.unwrap();
        let hasta = destino.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copiar(&e.path(), &hasta);
        } else {
            fs::copy(e.path(), hasta).unwrap();
        }
    }
}

fn construir_libro(mdbook: &str) -> Libro {
    let t = tempfile::tempdir().unwrap();
    let carpeta = t.path().join("libro");
    fs::create_dir_all(&carpeta).unwrap();
    // Lo que copia `magi-web preparar-manual`: `book.toml` y `theme/` del tema, y `src/`.
    fs::copy(web().join("manual/book.toml"), carpeta.join("book.toml")).unwrap();
    copiar(&web().join("manual/theme"), &carpeta.join("theme"));
    copiar(&fijos().join("libro/src"), &carpeta.join("src"));
    let ruta = carpeta.display().to_string();
    let salida = ejecutar_con_plazo(
        mdbook,
        &["build", &ruta],
        &[],
        Duration::from_secs(120),
        1024 * 1024,
    )
    .expect("mdbook build");
    assert!(
        salida.exito(),
        "mdbook build falló: {}{}",
        String::from_utf8_lossy(&salida.salida),
        String::from_utf8_lossy(&salida.error)
    );
    // Un aviso de mdBook sobre el libro (HTML sin cerrar, un enlace…) es un fallo del
    // libro de prueba o del tema.
    let avisos = String::from_utf8_lossy(&salida.error);
    assert!(!avisos.contains("WARN"), "mdbook avisa: {avisos}");
    Libro {
        salida: carpeta.join("book"),
        _raiz: t,
    }
}

/// El libro compartido, o `None` si no hay mdBook (y la prueba se omite con su aviso).
fn libro() -> Option<&'static Libro> {
    static LIBRO: OnceLock<Option<Libro>> = OnceLock::new();
    LIBRO
        .get_or_init(|| match buscar_mdbook() {
            Ok(mdbook) => Some(construir_libro(&mdbook)),
            Err(motivo) => {
                assert!(
                    std::env::var("MAGI_EXIGIR_MDBOOK").as_deref() != Ok("1"),
                    "MAGI_EXIGIR_MDBOOK=1 y {motivo}"
                );
                eprintln!("AVISO: se omiten las pruebas de mdBook: {motivo}");
                None
            }
        })
        .as_ref()
}

fn paginas(libro: &Libro) -> Vec<(String, String)> {
    let mut salida = Vec::new();
    for e in fs::read_dir(&libro.salida).unwrap() {
        let ruta = e.unwrap().path();
        if ruta.extension().is_some_and(|x| x == "html") {
            let nombre = ruta.file_name().unwrap().to_string_lossy().into_owned();
            salida.push((nombre, leer(&ruta)));
        }
    }
    salida.sort();
    salida
}

/// Todos los ficheros de `dir` (recursivo) con su ruta relativa.
fn ficheros(dir: &Path) -> Vec<String> {
    fn rec(base: &Path, dir: &Path, salida: &mut Vec<String>) {
        for e in fs::read_dir(dir).unwrap() {
            let ruta = e.unwrap().path();
            if ruta.is_dir() {
                rec(base, &ruta, salida);
            } else {
                salida.push(ruta.strip_prefix(base).unwrap().display().to_string());
            }
        }
    }
    let mut salida = Vec::new();
    rec(dir, dir, &mut salida);
    salida.sort();
    salida
}

/// El fichero de `libro.salida` que empieza por `prefijo` y acaba en `sufijo`
/// (mdBook les pone el hash del contenido: `css/variables-1a2b3c4d.css`).
fn con_hash(libro: &Libro, prefijo: &str, sufijo: &str) -> PathBuf {
    let encontrados: Vec<String> = ficheros(&libro.salida)
        .into_iter()
        .filter(|f| f.starts_with(prefijo) && f.ends_with(sufijo))
        .collect();
    assert_eq!(encontrados.len(), 1, "{prefijo}*{sufijo}: {encontrados:?}");
    libro.salida.join(&encontrados[0])
}

struct Mundo {
    plantillas: Plantillas,
    textos: Textos,
    plataformas: Plataformas,
}

fn mundo() -> Mundo {
    Mundo {
        plantillas: Plantillas::cargar(&web().join("plantillas")).expect("plantillas"),
        textos: Textos::cargar(&web().join("textos")).expect("textos"),
        plataformas: Plataformas::cargar(&web().join("datos/plataformas.toml")).expect("datos"),
    }
}

impl Mundo {
    fn pie(&self) -> String {
        let entorno = Entorno {
            plantillas: &self.plantillas,
            textos: &self.textos,
            plataformas: &self.plataformas,
            version_manual: "1.6.1",
        };
        pie_del_manual(&entorno).expect("pie del manual")
    }
}

/// Las páginas del libro tal como las deja `construir`: adaptadas y con su CSP.
fn paginas_finales(libro: &Libro) -> Vec<(String, String)> {
    let pie = mundo().pie();
    paginas(libro)
        .into_iter()
        .map(|(nombre, html)| {
            let ruta = format!("manual/{nombre}");
            let adaptada = if nombre == "toc.html" {
                adaptar_fragmento(&html, &ruta).unwrap()
            } else {
                adaptar_pagina(&html, &pie, &ruta).unwrap()
            };
            (nombre, csp::insertar(&adaptada, true).unwrap())
        })
        .collect()
}

fn etiquetas(html: &str) -> Vec<magi_web::html::Etiqueta> {
    tokenizar(html)
        .unwrap()
        .into_iter()
        .filter_map(|t| match t {
            Token::Etiqueta(e) => Some(e),
            _ => None,
        })
        .collect()
}

fn cuenta(texto: &str, trozo: &str) -> usize {
    texto.matches(trozo).count()
}

const CAPITULOS: [&str; 5] = [
    "instalacion.html",
    "tuneles.html",
    "app-android.html",
    "termux.html",
    "estado.html",
];

// ---------------------------------------------------------------------------

#[test]
fn construir_el_libro_de_prueba_con_el_tema() {
    let Some(libro) = libro() else { return };
    let lista = ficheros(&libro.salida);
    for pagina in CAPITULOS.iter().chain(["index.html", "toc.html"].iter()) {
        assert!(
            lista.iter().any(|f| f == pagina),
            "falta {pagina}: {lista:?}"
        );
    }
    // Sin la página de impresión ni el 404 (lleva `<base>`), por `book.toml`.
    assert!(
        !lista.iter().any(|f| f == "print.html" || f == "404.html"),
        "{lista:?}"
    );
    // Sin el botón de copiar ni el de ejecutar: ningún `clip-button`, ninguna zona de pruebas.
    for (nombre, html) in paginas(libro) {
        assert!(
            !html.contains("clip-button") && !html.contains("playground"),
            "{nombre}"
        );
    }

    // El tema es el nuestro: las variables, la hoja adicional y las dos de la web.
    let variables = leer(&con_hash(libro, "css/variables-", ".css"));
    assert!(variables.contains("--fondo: #F4F0E6") && variables.contains(".coal"));
    assert!(variables.contains("--bg: var(--fondo)"));
    let magi = leer(&con_hash(libro, "theme/magi-", ".css"));
    assert!(magi.contains(".limpiar") && magi.contains("#mdbook-theme-toggle"));

    let tuneles = leer(&libro.salida.join("tuneles.html"));
    assert!(tuneles.contains("<html lang=\"es\""), "idioma del libro");
    assert!(tuneles.contains("<title>Túneles - Manual de MAGI</title>"));
    assert!(
        tuneles.contains("href=\"../favicon.svg\""),
        "el favicon de la web"
    );
    assert!(
        tuneles.contains("href=\"../estilos/web.css\""),
        "la hoja común de la web"
    );
    // Las hojas de la web llegan antes que las de mdBook y la del tema, la última.
    let web_css = tuneles.find("../estilos/web.css").unwrap();
    let variables_css = tuneles.find("css/variables-").unwrap();
    let general = tuneles.find("css/general-").unwrap();
    let tema = tuneles.find("theme/magi-").unwrap();
    assert!(web_css < variables_css && variables_css < general && general < tema);

    // La búsqueda de mdBook está activa: índice, script y buscador.
    assert!(lista.iter().any(|f| f.starts_with("searchindex-")));
    assert!(lista.iter().any(|f| f.starts_with("searcher-")));
    assert!(lista.iter().any(|f| f.starts_with("elasticlunr")));
    assert!(tuneles.contains("id=\"mdbook-search-toggle\"") && tuneles.contains("type=\"search\""));
    // La barra lateral de mdBook también (el índice de capítulos).
    assert!(lista
        .iter()
        .any(|f| f.starts_with("toc-") && f.ends_with(".js")));
    // El código se resalta con el JavaScript de mdBook: sale como bloque con su lenguaje.
    let instalacion = leer(&libro.salida.join("instalacion.html"));
    assert!(instalacion.contains("class=\"language-toml\""));
    // Una tabla ancha va dentro de su caja, que es la que se desplaza.
    assert!(tuneles.contains("<div class=\"table-wrapper\">"));
    // Los capítulos de los que cuelgan la portada y las descargas están.
    let indice = leer(&libro.salida.join("index.html"));
    for capitulo in ["instalacion", "tuneles", "app-android", "termux", "estado"] {
        assert!(
            indice.contains(&format!("href=\"{capitulo}.html\"")),
            "{capitulo}"
        );
    }
}

#[test]
fn el_html_del_libro_no_tiene_scripts_ni_estilos_ajenos() {
    let Some(libro) = libro() else { return };
    // Antes de adaptar, lo único que choca es el `clear: both` de mdBook, en cada página.
    for (nombre, html) in paginas(libro) {
        if nombre == "toc.html" {
            continue;
        }
        let con_style: Vec<String> = etiquetas(&html)
            .iter()
            .filter(|e| e.tiene_atributo("style"))
            .map(|e| html[e.inicio..e.fin].to_string())
            .collect();
        assert_eq!(
            con_style,
            ["<div style=\"clear: both\">"],
            "{nombre}: lo único con `style` tiene que ser el `clear: both`"
        );
    }

    let finales = paginas_finales(libro);
    assert_eq!(finales.len(), 7);
    let mut guiones_de_referencia: Option<Vec<String>> = None;
    for (nombre, html) in &finales {
        let etiquetas = etiquetas(html);
        // Ningún `style` (atributo ni elemento) en el resultado final.
        assert!(
            etiquetas
                .iter()
                .all(|e| !e.tiene_atributo("style") && e.nombre != "style"),
            "{nombre}: queda un style"
        );
        assert!(!html.contains("clear: both"), "{nombre}");
        // Ningún manejador `on…=`, ningún `<base>`, ningún refresco.
        for e in &etiquetas {
            assert!(
                e.atributos.iter().all(|(a, _)| !a.starts_with("on")),
                "{nombre}: manejador de eventos en <{}>",
                e.nombre
            );
            assert!(
                e.nombre != "base" && e.nombre != "object" && e.nombre != "embed",
                "{nombre}"
            );
        }

        // Los scripts en línea: los mismos, idénticos, en todas las páginas (con
        // `path_to_root` vacío), y sin nada que cargue o ejecute lo que no está en la página.
        let en_linea = csp::scripts_en_linea(html).unwrap();
        if nombre == "toc.html" {
            assert!(en_linea.is_empty(), "toc.html no lleva scripts");
        } else {
            assert_eq!(
                en_linea.len(),
                5,
                "{nombre}: los scripts en línea de mdBook 0.5.4"
            );
            match &guiones_de_referencia {
                None => guiones_de_referencia = Some(en_linea.clone()),
                Some(referencia) => assert_eq!(&en_linea, referencia, "{nombre}"),
            }
            for guion in &en_linea {
                for prohibido in [
                    "eval(",
                    "new Function",
                    "document.write",
                    "fetch(",
                    "XMLHttpRequest",
                    "import(",
                    "http:",
                    "https:",
                    "//",
                ] {
                    // `//` solo vale como comentario de una línea, nunca como URL.
                    if prohibido == "//" {
                        assert!(
                            !guion.contains("://"),
                            "{nombre}: una URL en un script en línea"
                        );
                        continue;
                    }
                    assert!(
                        !guion.contains(prohibido),
                        "{nombre}: «{prohibido}» en un script"
                    );
                }
            }
        }

        // Los `<script src>` son ficheros del propio libro, en su carpeta.
        for e in etiquetas
            .iter()
            .filter(|e| e.nombre == "script" && !e.cierre)
        {
            if let Some(origen) = e.atributo("src") {
                assert!(
                    !origen.contains('/')
                        && !origen.contains(':')
                        && libro.salida.join(origen).is_file(),
                    "{nombre}: <script src={origen}> no es un fichero del libro"
                );
            }
        }

        // La CSP, calculada otra vez aquí con `sha2` y `base64` (no con `csp::hash_script`).
        let politica = csp::extraer(html).unwrap().expect("CSP");
        let esperados: BTreeSet<String> = en_linea
            .iter()
            .map(|g| {
                format!(
                    "'sha256-{}'",
                    STANDARD.encode(Sha256::digest(g.replace("\r\n", "\n").as_bytes()))
                )
            })
            .collect();
        let mut script_src = String::from("'self'");
        for h in &esperados {
            script_src.push(' ');
            script_src.push_str(h);
        }
        assert_eq!(
            politica,
            format!(
                "default-src 'none'; img-src 'self'; style-src 'self'; font-src 'self'; \
                 script-src {script_src}; base-uri 'none'; form-action 'none'; frame-src 'self'"
            ),
            "{nombre}"
        );
        assert!(!politica.contains("unsafe-inline") && !politica.contains("unsafe-eval"));
        // Sin `connect-src`: manda `default-src 'none'` y ningún `fetch` sale (el
        // `book.js` de mdBook tiene uno al *playground* de Rust, que aquí no corre).
        assert!(!politica.contains("connect-src"));
    }

    // El visor del libro lo comprueba también `comprobar`, con su propia lectura.
    let t = tempfile::tempdir().unwrap();
    let sitio = t.path().join("sitio");
    construir(&OpcionesConstruir {
        web: web(),
        releases: fijos().join("releases/sin-red.json"),
        manual: libro.salida.clone(),
        animacion: Some(raiz().join("assets/demo/magi.gif")),
        changelog_escritorio: raiz().join("CHANGELOG.md"),
        changelog_app: None,
        salida: sitio.clone(),
        homebrew: false,
        aur: false,
    })
    .expect("construir con el libro");
    // mdBook copia siempre sus fuentes con su licencia: la web las publica juntas.
    let del_sitio = ficheros(&sitio.join("manual/fonts"));
    assert!(
        del_sitio.iter().any(|f| f.ends_with(".woff2")),
        "{del_sitio:?}"
    );
    for licencia in ["OPEN-SANS-LICENSE.txt", "SOURCE-CODE-PRO-LICENSE.txt"] {
        assert!(
            del_sitio.iter().any(|f| f == licencia),
            "falta {licencia} junto a las fuentes: {del_sitio:?}"
        );
    }
    let hallazgos = comprobar_sitio(&sitio, &ListaPermitida::por_defecto()).unwrap();
    assert!(hallazgos.is_empty(), "{hallazgos:#?}");
}

#[test]
fn el_ancla_con_id_propio_funciona() {
    let Some(libro) = libro() else { return };
    let tuneles = leer(&libro.salida.join("tuneles.html"));
    let instalacion = leer(&libro.salida.join("instalacion.html"));

    // El título con `{#ancla-propia}` tiene ese `id` (una sola vez) y el `{#…}` no se ve.
    assert_eq!(cuenta(&tuneles, "id=\"ancla-propia\""), 1);
    assert!(!tuneles.contains("{#"), "el {{#id}} se ve en el texto");
    assert!(!instalacion.contains("{#"));
    let indice_de_busqueda = leer(&con_hash(libro, "searchindex-", ".js"));
    assert!(
        !indice_de_busqueda.contains("{#"),
        "el {{#id}} sale en la búsqueda"
    );

    // `x.md#ancla` pasa a `x.html#ancla` y el `id` existe en la página de destino.
    assert!(instalacion.contains("href=\"tuneles.html#ancla-propia\""));
    assert!(!instalacion.contains("tuneles.md#ancla-propia\""));
    assert!(instalacion.contains("href=\"estado.html#por-plataforma\""));
    assert_eq!(
        cuenta(
            &leer(&libro.salida.join("estado.html")),
            "id=\"por-plataforma\""
        ),
        1
    );
    assert!(tuneles.contains("href=\"instalacion.html#requisitos\""));

    // Todo enlace interno de todo el libro con ancla lleva a un `id` que existe.
    let mut ids: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let todas = paginas(libro);
    for (nombre, html) in &todas {
        let mut de_la_pagina = BTreeSet::new();
        for e in etiquetas(html) {
            if let Some(id) = e.atributo("id") {
                de_la_pagina.insert(id.to_string());
            }
        }
        ids.insert(nombre.clone(), de_la_pagina);
    }
    let mut comprobados = 0;
    for (nombre, html) in &todas {
        for e in etiquetas(html).iter().filter(|e| e.nombre == "a") {
            let Some(href) = e.atributo("href") else {
                continue;
            };
            match clasificar(&format!("manual/{nombre}"), href) {
                Destino::Ancla(a) if !a.is_empty() => {
                    assert!(
                        ids[nombre].contains(&a),
                        "{nombre}: #{a} no existe en su página"
                    );
                    comprobados += 1;
                }
                Destino::Interno {
                    ruta,
                    ancla: Some(a),
                } if ruta.starts_with("manual/") => {
                    let destino = ruta.trim_start_matches("manual/");
                    assert!(
                        ids.get(destino).is_some_and(|i| i.contains(&a)),
                        "{nombre}: {destino}#{a} no existe"
                    );
                    comprobados += 1;
                }
                _ => {}
            }
        }
    }
    assert!(
        comprobados >= 6,
        "pocos enlaces con ancla comprobados: {comprobados}"
    );
}

#[test]
fn sin_selector_de_temas() {
    let Some(libro) = libro() else { return };
    // Los dos ids que oculta el CSS existen en el HTML de mdBook 0.5.4: si mdBook los
    // renombrara, la regla quedaría muerta y esta prueba lo avisa.
    for (nombre, html) in paginas(libro) {
        if nombre == "toc.html" {
            continue;
        }
        assert_eq!(cuenta(&html, "id=\"mdbook-theme-toggle\""), 1, "{nombre}");
        assert_eq!(cuenta(&html, "id=\"mdbook-theme-list\""), 1, "{nombre}");
        // Los cinco temas más «Auto» solo están dentro de la lista oculta.
        let lista_ini = html.find("id=\"mdbook-theme-list\"").unwrap();
        let lista_fin = lista_ini + html[lista_ini..].find("</ul>").unwrap();
        for e in etiquetas(&html)
            .iter()
            .filter(|e| e.nombre == "button" && e.atributo("class") == Some("theme"))
        {
            assert!(
                e.inicio > lista_ini && e.fin < lista_fin,
                "{nombre}: un botón de tema fuera de la lista oculta"
            );
        }
    }
    // La hoja del tema, ya con su hash, los oculta con `display: none !important`.
    let magi = leer(&con_hash(libro, "theme/magi-", ".css"));
    let i = magi.find("#mdbook-theme-toggle").expect("falta la regla");
    let abre = i + magi[i..].find('{').unwrap();
    let cierra = abre + magi[abre..].find('}').unwrap();
    let selectores = &magi[i..abre];
    assert!(selectores.contains("#mdbook-theme-list"), "{selectores}");
    assert!(
        magi[abre + 1..cierra].contains("display: none !important"),
        "{}",
        &magi[abre..cierra]
    );
    // Ninguna regla posterior los vuelve a enseñar.
    let despues = &magi[cierra..];
    assert!(!despues.contains("#mdbook-theme-toggle") && !despues.contains("#mdbook-theme-list"));

    // Y la elección del tema es del sistema: las clases de tema de mdBook que pone su
    // JavaScript son `light` y `coal`, que las variables mapean a la clara y la oscura.
    let tuneles = leer(&libro.salida.join("tuneles.html"));
    assert!(tuneles.contains("const default_light_theme = \"light\";"));
    assert!(tuneles.contains("const default_dark_theme = \"coal\";"));
    assert!(tuneles.contains("(prefers-color-scheme: dark)"));
}

/// Los `url(…)` y `@import` de una hoja de estilos.
fn recursos_de_css(css: &str) -> Vec<String> {
    let mut salida = Vec::new();
    let mut resto = css;
    while let Some(i) = resto.find("url(") {
        let tras = &resto[i + 4..];
        let (cuerpo, siguiente) = match tras.trim_start().chars().next() {
            Some(c @ ('\'' | '"')) => {
                let inicio = tras.find(c).unwrap() + 1;
                let fin = tras[inicio..].find(c).expect("url( sin cerrar") + inicio;
                (&tras[inicio..fin], &tras[fin..])
            }
            _ => {
                let fin = tras.find(')').expect("url( sin cerrar");
                (&tras[..fin], &tras[fin..])
            }
        };
        salida.push(cuerpo.trim().to_string());
        resto = siguiente;
    }
    let mut resto = css;
    while let Some(i) = resto.find("@import") {
        salida.push(format!(
            "@import{}",
            &resto[i + 7..i + 40.min(resto.len() - i)]
        ));
        resto = &resto[i + 7..];
    }
    salida
}

#[test]
fn sin_recursos_externos() {
    let Some(libro) = libro() else { return };
    let lista = ListaPermitida::por_defecto();
    for (nombre, html) in paginas(libro) {
        for e in etiquetas(&html) {
            // Ningún recurso (hoja, script, imagen, marco…) viene de fuera: todo es
            // un fichero relativo del propio libro (o de la web, por `../`).
            let mut destinos: Vec<&str> = Vec::new();
            for atributo in [
                "src",
                "srcset",
                "data",
                "poster",
                "action",
                "formaction",
                "background",
            ] {
                destinos.extend(e.atributo(atributo));
            }
            if e.nombre == "link" || e.nombre == "use" {
                destinos.extend(e.atributo("href"));
            }
            for destino in destinos {
                assert!(
                    !destino.contains(':')
                        && !destino.starts_with("//")
                        && !destino.starts_with('/'),
                    "{nombre}: <{}> carga «{destino}», que no es relativo",
                    e.nombre
                );
            }
            // Los enlaces que salen del libro (contenido, no recursos) solo van a la lista.
            if e.nombre == "a" {
                if let Some(href) = e.atributo("href") {
                    if href.contains("://") {
                        assert!(
                            lista.permite(href),
                            "{nombre}: enlace externo no permitido: {href}"
                        );
                    }
                }
            }
            assert!(
                e.nombre != "base",
                "{nombre}: <base> rompe los enlaces relativos"
            );
            assert!(
                !(e.nombre == "meta"
                    && e.atributo("http-equiv")
                        .is_some_and(|v| v.eq_ignore_ascii_case("refresh"))),
                "{nombre}: refresco"
            );
        }
    }

    // Las hojas del libro: solo `url()` relativos (las fuentes de mdBook, que están en
    // el propio libro) y el `data:` del botón de copiar, que no existe en ninguna página.
    let mut datos = 0;
    for f in ficheros(&libro.salida)
        .iter()
        .filter(|f| f.ends_with(".css"))
    {
        let css = leer(&libro.salida.join(f));
        assert!(!css.contains("@import"), "{f}: @import");
        for recurso in recursos_de_css(&css) {
            if recurso.starts_with("data:") {
                assert!(
                    f.starts_with("css/chrome-"),
                    "{f}: un data: fuera del botón de copiar"
                );
                datos += 1;
                continue;
            }
            assert!(
                !recurso.contains("://") && !recurso.starts_with("//") && !recurso.starts_with('/'),
                "{f}: url({recurso}) no es relativo"
            );
            let carpeta = Path::new(f).parent().unwrap();
            assert!(
                libro.salida.join(carpeta).join(&recurso).is_file(),
                "{f}: url({recurso}) no existe en el libro"
            );
        }
    }
    assert!(datos <= 1, "más de un data: en las hojas");
    // El `data:` queda muerto: nada del HTML lo activa (`clip-button` no sale).
    for (nombre, html) in paginas(libro) {
        assert!(!html.contains("clip-button"), "{nombre}");
    }
}

#[test]
fn la_cabecera_y_el_aviso_estan_en_cada_pagina() {
    let Some(libro) = libro() else { return };
    let m = mundo();
    let aviso = m
        .textos
        .texto(magi_web::rutas::Idioma::Es, "comun.manual_idioma")
        .unwrap();
    assert_eq!(aviso, "Solo en castellano · Spanish only");

    let mut con_cabecera = 0;
    for (nombre, html) in paginas_finales(libro) {
        if nombre == "toc.html" {
            // El relleno de la barra lateral no es una página: ni cabecera ni aviso.
            assert!(!html.contains("class=\"cabecera\"") && !html.contains("aviso-idioma"));
            continue;
        }
        con_cabecera += 1;
        assert_eq!(cuenta(&html, "<header class=\"cabecera\">"), 1, "{nombre}");
        assert_eq!(
            // La parte en inglés lleva su `lang` (WCAG 3.1.2, AA): la página es `lang="es"`.
            cuenta(
                &html,
                &format!(
                    "<p class=\"aviso-idioma\">{}</p>",
                    aviso.replace("Spanish only", "<span lang=\"en\">Spanish only</span>")
                )
            ),
            1,
            "{nombre}: el aviso de idioma"
        );
        assert_eq!(
            cuenta(&html, "class=\"saltar\""),
            1,
            "{nombre}: el enlace para saltar"
        );
        // El menú común, con enlaces relativos que salen de `manual/`, y «Manual» activo.
        for enlace in [
            "<a class=\"marca\" href=\"../\">MAGI</a>",
            "<a href=\"../\">Inicio</a>",
            "<a href=\"../descargas/\">Descargas</a>",
            "<a href=\"../versiones/\">Versiones</a>",
            "<a href=\"./\" hreflang=\"es\" aria-current=\"true\">Manual</a>",
            "<a class=\"idioma\" href=\"../en/\" hreflang=\"en\" lang=\"en\" title=\"Read this page in English\">English</a>",
        ] {
            assert!(html.contains(enlace), "{nombre}: falta {enlace}");
        }
        // La cabecera y el aviso van antes de la barra de menú de mdBook y del contenido.
        let cabecera = html.find("<header class=\"cabecera\">").unwrap();
        let barra = html.find("id=\"mdbook-menu-bar\"").unwrap();
        let principal = html.find("<main>").unwrap();
        assert!(
            cabecera < html.find("aviso-idioma").unwrap()
                && html.find("aviso-idioma").unwrap() < barra
        );
        assert!(barra < principal);
        // El enlace de salto apunta a algo que existe: el contenido de mdBook.
        assert!(
            html.contains("href=\"#mdbook-content\"") && html.contains("id=\"mdbook-content\"")
        );
        // Y el pie, tras `</main>`, una vez.
        assert_eq!(cuenta(&html, "<footer class=\"pie\">"), 1, "{nombre}");
        assert!(html.find("</main>").unwrap() < html.find("<footer class=\"pie\">").unwrap());
    }
    assert_eq!(con_cabecera, 6);
}

/// El manual de verdad: `docs/manual/` del árbol de trabajo, preparado con
/// `preparar_manual`, construido con mdBook y puesto en el sitio con lo demás. El sitio
/// entero tiene que pasar `comprobar` sin un solo hallazgo: enlaces, anclas, dominios,
/// CSP y nada de `style`. Es la prueba de que el manual, los CHANGELOG y las páginas
/// propias se enlazan bien entre sí.
#[test]
fn el_sitio_con_el_manual_real_no_tiene_hallazgos() {
    let mdbook = match buscar_mdbook() {
        Ok(mdbook) => mdbook,
        Err(motivo) => {
            assert!(
                std::env::var("MAGI_EXIGIR_MDBOOK").as_deref() != Ok("1"),
                "MAGI_EXIGIR_MDBOOK=1 y {motivo}"
            );
            eprintln!("AVISO: se omite el sitio con el manual real: {motivo}");
            return;
        }
    };
    let t = tempfile::tempdir().unwrap();
    let libro = t.path().join("libro");
    let informe = preparar_manual(&OpcionesManual {
        origen: OrigenManual::Arbol {
            carpeta: raiz().join("docs/manual"),
        },
        etiqueta: "v1.6.1".to_string(),
        tema: web().join("manual"),
        salida: libro.clone(),
    })
    .expect("preparar el manual real");
    assert!(
        informe.capitulos.len() >= 10,
        "capítulos: {:?}",
        informe.capitulos
    );

    let manual = t.path().join("manual");
    let salida = ejecutar_con_plazo(
        &mdbook,
        &[
            "build",
            &libro.display().to_string(),
            "--dest-dir",
            &manual.display().to_string(),
        ],
        &[],
        Duration::from_secs(180),
        1024 * 1024,
    )
    .expect("mdbook build");
    let avisos = String::from_utf8_lossy(&salida.error).into_owned();
    assert!(salida.exito(), "mdbook build falló: {avisos}");
    // Un aviso de mdBook sobre el manual real (HTML sin cerrar, enlace roto…) es un
    // defecto de `preparar-manual` o del manual.
    assert!(!avisos.contains("WARN"), "mdbook avisa: {avisos}");
    assert!(manual.join("index.html").is_file());

    let sitio = t.path().join("sitio");
    construir(&OpcionesConstruir {
        web: web(),
        releases: fijos().join("releases/sin-red.json"),
        manual,
        animacion: Some(raiz().join("assets/demo/magi.gif")),
        changelog_escritorio: raiz().join("CHANGELOG.md"),
        changelog_app: None,
        salida: sitio.clone(),
        homebrew: false,
        aur: false,
    })
    .expect("construir con el manual real");
    let hallazgos = comprobar_sitio(&sitio, &ListaPermitida::por_defecto()).unwrap();
    assert!(hallazgos.is_empty(), "{hallazgos:#?}");
}
