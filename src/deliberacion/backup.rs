//! BALTHASAR-2 · backup: el fichero más reciente del directorio de backups
//! del host que casa con el patrón tiene menos de `backup_horas`. Se mira por
//! SFTP (`ListarDir`), sin ejecutar nada en el host.

use crate::archivos::marcas::{Entrada, TipoEntrada};

/// Compila el patrón (glob sobre el nombre del fichero).
pub fn compilar_patron(patron: &str) -> Result<globset::GlobMatcher, String> {
    globset::Glob::new(patron)
        .map(|glob| glob.compile_matcher())
        .map_err(|error| format!("patrón no válido: {error}"))
}

/// Juicio sobre un listado: aprueba si el fichero más reciente que casa con
/// el patrón es más nuevo que `horas`. El texto siempre dice por qué.
pub fn evaluar(
    entradas: &[Entrada],
    patron: Option<&str>,
    ahora: i64,
    horas: u64,
) -> (bool, String) {
    let casa = match patron {
        Some(patron) => match compilar_patron(patron) {
            Ok(patron) => Some(patron),
            Err(motivo) => return (false, motivo),
        },
        None => None,
    };
    if entradas.is_empty() {
        return (false, "directorio vacío".to_string());
    }
    let ficheros: Vec<&Entrada> = entradas
        .iter()
        .filter(|entrada| entrada.tipo == TipoEntrada::Fichero)
        .collect();
    if ficheros.is_empty() {
        return (false, "sin ficheros en el directorio".to_string());
    }
    let candidatos: Vec<&&Entrada> = ficheros
        .iter()
        .filter(|entrada| {
            casa.as_ref()
                .is_none_or(|patron| patron.is_match(&entrada.nombre))
        })
        .collect();
    let Some(ultimo) = candidatos
        .iter()
        .filter(|entrada| entrada.mtime > 0)
        .max_by_key(|entrada| entrada.mtime)
    else {
        return (
            false,
            match patron {
                Some(patron) => format!("ningún fichero casa con {patron}"),
                None => "ningún fichero con fecha".to_string(),
            },
        );
    };
    let edad = (ahora - ultimo.mtime).max(0) as u64;
    let nombre = crate::snippets::salida::sanear_linea(&ultimo.nombre, 40);
    if edad < horas.saturating_mul(3600) {
        (true, format!("hace {} ({nombre})", edad_legible(edad)))
    } else {
        (
            false,
            format!("último backup hace {} ({nombre})", edad_legible(edad)),
        )
    }
}

/// «25 min», «3 h», «4 d».
fn edad_legible(segundos: u64) -> String {
    if segundos < 3600 {
        format!("{} min", segundos / 60)
    } else if segundos < 48 * 3600 {
        format!("{} h", segundos / 3600)
    } else {
        format!("{} d", segundos / 86_400)
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    const AHORA: i64 = 1_800_000_000;

    fn fichero(nombre: &str, hace_horas: i64) -> Entrada {
        Entrada {
            nombre: nombre.to_string(),
            tipo: TipoEntrada::Fichero,
            tamano: 10,
            mtime: AHORA - hace_horas * 3600,
            permisos: None,
            propietario: None,
            enlace: None,
            marca: Default::default(),
        }
    }

    #[test]
    fn reciente_aprueba_con_la_edad() {
        let entradas = vec![fichero("db-1.sql.gz", 30), fichero("db-2.sql.gz", 3)];
        let (aprueba, texto) = evaluar(&entradas, Some("*.sql.gz"), AHORA, 24);
        assert!(aprueba);
        assert_eq!(texto, "hace 3 h (db-2.sql.gz)");
    }

    #[test]
    fn viejo_rechaza_con_el_dato() {
        let entradas = vec![fichero("db.sql.gz", 31)];
        let (aprueba, texto) = evaluar(&entradas, Some("*.sql.gz"), AHORA, 24);
        assert!(!aprueba);
        assert_eq!(texto, "último backup hace 31 h (db.sql.gz)");
    }

    #[test]
    fn el_patron_manda() {
        let entradas = vec![fichero("notas.txt", 1), fichero("db.sql.gz", 40)];
        let (aprueba, _) = evaluar(&entradas, Some("*.sql.gz"), AHORA, 24);
        assert!(!aprueba);
        let (sin_patron, _) = evaluar(&entradas, None, AHORA, 24);
        assert!(sin_patron);
    }

    #[test]
    fn vacio_sin_coincidencias_o_solo_directorios_rechazan() {
        assert_eq!(evaluar(&[], None, AHORA, 24).1, "directorio vacío");
        let (aprueba, texto) = evaluar(&[fichero("a.txt", 1)], Some("*.gz"), AHORA, 24);
        assert!(!aprueba);
        assert_eq!(texto, "ningún fichero casa con *.gz");
        let mut directorio = fichero("sub", 1);
        directorio.tipo = TipoEntrada::Directorio;
        assert!(!evaluar(&[directorio], None, AHORA, 24).0);
        let mut sin_fecha = fichero("x.gz", 1);
        sin_fecha.mtime = 0;
        assert!(!evaluar(&[sin_fecha], None, AHORA, 24).0);
    }

    #[test]
    fn un_patron_invalido_rechaza_con_motivo() {
        let (aprueba, texto) = evaluar(&[fichero("a", 1)], Some("["), AHORA, 24);
        assert!(!aprueba);
        assert!(texto.starts_with("patrón no válido"));
    }
}
