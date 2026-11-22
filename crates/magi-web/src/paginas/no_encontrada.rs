//! Página 404, en castellano e inglés, con enlaces absolutos a `https://magi.erratia.com/`.
//!
//! Es un solo fichero (`404.html`) para los dos idiomas: sale de cualquier ruta
//! que no existe, así que ni sus enlaces ni su hoja de estilos pueden ser
//! relativos. El texto principal va en castellano y el inglés lleva `lang="en"`.

use serde_json::json;

use crate::error::Resultado;
use crate::paginas::{ampliar, contexto_comun, Entorno, DOMINIO};
use crate::rutas::{self, Idioma, Pagina};

pub fn renderizar(e: &Entorno<'_>) -> Resultado<String> {
    let mut contexto = contexto_comun(e, Pagina::NoEncontrada, Idioma::Es);
    ampliar(
        &mut contexto,
        "d",
        json!({
            "t_en": e.textos.arbol(Idioma::En),
            "inicio_es": format!("{DOMINIO}{}", rutas::ruta(Pagina::Portada, Idioma::Es)),
            "inicio_en": format!("{DOMINIO}{}", rutas::ruta(Pagina::Portada, Idioma::En)),
        }),
    );
    e.plantillas.renderizar("404.html", &contexto)
}
