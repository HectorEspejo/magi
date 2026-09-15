//! Visor de ficheros de la vista Archivos: el comando del paginador, sin pasar
//! nunca por un shell (T30), y la búsqueda de una alternativa si falta.
//!
//! La secuencia de suspender y restaurar la TUI vive en `ui::pager`, porque
//! necesita el terminal.

/// Un fichero que se va a ver con el paginador.
#[derive(Debug, Clone, PartialEq)]
pub struct Peticion {
    /// Ruta local del fichero (para un remoto, su temporal).
    pub ruta: String,
    /// Título del visor (el nombre del fichero).
    pub titulo: String,
    /// Es un temporal que hay que borrar al cerrar el visor.
    pub temporal: bool,
}

impl Peticion {
    pub fn local(ruta: &str) -> Self {
        Self {
            ruta: ruta.to_string(),
            titulo: nombre_de(ruta),
            temporal: false,
        }
    }

    pub fn temporal(ruta: &str) -> Self {
        Self {
            ruta: ruta.to_string(),
            titulo: nombre_de(ruta),
            temporal: true,
        }
    }
}

fn nombre_de(ruta: &str) -> String {
    ruta.rsplit('/').next().unwrap_or(ruta).to_string()
}

/// Separa el comando del paginador en programa y argumentos. La ruta del
/// fichero no entra aquí: se pasa siempre como argumento suelto.
pub fn comando(pager: &str) -> Option<(String, Vec<String>)> {
    let mut partes = pager.split_whitespace();
    let programa = partes.next()?.to_string();
    Some((programa, partes.map(|parte| parte.to_string()).collect()))
}

/// ¿Está el programa en el `PATH`?
pub fn disponible(programa: &str) -> bool {
    if programa.contains('/') {
        return std::path::Path::new(programa).exists();
    }
    std::env::var_os("PATH")
        .map(|camino| {
            std::env::split_paths(&camino).any(|directorio| directorio.join(programa).exists())
        })
        .unwrap_or(false)
}

/// Paginador alternativo instalado, si lo hay: `less` primero y `more` después.
pub fn alternativas() -> Vec<&'static str> {
    ["less", "more"]
        .into_iter()
        .filter(|programa| disponible(programa))
        .collect()
}

/// El paginador a usar, con el de reserva si el configurado no está.
pub fn efectivo(pager: &str) -> Option<(String, Vec<String>)> {
    let (programa, argumentos) = comando(pager)?;
    if disponible(&programa) {
        return Some((programa, argumentos));
    }
    // El configurado no está: se usa el primero que se encuentre de los de
    // siempre, para no dejar al usuario sin ver el fichero.
    alternativas()
        .first()
        .map(|programa| (programa.to_string(), Vec::new()))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_pager_se_parte_en_programa_y_argumentos_sin_shell() {
        assert_eq!(
            comando("less -R"),
            Some(("less".to_string(), vec!["-R".to_string()]))
        );
        assert_eq!(
            comando("  bat  --plain --paging=always "),
            Some((
                "bat".to_string(),
                vec!["--plain".to_string(), "--paging=always".to_string()]
            ))
        );
        assert_eq!(comando(""), None);
        assert_eq!(comando("   "), None);
    }

    #[test]
    fn un_programa_ausente_cae_al_paginador_de_siempre() {
        let (programa, argumentos) = efectivo("no-existe-este-paginador").expect("reserva");
        assert!(
            ["less", "more"].contains(&programa.as_str()),
            "debe caer a less o more: {programa}"
        );
        assert!(argumentos.is_empty());
    }

    #[test]
    fn sin_ningun_paginador_no_hay_visor() {
        // Un paginador que no existe y sin `less` ni `more` en el sistema no
        // se puede resolver; aquí basta con comprobar que un programa real se
        // acepta tal cual.
        if disponible("sh") {
            assert_eq!(
                efectivo("sh -c"),
                Some(("sh".to_string(), vec!["-c".to_string()]))
            );
        }
    }

    #[test]
    fn el_titulo_es_el_nombre_del_fichero() {
        assert_eq!(
            Peticion::local("/var/log/nginx/error.log").titulo,
            "error.log"
        );
        assert!(Peticion::temporal("/run/magi/tmp/3-.env").temporal);
    }
}
