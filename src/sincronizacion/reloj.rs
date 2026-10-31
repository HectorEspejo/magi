//! Reloj híbrido, versiones y vectores (puro, sin base de datos).
//!
//! Una versión es "<ms con 13 cifras>.<uuid del equipo>". El vector de un
//! elemento es un mapa {uuid de equipo: ms con la última edición vista}. Con
//! las dos cosas, la fusión distingue una edición que deriva de otra (el
//! vector domina) de dos hechas a la vez (concurrentes).

use std::cmp::Ordering;
use std::collections::BTreeMap;

/// Formatea una versión.
pub fn formatear_version(ms: i64, equipo: &str) -> String {
    format!("{ms:013}.{equipo}")
}

/// Milisegundos de una versión; None si no tiene la forma esperada.
pub fn ms_de_version(version: &str) -> Option<i64> {
    let (ms, uuid) = version.split_once('.')?;
    if ms.is_empty() || uuid.is_empty() || !ms.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    ms.parse().ok()
}

/// UUID del equipo que hizo la versión.
pub fn uuid_de_version(version: &str) -> Option<&str> {
    version.split_once('.').map(|(_, uuid)| uuid)
}

/// Orden total y determinista entre versiones: primero los milisegundos y,
/// si empatan, el uuid del equipo. Una versión mal formada queda por debajo.
pub fn comparar(a: &str, b: &str) -> Ordering {
    match (ms_de_version(a), ms_de_version(b)) {
        (Some(ms_a), Some(ms_b)) => ms_a.cmp(&ms_b).then_with(|| a.cmp(b)),
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
        (None, None) => a.cmp(b),
    }
}

/// La versión mayor de las dos.
pub fn mayor<'a>(a: &'a str, b: &'a str) -> &'a str {
    if comparar(a, b) == Ordering::Less {
        b
    } else {
        a
    }
}

/// ¿El vector a ha visto todo lo que ha visto el b? (dominancia).
pub fn domina(a: &BTreeMap<String, i64>, b: &BTreeMap<String, i64>) -> bool {
    b.iter()
        .all(|(equipo, ms)| a.get(equipo).copied().unwrap_or(0) >= *ms)
}

/// ¿Son concurrentes (ninguno domina al otro)?
pub fn concurrentes(a: &BTreeMap<String, i64>, b: &BTreeMap<String, i64>) -> bool {
    !domina(a, b) && !domina(b, a)
}

/// Máximo componente a componente de dos vectores.
pub fn fusionar_vectores(
    a: &BTreeMap<String, i64>,
    b: &BTreeMap<String, i64>,
) -> BTreeMap<String, i64> {
    let mut fusionado = a.clone();
    for (equipo, ms) in b {
        let entrada = fusionado.entry(equipo.clone()).or_insert(*ms);
        *entrada = (*entrada).max(*ms);
    }
    fusionado
}

/// Máximo componente a componente de varios vectores.
pub fn vector_max<'a>(
    vectores: impl IntoIterator<Item = &'a BTreeMap<String, i64>>,
) -> BTreeMap<String, i64> {
    let mut maximo = BTreeMap::new();
    for vector in vectores {
        maximo = fusionar_vectores(&maximo, vector);
    }
    maximo
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn vector(pares: &[(&str, i64)]) -> BTreeMap<String, i64> {
        pares
            .iter()
            .map(|(equipo, ms)| (equipo.to_string(), *ms))
            .collect()
    }

    #[test]
    fn las_versiones_ordenan_por_milisegundos_y_desempatan_por_equipo() {
        let a = formatear_version(100, "aaaa");
        let b = formatear_version(100, "bbbb");
        let c = formatear_version(101, "aaaa");
        assert_eq!(comparar(&a, &b), Ordering::Less);
        assert_eq!(comparar(&b, &a), Ordering::Greater);
        assert_eq!(comparar(&a, &c), Ordering::Less);
        assert_eq!(mayor(&a, &c), c);
        assert_eq!(ms_de_version(&a), Some(100));
        assert_eq!(uuid_de_version(&a), Some("aaaa"));
    }

    #[test]
    fn la_dominancia_es_componente_a_componente() {
        let base = vector(&[("a", 10)]);
        let despues = vector(&[("a", 10), ("b", 20)]);
        let otro = vector(&[("a", 11)]);
        assert!(domina(&despues, &base));
        assert!(!domina(&base, &despues));
        assert!(concurrentes(&despues, &otro));
        assert!(!concurrentes(&despues, &base));
        assert_eq!(
            fusionar_vectores(&despues, &otro),
            vector(&[("a", 11), ("b", 20)])
        );
    }

    #[test]
    fn un_vector_vacio_es_dominado_por_cualquiera() {
        let vacio = BTreeMap::new();
        let lleno = vector(&[("a", 1)]);
        assert!(domina(&lleno, &vacio));
        assert!(domina(&vacio, &vacio));
        assert!(!concurrentes(&vacio, &lleno));
    }
}
