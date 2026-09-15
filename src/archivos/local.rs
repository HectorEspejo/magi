//! Panel local (F4): listado con `std::fs`, recorrido recursivo para preparar
//! una subida y las operaciones locales (borrar, renombrar, crear directorio).
//!
//! Nunca se sigue un enlace simbólico al listar: se muestra como enlace. Al
//! copiar, un enlace a fichero se copia como el fichero apuntado y un enlace a
//! directorio se omite, porque seguir enlaces puede duplicar árboles enteros.

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use crate::archivos::marcas::{ordenar, Entrada, TipoEntrada};

/// Elemento de una subida preparada por el cliente: su ruta de origen y su
/// ruta relativa al directorio elegido, que es la que se usa en el destino.
#[derive(Debug, Clone, PartialEq)]
pub struct ElementoLocal {
    pub origen: PathBuf,
    pub relativo: PathBuf,
    pub bytes: u64,
    pub es_dir: bool,
}

/// Lista un directorio: directorios primero, luego por nombre. Los enlaces se
/// muestran como enlaces (nunca se siguen).
pub fn listar(ruta: &Path) -> Result<Vec<Entrada>, String> {
    let mut entradas = Vec::new();
    let lectura = fs::read_dir(ruta).map_err(|error| format!("{}: {error}", ruta.display()))?;
    for elemento in lectura {
        let elemento = elemento.map_err(|error| format!("{}: {error}", ruta.display()))?;
        let nombre = elemento.file_name().to_string_lossy().to_string();
        match entrada_de(&elemento.path(), nombre) {
            Ok(entrada) => entradas.push(entrada),
            // Un fichero que desaparece entre el listado y el `stat` no debe
            // tumbar el panel entero.
            Err(_) => continue,
        }
    }
    ordenar(&mut entradas);
    Ok(entradas)
}

fn entrada_de(ruta: &Path, nombre: String) -> Result<Entrada, String> {
    let metadata =
        fs::symlink_metadata(ruta).map_err(|error| format!("{}: {error}", ruta.display()))?;
    let tipo_fichero = metadata.file_type();
    let (tipo, enlace) = if tipo_fichero.is_symlink() {
        let destino = fs::read_link(ruta)
            .ok()
            .map(|destino| destino.to_string_lossy().to_string());
        // Un enlace a directorio se marca como directorio para que la
        // navegación y las marcas lo traten como tal.
        let apunta_a_dir = fs::metadata(ruta).map(|m| m.is_dir()).unwrap_or(false);
        let tipo = if apunta_a_dir {
            TipoEntrada::Directorio
        } else {
            TipoEntrada::Enlace
        };
        (tipo, destino)
    } else if tipo_fichero.is_dir() {
        (TipoEntrada::Directorio, None)
    } else {
        (TipoEntrada::Fichero, None)
    };
    Ok(Entrada {
        nombre,
        tipo,
        tamano: metadata.len(),
        mtime: mtime_de(&metadata),
        permisos: Some(metadata.mode()),
        propietario: propietario_de(&metadata),
        enlace,
        marca: Default::default(),
    })
}

/// Época en segundos del mtime, o cero si el sistema no la da.
pub fn mtime_de(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|fecha| fecha.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duracion| duracion.as_secs() as i64)
        .unwrap_or(0)
}

/// `usuario:grupo` si se pueden resolver los nombres; si no, los números.
fn propietario_de(metadata: &fs::Metadata) -> Option<String> {
    let usuario = nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(metadata.uid()))
        .ok()
        .flatten()
        .map(|usuario| usuario.name)
        .unwrap_or_else(|| metadata.uid().to_string());
    let grupo = nix::unistd::Group::from_gid(nix::unistd::Gid::from_raw(metadata.gid()))
        .ok()
        .flatten()
        .map(|grupo| grupo.name)
        .unwrap_or_else(|| metadata.gid().to_string());
    Some(format!("{usuario}:{grupo}"))
}

/// Recorre un directorio entero para preparar una subida: primero los
/// directorios y luego sus ficheros, con la ruta relativa de cada uno. Los
/// enlaces a directorio se omiten y se cuentan en `omitidos`.
pub fn recorrer(raiz: &Path) -> Result<(Vec<ElementoLocal>, usize), String> {
    let mut elementos = Vec::new();
    let mut omitidos = 0usize;
    recorrer_en(raiz, Path::new(""), &mut elementos, &mut omitidos)?;
    Ok((elementos, omitidos))
}

fn recorrer_en(
    directorio: &Path,
    relativo: &Path,
    elementos: &mut Vec<ElementoLocal>,
    omitidos: &mut usize,
) -> Result<(), String> {
    let lectura =
        fs::read_dir(directorio).map_err(|error| format!("{}: {error}", directorio.display()))?;
    let mut hijos: Vec<(PathBuf, String)> = Vec::new();
    for elemento in lectura {
        let elemento = elemento.map_err(|error| format!("{}: {error}", directorio.display()))?;
        hijos.push((
            elemento.path(),
            elemento.file_name().to_string_lossy().to_string(),
        ));
    }
    hijos.sort_by(|uno, otro| uno.1.cmp(&otro.1));

    for (ruta, nombre) in &hijos {
        let destino_relativo = relativo.join(nombre);
        let metadata = match fs::symlink_metadata(ruta) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if metadata.file_type().is_symlink() {
            let apunta_a_dir = fs::metadata(ruta).map(|m| m.is_dir()).unwrap_or(false);
            if apunta_a_dir {
                *omitidos += 1;
                continue;
            }
            elementos.push(ElementoLocal {
                origen: ruta.clone(),
                relativo: destino_relativo,
                bytes: fs::metadata(ruta).map(|m| m.len()).unwrap_or(0),
                es_dir: false,
            });
        } else if metadata.is_dir() {
            elementos.push(ElementoLocal {
                origen: ruta.clone(),
                relativo: destino_relativo.clone(),
                bytes: 0,
                es_dir: true,
            });
            recorrer_en(ruta, &destino_relativo, elementos, omitidos)?;
        } else {
            elementos.push(ElementoLocal {
                origen: ruta.clone(),
                relativo: destino_relativo,
                bytes: metadata.len(),
                es_dir: false,
            });
        }
    }
    Ok(())
}

/// Bytes de un elemento: los de un fichero, la suma de los de un directorio.
pub fn bytes_de(ruta: &Path) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(ruta) else {
        return 0;
    };
    if !metadata.is_dir() {
        return metadata.len();
    }
    match recorrer(ruta) {
        Ok((elementos, _)) => elementos.iter().map(|elemento| elemento.bytes).sum(),
        Err(_) => 0,
    }
}

/// Borra un fichero o un directorio entero. Sin papelera: es lo que avisa el
/// diálogo de confirmación.
pub fn borrar(ruta: &Path, es_dir: bool) -> Result<(), String> {
    let resultado = if es_dir {
        fs::remove_dir_all(ruta)
    } else {
        fs::remove_file(ruta)
    };
    resultado.map_err(|error| format!("{}: {error}", ruta.display()))
}

pub fn renombrar(de: &Path, a: &Path) -> Result<(), String> {
    fs::rename(de, a).map_err(|error| format!("{}: {error}", de.display()))
}

pub fn crear_dir(ruta: &Path) -> Result<(), String> {
    fs::create_dir(ruta).map_err(|error| format!("{}: {error}", ruta.display()))
}

/// Directorio padre, o la propia raíz si no hay padre.
pub fn ruta_padre(ruta: &Path) -> PathBuf {
    match ruta.parent() {
        Some(padre) if !padre.as_os_str().is_empty() => padre.to_path_buf(),
        _ => ruta.to_path_buf(),
    }
}

pub fn es_oculto(nombre: &str) -> bool {
    nombre.starts_with('.') && nombre != "." && nombre != ".."
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_listado_pone_los_directorios_primero_y_omite_lo_ilegible() {
        let temporal = tempfile::tempdir().unwrap();
        std::fs::write(temporal.path().join("zeta.txt"), b"hola").unwrap();
        std::fs::create_dir(temporal.path().join("alfa")).unwrap();
        std::fs::write(temporal.path().join(".oculto"), b"").unwrap();

        let entradas = listar(temporal.path()).unwrap();
        let nombres: Vec<&str> = entradas.iter().map(|e| e.nombre.as_str()).collect();
        assert_eq!(nombres, vec!["alfa", ".oculto", "zeta.txt"]);
        assert_eq!(entradas[0].tipo, TipoEntrada::Directorio);
        assert_eq!(entradas[2].tamano, 4);
    }

    #[test]
    fn el_recorrido_devuelve_directorios_y_ficheros_con_su_ruta_relativa() {
        let temporal = tempfile::tempdir().unwrap();
        let raiz = temporal.path().join("static");
        std::fs::create_dir_all(raiz.join("css")).unwrap();
        std::fs::write(raiz.join("index.html"), b"12345").unwrap();
        std::fs::write(raiz.join("css/app.css"), b"123").unwrap();

        let (elementos, omitidos) = recorrer(&raiz).unwrap();
        assert_eq!(omitidos, 0);
        let relativos: Vec<String> = elementos
            .iter()
            .map(|elemento| elemento.relativo.display().to_string())
            .collect();
        assert_eq!(relativos, vec!["css", "css/app.css", "index.html"]);
        assert!(elementos[0].es_dir);
        assert_eq!(bytes_de(&raiz), 8);
    }

    #[test]
    fn un_enlace_a_directorio_se_omite_al_recorrer() {
        let temporal = tempfile::tempdir().unwrap();
        let raiz = temporal.path().join("raiz");
        std::fs::create_dir_all(raiz.join("real")).unwrap();
        std::fs::write(raiz.join("real/dentro.txt"), b"x").unwrap();
        std::os::unix::fs::symlink(raiz.join("real"), raiz.join("atajo")).unwrap();

        let (elementos, omitidos) = recorrer(&raiz).unwrap();
        assert_eq!(omitidos, 1);
        let relativos: Vec<String> = elementos
            .iter()
            .map(|elemento| elemento.relativo.display().to_string())
            .collect();
        assert_eq!(relativos, vec!["real", "real/dentro.txt"]);
    }

    #[test]
    fn los_ocultos_se_reconocen_por_el_punto() {
        assert!(es_oculto(".env"));
        assert!(es_oculto(".config"));
        assert!(!es_oculto("env"));
        assert!(!es_oculto(".."));
    }
}
