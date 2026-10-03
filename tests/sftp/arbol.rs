//! Fase 8 · `ListarArbol` por bloques, con exclusiones y `.magiignore`.

#[allow(unused_imports)]
use super::*;

use std::os::unix::fs::MetadataExt as _;

use magi::protocolo::EntradaArbol;

/// Un `Arbol` tal como llega.
struct Bloque {
    entradas: Vec<EntradaArbol>,
    magiignore: Option<String>,
    excluidos: u32,
    fin: bool,
}

/// Pide el árbol y recoge sus bloques hasta `fin` (o el `Error` de la
/// petición).
async fn arbol(
    montaje: &mut Montaje,
    ruta: &Path,
    exclusiones: &[&str],
    usar_magiignore: bool,
    exclusiones_extra: &[&str],
    peticion_id: u64,
) -> Result<Vec<Bloque>, String> {
    let host_id = montaje.host_id;
    let texto =
        |lista: &[&str]| -> Vec<String> { lista.iter().map(|patron| patron.to_string()).collect() };
    montaje
        .enviar(&MensajeCliente::ListarArbol {
            peticion_id,
            host_id,
            ruta: ruta.display().to_string(),
            exclusiones: texto(exclusiones),
            usar_magiignore,
            exclusiones_extra: texto(exclusiones_extra),
        })
        .await;
    let mut bloques = Vec::new();
    loop {
        match montaje
            .esperar(|mensaje| match mensaje {
                MensajeServidor::Arbol {
                    peticion_id: id, ..
                } => *id == peticion_id,
                MensajeServidor::Error {
                    peticion_id: id, ..
                } => *id == Some(peticion_id),
                _ => false,
            })
            .await
        {
            MensajeServidor::Arbol {
                entradas,
                magiignore,
                excluidos,
                fin,
                ..
            } => {
                bloques.push(Bloque {
                    entradas,
                    magiignore,
                    excluidos,
                    fin,
                });
                if fin {
                    return Ok(bloques);
                }
            }
            MensajeServidor::Error { mensaje, .. } => return Err(mensaje),
            otro => panic!("respuesta inesperada: {otro:?}"),
        }
    }
}

fn rutas_de(bloques: &[Bloque]) -> Vec<String> {
    let mut rutas: Vec<String> = bloques
        .iter()
        .flat_map(|bloque| bloque.entradas.iter().map(|entrada| entrada.ruta.clone()))
        .collect();
    rutas.sort();
    rutas
}

fn entrada<'a>(bloques: &'a [Bloque], ruta: &str) -> &'a EntradaArbol {
    bloques
        .iter()
        .flat_map(|bloque| bloque.entradas.iter())
        .find(|entrada| entrada.ruta == ruta)
        .unwrap_or_else(|| panic!("{ruta} no está en el árbol"))
}

/// Más de 1000 entradas: varios bloques, ninguno de más de 1000, `fin` solo
/// en el último y las rutas relativas con `/`. Sin pasar por `AbrirSftp`: el
/// árbol abre el canal si hace falta.
#[tokio::test]
async fn un_arbol_grande_llega_en_bloques_de_mil() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let raiz = montaje.remoto().join("web");
    for directorio in ["a", "b/c"] {
        std::fs::create_dir_all(raiz.join(directorio)).unwrap();
        for indice in 0..700 {
            std::fs::write(raiz.join(directorio).join(format!("f{indice}.txt")), b"x").unwrap();
        }
    }

    let bloques = arbol(&mut montaje, &raiz, &[], false, &[], 1)
        .await
        .expect("el árbol se lista");
    assert!(bloques.len() >= 2, "{} bloques", bloques.len());
    assert!(bloques.iter().all(|bloque| bloque.entradas.len() <= 1000));
    let finales: Vec<bool> = bloques.iter().map(|bloque| bloque.fin).collect();
    assert_eq!(finales.iter().filter(|fin| **fin).count(), 1);
    assert!(finales.last().copied().unwrap(), "fin solo en el último");
    assert!(bloques.iter().all(|bloque| bloque.magiignore.is_none()));

    let rutas = rutas_de(&bloques);
    // a, b, b/c y los 1400 ficheros, cada uno una sola vez.
    assert_eq!(rutas.len(), 1403, "{} entradas", rutas.len());
    let mut sin_repetir = rutas.clone();
    sin_repetir.dedup();
    assert_eq!(sin_repetir.len(), rutas.len(), "ninguna entrada se repite");
    assert!(rutas.contains(&"b/c/f699.txt".to_string()));
    assert_eq!(entrada(&bloques, "b/c").tipo, TipoEntrada::Directorio);
    let fichero = entrada(&bloques, "a/f0.txt");
    assert_eq!(fichero.tipo, TipoEntrada::Fichero);
    assert_eq!(fichero.tamano, 1);
    assert!(fichero.mtime > 1_600_000_000);
    assert!(fichero.permisos.is_some());
    let metadatos = std::fs::metadata(raiz.join("a/f0.txt")).unwrap();
    let propietario = fichero.propietario.clone().expect("con propietario");
    assert_eq!(propietario.uid, Some(metadatos.uid()));
    assert!(
        propietario.usuario.is_some() || propietario.uid.is_some(),
        "el nombre, si el host lo resuelve; si no, el número"
    );
}

/// Lo excluido se poda (un directorio excluido no se recorre) y se cuenta en
/// el último bloque.
#[tokio::test]
async fn las_exclusiones_se_podan_y_se_cuentan() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let raiz = montaje.remoto().join("proyecto");
    std::fs::create_dir_all(raiz.join(".git/objects")).unwrap();
    std::fs::write(raiz.join(".git/config"), b"x").unwrap();
    std::fs::write(raiz.join(".git/objects/aa"), b"x").unwrap();
    std::fs::create_dir_all(raiz.join("src/node_modules/react")).unwrap();
    std::fs::write(raiz.join("src/node_modules/react/index.js"), b"x").unwrap();
    std::fs::write(raiz.join("src/main.rs"), b"fn main() {}").unwrap();
    std::fs::write(raiz.join("error.log"), b"x").unwrap();
    std::fs::write(raiz.join("src/debug.log"), b"x").unwrap();

    let bloques = arbol(
        &mut montaje,
        &raiz,
        &[".git/", "node_modules/", "*.log"],
        false,
        &[],
        2,
    )
    .await
    .expect("el árbol se lista");
    assert_eq!(bloques.len(), 1, "cabe en uno");
    assert!(bloques[0].fin);
    assert_eq!(rutas_de(&bloques), vec!["src", "src/main.rs"]);
    // .git, src/node_modules, error.log y src/debug.log: lo de dentro de un
    // directorio excluido ni se mira.
    assert_eq!(bloques[0].excluidos, 4);
}

/// El `.magiignore` de la raíz se devuelve en el primer bloque y se aplica
/// entre las exclusiones recibidas y las extras: puede reincluir lo que
/// excluye la configuración, pero no lo que excluye una extra.
#[tokio::test]
async fn el_magiignore_se_devuelve_y_se_aplica_en_su_orden() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let raiz = montaje.remoto().join("sitio");
    std::fs::create_dir_all(&raiz).unwrap();
    let texto = "# secretos\nsecreto.txt\n!importante.log\n!vital.log\n";
    std::fs::write(raiz.join(".magiignore"), texto).unwrap();
    std::fs::write(raiz.join("secreto.txt"), b"x").unwrap();
    std::fs::write(raiz.join("importante.log"), b"x").unwrap();
    std::fs::write(raiz.join("vital.log"), b"x").unwrap();
    std::fs::write(raiz.join("ruido.log"), b"x").unwrap();
    std::fs::write(raiz.join("index.html"), b"x").unwrap();

    let exclusiones = [".magiignore", "*.log"];
    let bloques = arbol(&mut montaje, &raiz, &exclusiones, true, &["vital.log"], 3)
        .await
        .expect("el árbol se lista");
    assert_eq!(bloques[0].magiignore.as_deref(), Some(texto));
    assert_eq!(rutas_de(&bloques), vec!["importante.log", "index.html"]);
    // .magiignore, secreto.txt, ruido.log y vital.log.
    assert_eq!(bloques.last().unwrap().excluidos, 4);

    // Sin pedirlo, el .magiignore ni se lee ni se aplica.
    let bloques = arbol(&mut montaje, &raiz, &exclusiones, false, &[], 4)
        .await
        .expect("el árbol se lista");
    assert_eq!(bloques[0].magiignore, None);
    assert_eq!(rutas_de(&bloques), vec!["index.html", "secreto.txt"]);
}

/// Un enlace a directorio llega marcado y no se recorre; uno a fichero lleva
/// los datos de su destino; uno roto, los suyos.
#[tokio::test]
async fn los_enlaces_llegan_marcados_y_no_se_siguen() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let remoto = montaje.remoto();
    let raiz = remoto.join("sitio");
    let comun = remoto.join("comun");
    std::fs::create_dir_all(&raiz).unwrap();
    std::fs::create_dir_all(&comun).unwrap();
    std::fs::write(comun.join("dentro.txt"), b"x").unwrap();
    std::fs::write(remoto.join("datos.bin"), b"doce bytes!!").unwrap();
    std::os::unix::fs::symlink(&comun, raiz.join("compartido")).unwrap();
    std::os::unix::fs::symlink(remoto.join("datos.bin"), raiz.join("datos")).unwrap();
    std::os::unix::fs::symlink(remoto.join("no-esta"), raiz.join("roto")).unwrap();

    let bloques = arbol(&mut montaje, &raiz, &[], false, &[], 5)
        .await
        .expect("el árbol se lista");
    assert_eq!(rutas_de(&bloques), vec!["compartido", "datos", "roto"]);
    let compartido = entrada(&bloques, "compartido");
    assert_eq!(compartido.tipo, TipoEntrada::Enlace);
    assert!(compartido.enlace_a_dir, "el plan lo omitirá");
    let datos = entrada(&bloques, "datos");
    assert_eq!(datos.tipo, TipoEntrada::Enlace);
    assert!(!datos.enlace_a_dir);
    assert_eq!(datos.tamano, 12, "el tamaño es el de su destino");
    let roto = entrada(&bloques, "roto");
    assert_eq!(roto.tipo, TipoEntrada::Enlace);
    assert!(!roto.enlace_a_dir);
}

#[tokio::test]
async fn una_raiz_que_no_existe_o_no_es_un_directorio_es_un_error() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let remoto = montaje.remoto();
    std::fs::write(remoto.join("fichero.txt"), b"x").unwrap();

    let error = arbol(&mut montaje, &remoto.join("no-existe"), &[], false, &[], 6)
        .await
        .err()
        .expect("una raíz que no existe no se lista");
    assert_eq!(error, "la ruta no existe");

    let error = arbol(
        &mut montaje,
        &remoto.join("fichero.txt"),
        &[],
        false,
        &[],
        7,
    )
    .await
    .err()
    .expect("un fichero no es un árbol");
    assert!(error.contains("no es un directorio"), "{error}");
}

/// Un subdirectorio que no se puede leer aborta el recorrido: un plan con
/// un trozo del destino sin ver podría borrar o pisar lo que no se vio.
#[tokio::test]
async fn un_subdirectorio_ilegible_aborta_el_arbol() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    // Como root se puede leer todo: la prueba no tendría sentido.
    if nix::unistd::geteuid().is_root() {
        return;
    }
    let raiz = montaje.remoto().join("sitio");
    let cerrado = raiz.join("cerrado");
    std::fs::create_dir_all(&cerrado).unwrap();
    std::fs::write(cerrado.join("a.txt"), b"x").unwrap();
    std::fs::set_permissions(&cerrado, std::fs::Permissions::from_mode(0o000)).unwrap();

    let resultado = arbol(&mut montaje, &raiz, &[], false, &[], 8).await;
    std::fs::set_permissions(&cerrado, std::fs::Permissions::from_mode(0o755)).unwrap();
    let error = resultado.err().expect("el árbol no se da por bueno");
    assert!(error.contains("permiso denegado"), "{error}");
    assert!(error.contains("cerrado"), "dice cuál: {error}");
}
