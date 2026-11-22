//! El HTML del manual ya generado por mdBook: pie común y lo que choca con la CSP.
//!
//! `construir` pasa por aquí cada página `.html` del manual antes de ponerle su CSP
//! (`csp::insertar(html, true)`):
//!
//! - `<div style="clear: both"></div>` (que mdBook pone en cada página y la CSP
//!   bloquea) pasa a `<div class="limpiar"></div>`; la clase está en el CSS del tema.
//!   Cualquier otro atributo `style` es un error: el manual no lleva ninguno.
//! - El pie común de la web (`parciales/pie.html`) se inserta justo después de
//!   `</main>` (mdBook cierra `</body>` tras sus scripts, fuera de la página).
//!
//! La cabecera común es estática, en `web/manual/theme/header.hbs`, y una prueba
//! comprueba que dice lo mismo que `parciales/cabecera.html`.
//!
//! Todo lo que se decide se lee de los tokens de `html` (nunca con búsquedas de
//! texto sobre el documento entero): un `</main>` o un `<div style…>` escritos
//! dentro de un `<script>`, de un comentario o de un `<title>` no cuentan, y los
//! `<script>` no se tocan nunca. Una página que no se puede leer entera es un error.

use serde_json::{json, Value};

use crate::error::{ErrorWeb, Resultado};
use crate::html::{self, Etiqueta, Token};
use crate::ordenes::texto_seguro;
use crate::paginas::{contexto_comun, Entorno};
use crate::rutas::{self, Idioma, Pagina};

/// Plantilla del pie común, la misma de todas las páginas de la web.
const PLANTILLA_PIE: &str = "parciales/pie.html";

/// Una página cualquiera del manual (están todas en `manual/`): de ella salen los
/// enlaces relativos del pie.
const PAGINA_DEL_MANUAL: &str = "manual/index.html";

/// Lo único que mdBook escribe con un `style`: el salto de flotantes bajo la
/// navegación de capítulos. La CSP (`style-src 'self'`) lo bloquea.
const LIMPIAR_DE_MDBOOK: &str = "<div style=\"clear: both\"></div>";
/// Su sustituto: `.limpiar { clear: both }` está en `web/manual/theme/magi.css`.
const LIMPIAR_PROPIO: &str = "<div class=\"limpiar\"></div>";

/// Longitud máxima de un nombre de página dentro de un mensaje de error.
const MAX_ORIGEN: usize = 200;

/// El pie común de la web para las páginas del manual (`manual/<capitulo>.html`:
/// los enlaces salen relativos desde una carpeta, `../`), ya renderizado y escapado.
pub fn pie_del_manual(e: &Entorno<'_>) -> Resultado<String> {
    // El manual solo existe en castellano: el pie es el de la parte en castellano.
    let mut contexto = contexto_comun(e, Pagina::Portada, Idioma::Es);
    let enlace = |hasta: &str| rutas::relativa(PAGINA_DEL_MANUAL, hasta);
    let Value::Object(mapa) = &mut contexto else {
        return Err(ErrorWeb::dato_raro(
            "pie del manual",
            "el contexto común no es un objeto",
        ));
    };
    mapa.insert(
        "raiz".to_string(),
        json!(rutas::hasta_raiz(PAGINA_DEL_MANUAL)),
    );
    mapa.insert(
        "nav".to_string(),
        json!({
            "inicio": {"href": enlace(rutas::ruta(Pagina::Portada, Idioma::Es))},
            "descargas": {"href": enlace(rutas::ruta(Pagina::Descargas, Idioma::Es))},
            "versiones": {"href": enlace(rutas::ruta(Pagina::Versiones, Idioma::Es))},
            "manual": {"href": enlace(rutas::RUTA_MANUAL)},
        }),
    );

    let pie = e.plantillas.renderizar(PLANTILLA_PIE, &contexto)?;
    validar_pie(&pie)?;
    Ok(pie)
}

/// Adapta una página de mdBook: sustituye el `clear: both`, rechaza cualquier otro
/// `style` y mete `pie` tras `</main>` (exactamente una vez; si falta `</main>` o
/// hay más de uno, es un error que dice cuál y en qué página). `origen` es la ruta
/// de la página y sale en los errores. Idempotente: adaptar dos veces no repite el pie.
pub fn adaptar_pagina(html: &str, pie: &str, origen: &str) -> Resultado<String> {
    let pagina = texto_seguro(origen, MAX_ORIGEN);
    validar_pie(pie)?;
    let pie = pie.trim();

    let tokens = leer(html, &pagina)?;
    let Analisis {
        mut cambios,
        cierres_de_main,
        pies_ya_puestos,
    } = analizar(html, &tokens, &pagina)?;

    let cierre_de_main = match cierres_de_main.as_slice() {
        [uno] => *uno,
        [] => {
            return Err(ErrorWeb::dato_raro(
                pagina,
                "no tiene `</main>`: no se sabe dónde va el pie",
            ));
        }
        varios => {
            return Err(ErrorWeb::dato_raro(
                pagina,
                format!(
                    "tiene {} `</main>` y tiene que tener uno: no se sabe dónde va el pie",
                    varios.len()
                ),
            ));
        }
    };

    // Idempotente: si el pie ya está justo tras `</main>`, no se repite. Otro pie de
    // la web (distinto o en otro sitio) es un error: no se acumulan dos.
    let tras_main = html.get(cierre_de_main.fin..).unwrap_or("");
    let ya_esta = tras_main.trim_start().starts_with(pie);
    if ya_esta {
        if pies_ya_puestos > 1 {
            return Err(ErrorWeb::dato_raro(
                pagina,
                "tiene más de un pie de la web (`<footer class=\"pie\">`)",
            ));
        }
    } else {
        if pies_ya_puestos > 0 {
            return Err(ErrorWeb::dato_raro(
                pagina,
                "ya tiene un pie de la web (`<footer class=\"pie\">`) que no es el que toca \
                 tras `</main>`",
            ));
        }
        cambios.push(Cambio {
            inicio: cierre_de_main.fin,
            fin: cierre_de_main.fin,
            nuevo: format!("\n{pie}"),
        });
    }

    Ok(aplicar(html, cambios))
}

/// Como `adaptar_pagina`, para un trozo de manual que no es una página y no lleva
/// pie (el `toc.html` de la barra lateral sin JavaScript): sustituye el
/// `clear: both` y rechaza cualquier otro `style`, pero no pide `</main>`.
pub fn adaptar_fragmento(html: &str, origen: &str) -> Resultado<String> {
    let pagina = texto_seguro(origen, MAX_ORIGEN);
    let tokens = leer(html, &pagina)?;
    let analisis = analizar(html, &tokens, &pagina)?;
    Ok(aplicar(html, analisis.cambios))
}

/// Lo que `analizar` encuentra en una página.
struct Analisis<'a> {
    /// Los `clear: both` de mdBook, ya con su sustituto.
    cambios: Vec<Cambio>,
    /// Las etiquetas `</main>`.
    cierres_de_main: Vec<&'a Etiqueta>,
    /// Cuántos `<footer class="pie">` lleva ya.
    pies_ya_puestos: usize,
}

fn leer(html: &str, pagina: &str) -> Resultado<Vec<Token>> {
    html::tokenizar_detallado(html).map_err(|e| {
        ErrorWeb::dato_raro(
            format!("{pagina} (línea {})", html::linea_de(html, e.posicion)),
            e.que,
        )
    })
}

/// Recorre los tokens: planea la sustitución del `clear: both`, falla ante cualquier
/// otro `style` y anota dónde cierra `<main>` y si ya hay un pie.
fn analizar<'a>(html: &str, tokens: &'a [Token], pagina: &str) -> Resultado<Analisis<'a>> {
    let mut analisis = Analisis {
        cambios: Vec::new(),
        cierres_de_main: Vec::new(),
        pies_ya_puestos: 0,
    };
    for (i, token) in tokens.iter().enumerate() {
        let Token::Etiqueta(etiqueta) = token else {
            continue;
        };
        let donde = || format!("{pagina} (línea {})", html::linea_de(html, etiqueta.inicio));
        if etiqueta.nombre == "style" && !etiqueta.cierre {
            return Err(ErrorWeb::dato_raro(
                donde(),
                "<style> en línea: la CSP (`style-src 'self'`) no lo permite; el estilo va en \
                 una hoja del tema",
            ));
        }
        if etiqueta.tiene_atributo("style") {
            let Some(fin) = limpiar_de_mdbook(html, tokens, i, etiqueta) else {
                return Err(ErrorWeb::dato_raro(
                    donde(),
                    format!(
                        "atributo `style` en <{}>: la CSP (`style-src 'self'`) no lo permite \
                         y el manual no lleva ninguno; solo se sustituye el \
                         `<div style=\"clear: both\"></div>` de mdBook",
                        etiqueta.nombre
                    ),
                ));
            };
            analisis.cambios.push(Cambio {
                inicio: etiqueta.inicio,
                fin,
                nuevo: LIMPIAR_PROPIO.to_string(),
            });
        }
        if etiqueta.nombre == "main" && etiqueta.cierre {
            analisis.cierres_de_main.push(etiqueta);
        }
        if etiqueta.nombre == "footer" && !etiqueta.cierre && es_pie_de_la_web(etiqueta) {
            analisis.pies_ya_puestos += 1;
        }
    }
    Ok(analisis)
}

/// Un trozo del documento que se sustituye (`inicio == fin` es una inserción).
struct Cambio {
    inicio: usize,
    fin: usize,
    nuevo: String,
}

/// Aplica los cambios en una sola pasada, de adelante atrás, copiando lo que hay
/// entre uno y otro: el coste es lineal aunque una página traiga decenas de miles de
/// cambios (sustituir sobre el `String` entero mueve su cola en cada uno, y es
/// cuadrático). A igual comienzo va antes la inserción (`inicio == fin`) que el que
/// sustituye algo: así la inserción acaba delante.
fn aplicar(html: &str, mut cambios: Vec<Cambio>) -> String {
    cambios.sort_by(|a, b| a.inicio.cmp(&b.inicio).then(a.fin.cmp(&b.fin)));
    let mut salida =
        String::with_capacity(html.len() + cambios.iter().map(|c| c.nuevo.len()).sum::<usize>());
    let mut cursor = 0;
    for c in cambios {
        // Las posiciones salen del tokenizador y caen siempre en un límite de
        // carácter, ordenadas y sin solaparse; si no, el cambio se descarta antes
        // que cortar un carácter o pisar otro cambio.
        let valido = c.inicio >= cursor
            && c.inicio <= c.fin
            && html.is_char_boundary(c.inicio)
            && html.is_char_boundary(c.fin);
        if !valido {
            continue;
        }
        salida.push_str(&html[cursor..c.inicio]);
        salida.push_str(&c.nuevo);
        cursor = c.fin;
    }
    salida.push_str(&html[cursor..]);
    salida
}

/// Si la etiqueta `tokens[i]` es el `<div style="clear: both"></div>` de mdBook,
/// exactamente así escrito y con su cierre pegado, devuelve dónde acaba el cierre.
fn limpiar_de_mdbook(html: &str, tokens: &[Token], i: usize, apertura: &Etiqueta) -> Option<usize> {
    if apertura.nombre != "div" || apertura.cierre || apertura.autocierre {
        return None;
    }
    let Some(Token::Etiqueta(cierre)) = tokens.get(i + 1) else {
        return None;
    };
    if cierre.nombre != "div" || !cierre.cierre || cierre.inicio != apertura.fin {
        return None;
    }
    (html.get(apertura.inicio..cierre.fin)? == LIMPIAR_DE_MDBOOK).then_some(cierre.fin)
}

/// `true` si es el `<footer>` del pie común de la web: lleva la clase `pie`.
fn es_pie_de_la_web(etiqueta: &Etiqueta) -> bool {
    etiqueta
        .atributo("class")
        .and_then(html::decodificar_atributo)
        .is_some_and(|clases| clases.split_whitespace().any(|c| c == "pie"))
}

/// El pie se mete tal cual en cada página: tiene que ser HTML que se lee entero y
/// que no añade nada que la CSP bloquee ni ejecuta nada.
fn validar_pie(pie: &str) -> Resultado<()> {
    if pie.trim().is_empty() {
        return Err(ErrorWeb::dato_raro("pie del manual", "el pie está vacío"));
    }
    let tokens =
        html::tokenizar(pie).map_err(|e| ErrorWeb::dato_raro("pie del manual", e.to_string()))?;
    for token in &tokens {
        let Token::Etiqueta(e) = token else { continue };
        let ajeno = (e.nombre == "script" || e.nombre == "style") && !e.cierre;
        if ajeno || e.tiene_atributo("style") {
            return Err(ErrorWeb::dato_raro(
                "pie del manual",
                format!(
                    "el pie lleva <{}> o un atributo `style`: no se puede meter en una página \
                     con CSP",
                    e.nombre
                ),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn aplicar_pone_la_insercion_delante_de_lo_que_empieza_en_el_mismo_sitio() {
        let cambios = vec![
            Cambio {
                inicio: 3,
                fin: 3,
                nuevo: "[PIE]".to_string(),
            },
            Cambio {
                inicio: 3,
                fin: 6,
                nuevo: "<x>".to_string(),
            },
            Cambio {
                inicio: 8,
                fin: 9,
                nuevo: "Z".to_string(),
            },
        ];
        assert_eq!(aplicar("abcdefghij", cambios), "abc[PIE]<x>ghZj");
    }

    #[test]
    fn aplicar_da_lo_mismo_que_sustituir_de_atras_adelante() {
        // Referencia lenta y obvia: cada cambio sobre el `String`, del último al primero.
        let referencia = |html: &str, cambios: &[Cambio]| -> String {
            let mut ordenados: Vec<&Cambio> = cambios.iter().collect();
            ordenados.sort_by(|a, b| b.inicio.cmp(&a.inicio).then(b.fin.cmp(&a.fin)));
            let mut salida = html.to_string();
            for c in ordenados {
                salida.replace_range(c.inicio..c.fin, &c.nuevo);
            }
            salida
        };
        // «é» y «ñ» ocupan dos bytes: los cambios caen entre caracteres.
        let html = "aébñc".repeat(300);
        let mut cambios = Vec::new();
        let mut i = 0;
        while i + 4 < html.len() {
            // Una inserción y una sustitución en el mismo sitio, y un hueco.
            cambios.push(Cambio {
                inicio: i,
                fin: i,
                nuevo: format!("[{i}]"),
            });
            cambios.push(Cambio {
                inicio: i,
                fin: i + 1,
                nuevo: "<x>".to_string(),
            });
            i += 7;
        }
        let esperado = referencia(&html, &cambios);
        assert_eq!(aplicar(&html, cambios), esperado);
    }

    #[test]
    fn aplicar_no_pisa_cambios_solapados_ni_invertidos() {
        let cambios = vec![
            Cambio {
                inicio: 2,
                fin: 6,
                nuevo: "A".to_string(),
            },
            Cambio {
                inicio: 4,
                fin: 5,
                nuevo: "B".to_string(),
            },
            Cambio {
                inicio: 8,
                fin: 7,
                nuevo: "C".to_string(),
            },
            Cambio {
                inicio: 9,
                fin: 99,
                nuevo: "D".to_string(),
            },
        ];
        // Vale el primero; el que cae dentro, el invertido y el que se pasa se descartan.
        assert_eq!(aplicar("abcdefghij", cambios), "abAghij");
    }

    #[test]
    fn aplicar_tiene_coste_lineal() {
        // Regresión: sustituir sobre el `String` entero mueve su cola en cada cambio y es
        // cuadrático; con 200 000 cambios sobre ~6 MB eran minutos, y ahora milisegundos.
        let n = 200_000;
        let largo = LIMPIAR_DE_MDBOOK.len();
        let html = LIMPIAR_DE_MDBOOK.repeat(n);
        let cambios: Vec<Cambio> = (0..n)
            .map(|i| Cambio {
                inicio: i * largo,
                fin: (i + 1) * largo,
                nuevo: LIMPIAR_PROPIO.to_string(),
            })
            .collect();
        let empezo = std::time::Instant::now();
        let salida = aplicar(&html, cambios);
        assert!(
            empezo.elapsed() < std::time::Duration::from_secs(5),
            "aplicar tarda {:?}",
            empezo.elapsed()
        );
        assert!(
            salida == LIMPIAR_PROPIO.repeat(n),
            "el resultado no es el esperado"
        );
    }

    #[test]
    fn aplicar_no_corta_un_caracter_por_la_mitad() {
        // «é» ocupa dos bytes: un cambio que cae dentro se descarta, no entra en pánico.
        let cambios = vec![Cambio {
            inicio: 1,
            fin: 2,
            nuevo: "X".to_string(),
        }];
        assert_eq!(aplicar("aébc", cambios), "aébc");
    }

    #[test]
    fn solo_el_clear_both_exacto_y_con_su_cierre_pegado_se_sustituye() {
        let comprobar = |html: &str| {
            let tokens = html::tokenizar(html).unwrap();
            let Some(Token::Etiqueta(apertura)) = tokens.first() else {
                panic!("sin etiqueta");
            };
            limpiar_de_mdbook(html, &tokens, 0, apertura)
        };
        let bueno = "<div style=\"clear: both\"></div>";
        assert_eq!(comprobar(bueno), Some(bueno.len()));
        for malo in [
            "<div style=\"clear:both\"></div>",
            "<div style=\"clear: both\"> </div>",
            "<div style=\"clear: both\" id=\"a\"></div>",
            "<div style=\"clear: both\"/>",
            "<p style=\"clear: both\"></p>",
        ] {
            assert_eq!(comprobar(malo), None, "{malo}");
        }
    }

    #[test]
    fn el_pie_se_reconoce_por_su_clase() {
        let nombre = |html: &str| match &html::tokenizar(html).unwrap()[0] {
            Token::Etiqueta(e) => es_pie_de_la_web(e),
            _ => false,
        };
        assert!(nombre("<footer class=\"pie\">"));
        assert!(nombre("<footer class=\"a pie\">"));
        assert!(nombre("<footer class=\"&#112;ie\">"));
        assert!(!nombre("<footer class=\"piedra\">"));
        assert!(!nombre("<footer>"));
    }
}
