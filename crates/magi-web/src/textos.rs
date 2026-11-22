//! Textos de las páginas: `web/textos/es.toml` y `web/textos/en.toml`.
//!
//! Los dos ficheros tienen exactamente las mismas claves y ninguna vacía. Si una
//! clave solo está en uno, la construcción falla diciendo cuál y en qué fichero.
//! Los textos no llevan HTML: lo que lleva un enlace se parte en `antes`,
//! `enlace` y `despues`, y la plantilla pone el `<a>`.
//!
//! Un texto es una cadena o una tabla de textos: nada de números, listas ni
//! fechas, y las claves solo llevan letras, cifras, `_` y `-` (el punto separa
//! los niveles al aplanarlas: `portada.titulo`).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::error::{ErrorWeb, Resultado};
use crate::rutas::Idioma;

/// Niveles de tablas como mucho (un fichero de textos real llega a 3).
const PROFUNDIDAD_MAXIMA: usize = 8;

#[derive(Debug, Clone)]
pub struct Textos {
    pub(crate) es: toml::Table,
    pub(crate) en: toml::Table,
}

impl Textos {
    /// Lee `es.toml` y `en.toml` de `dir` y los valida.
    pub fn cargar(dir: &Path) -> Resultado<Self> {
        let ruta_es = dir.join("es.toml");
        let ruta_en = dir.join("en.toml");
        let es = fs::read_to_string(&ruta_es).map_err(|e| ErrorWeb::lectura(&ruta_es, e))?;
        let en = fs::read_to_string(&ruta_en).map_err(|e| ErrorWeb::lectura(&ruta_en, e))?;
        Self::desde_textos(
            &es,
            &ruta_es.display().to_string(),
            &en,
            &ruta_en.display().to_string(),
        )
    }

    /// Valida dos textos TOML (`origen_es` y `origen_en` salen en los errores).
    pub fn desde_textos(es: &str, origen_es: &str, en: &str, origen_en: &str) -> Resultado<Self> {
        let tabla_es = leer_tabla(es, origen_es)?;
        let tabla_en = leer_tabla(en, origen_en)?;
        let claves_es = validar_tabla(&tabla_es, origen_es)?;
        let claves_en = validar_tabla(&tabla_en, origen_en)?;

        let mut diferencias = Vec::new();
        for clave in claves_es.difference(&claves_en) {
            diferencias.push(format!(
                "la clave «{clave}» está en {origen_es} y falta en {origen_en}"
            ));
        }
        for clave in claves_en.difference(&claves_es) {
            diferencias.push(format!(
                "la clave «{clave}» está en {origen_en} y falta en {origen_es}"
            ));
        }
        if !diferencias.is_empty() {
            return Err(ErrorWeb::dato_raro(
                format!("textos ({origen_es} y {origen_en})"),
                diferencias.join("; "),
            ));
        }
        Ok(Self {
            es: tabla_es,
            en: tabla_en,
        })
    }

    fn tabla(&self, idioma: Idioma) -> &toml::Table {
        match idioma {
            Idioma::Es => &self.es,
            Idioma::En => &self.en,
        }
    }

    /// Árbol anidado de un idioma, para pasarlo a las plantillas (`t.portada.titulo`).
    pub fn arbol(&self, idioma: Idioma) -> serde_json::Value {
        tabla_a_json(self.tabla(idioma))
    }

    /// Claves aplanadas con puntos (`portada.titulo`) de un idioma.
    pub fn claves(&self, idioma: Idioma) -> BTreeSet<String> {
        let mut claves = BTreeSet::new();
        recoger_claves(self.tabla(idioma), "", &mut claves);
        claves
    }

    /// Un texto por su clave aplanada.
    pub fn texto(&self, idioma: Idioma, clave: &str) -> Option<&str> {
        let mut tabla = self.tabla(idioma);
        let mut tramos = clave.split('.').peekable();
        while let Some(tramo) = tramos.next() {
            let valor = tabla.get(tramo)?;
            if tramos.peek().is_none() {
                return valor.as_str();
            }
            tabla = valor.as_table()?;
        }
        None
    }
}

fn leer_tabla(texto: &str, origen: &str) -> Resultado<toml::Table> {
    toml::from_str::<toml::Table>(texto)
        .map_err(|e| ErrorWeb::dato_raro(origen, format!("no es un TOML válido: {e}")))
}

/// Una clave válida: letras ASCII, cifras, `_` y `-`.
fn clave_valida(clave: &str) -> bool {
    !clave.is_empty()
        && clave
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Comprueba la tabla entera (claves, tipos, vacíos) y devuelve sus claves aplanadas.
fn validar_tabla(tabla: &toml::Table, origen: &str) -> Resultado<BTreeSet<String>> {
    let mut textos = BTreeMap::new();
    aplanar(tabla, "", origen, 0, &mut textos)?;
    Ok(textos.into_keys().collect())
}

fn aplanar(
    tabla: &toml::Table,
    prefijo: &str,
    origen: &str,
    profundidad: usize,
    salida: &mut BTreeMap<String, String>,
) -> Resultado<()> {
    if profundidad > PROFUNDIDAD_MAXIMA {
        return Err(ErrorWeb::dato_raro(
            origen,
            format!(
                "demasiados niveles de tablas bajo «{}»",
                prefijo.trim_end_matches('.')
            ),
        ));
    }
    for (clave, valor) in tabla {
        if !clave_valida(clave) {
            return Err(ErrorWeb::dato_raro(
                origen,
                format!("la clave «{prefijo}{clave}» no es válida: solo letras, cifras, «_» y «-»"),
            ));
        }
        let completa = format!("{prefijo}{clave}");
        match valor {
            toml::Value::String(texto) => {
                if texto.trim().is_empty() {
                    return Err(ErrorWeb::dato_raro(
                        origen,
                        format!("la clave «{completa}» está vacía"),
                    ));
                }
                salida.insert(completa, texto.clone());
            }
            toml::Value::Table(subtabla) => {
                aplanar(
                    subtabla,
                    &format!("{completa}."),
                    origen,
                    profundidad + 1,
                    salida,
                )?;
            }
            otro => {
                return Err(ErrorWeb::dato_raro(
                    origen,
                    format!(
                        "la clave «{completa}» es de tipo {} y solo se admiten textos y tablas",
                        otro.type_str()
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// Claves de una tabla ya validada (sin volver a comprobar nada).
fn recoger_claves(tabla: &toml::Table, prefijo: &str, claves: &mut BTreeSet<String>) {
    for (clave, valor) in tabla {
        let completa = format!("{prefijo}{clave}");
        match valor {
            toml::Value::Table(subtabla) => {
                recoger_claves(subtabla, &format!("{completa}."), claves);
            }
            toml::Value::String(_) => {
                claves.insert(completa);
            }
            _ => {}
        }
    }
}

fn tabla_a_json(tabla: &toml::Table) -> serde_json::Value {
    let mut mapa = serde_json::Map::new();
    for (clave, valor) in tabla {
        match valor {
            toml::Value::String(texto) => {
                mapa.insert(clave.clone(), serde_json::Value::String(texto.clone()));
            }
            toml::Value::Table(subtabla) => {
                mapa.insert(clave.clone(), tabla_a_json(subtabla));
            }
            _ => {}
        }
    }
    serde_json::Value::Object(mapa)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    const ES: &str = "[a]\nb = \"uno\"\n[a.c]\nd = \"dos\"\n";
    const EN: &str = "[a]\nb = \"one\"\n[a.c]\nd = \"two\"\n";

    #[test]
    fn aplana_y_busca_por_clave() {
        let t = Textos::desde_textos(ES, "es.toml", EN, "en.toml").unwrap();
        assert_eq!(t.texto(Idioma::Es, "a.b"), Some("uno"));
        assert_eq!(t.texto(Idioma::En, "a.c.d"), Some("two"));
        assert_eq!(t.texto(Idioma::Es, "a"), None, "una tabla no es un texto");
        assert_eq!(t.texto(Idioma::Es, "a.x"), None);
        assert_eq!(t.texto(Idioma::Es, "a.b.z"), None);
        assert_eq!(t.texto(Idioma::Es, ""), None);
        let claves: Vec<String> = t.claves(Idioma::Es).into_iter().collect();
        assert_eq!(claves, ["a.b", "a.c.d"]);
    }

    #[test]
    fn el_arbol_es_anidado() {
        let t = Textos::desde_textos(ES, "es.toml", EN, "en.toml").unwrap();
        assert_eq!(
            t.arbol(Idioma::En),
            serde_json::json!({"a": {"b": "one", "c": {"d": "two"}}})
        );
    }

    #[test]
    fn una_clave_de_mas_dice_cual_y_en_que_fichero() {
        let en_de_menos = "[a]\nb = \"one\"\n";
        let error = Textos::desde_textos(ES, "es.toml", en_de_menos, "en.toml")
            .unwrap_err()
            .to_string();
        assert!(error.contains("a.c.d"), "{error}");
        assert!(error.contains("está en es.toml"), "{error}");
        assert!(error.contains("falta en en.toml"), "{error}");
    }

    #[test]
    fn rechaza_claves_vacias_tipos_raros_y_claves_con_punto() {
        let vacia = Textos::desde_textos("a = \"  \"", "es.toml", "a = \"x\"", "en.toml");
        assert!(vacia.unwrap_err().to_string().contains("«a» está vacía"));
        let numero = Textos::desde_textos("a = 3", "es.toml", "a = 3", "en.toml");
        assert!(numero.unwrap_err().to_string().contains("tipo integer"));
        let lista = Textos::desde_textos("a = [\"x\"]", "es.toml", "a = [\"x\"]", "en.toml");
        assert!(lista.is_err());
        let punto =
            Textos::desde_textos("\"a.b\" = \"x\"", "es.toml", "\"a.b\" = \"x\"", "en.toml");
        assert!(punto.unwrap_err().to_string().contains("no es válida"));
        let roto = Textos::desde_textos("a = ", "es.toml", "a = \"x\"", "en.toml");
        assert!(roto.unwrap_err().to_string().contains("es.toml"));
    }
}
