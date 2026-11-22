//! `magi-web construir`: genera el sitio completo.
//!
//! Flujo (informe §4.1): lee `plataformas.toml`, los textos, las plantillas y
//! `releases.json` (revalidado con `descargas::elegir`), renderiza cada página en
//! los dos idiomas, copia estilos, fuentes, capturas y animación, y aplica la CSP
//! a todo, también al manual ya generado. La comprobación del resultado
//! (`magi-web comprobar`) es un paso aparte, que corre después.
//!
//! Escribe en un directorio hermano y lo renombra al final: un fallo a mitad nunca
//! deja un sitio a medias ni toca el anterior (informe §4.1, «la web publicada
//! sigue siendo la anterior»).

use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use crate::csp;
use crate::datos::Plataformas;
use crate::descargas::{elegir, OpcionesDescargas};
use crate::error::{ErrorWeb, Resultado};
use crate::paginas::portada::Capturas;
use crate::paginas::versiones::DatosVersiones;
use crate::paginas::{self, Entorno};
use crate::plantillas::Plantillas;
use crate::releases::DocumentoReleases;
use crate::rutas::{self, Idioma, Pagina};
use crate::textos::Textos;
use crate::versiones::{entradas_de_changelog, EntradaChangelog, TOPE_CHANGELOG};

/// Tope de un fichero copiado al sitio (32 MiB): lo que se copia es entrada de
/// otra herramienta (mdBook) o del repositorio, pero nunca se confía en su tamaño.
const TOPE_FICHERO: u64 = 32 * 1024 * 1024;
/// Tope de la animación de la portada (4 MiB; `repositorio_publico` exige 2 MiB).
const TOPE_ANIMACION: u64 = 4 * 1024 * 1024;
/// Ficheros y niveles como mucho al copiar un árbol.
const TOPE_FICHEROS: usize = 4096;
const PROFUNDIDAD_MAXIMA: usize = 8;
/// Tope de una página HTML del manual que se lee para ponerle la CSP (16 MiB).
const TOPE_HTML: u64 = 16 * 1024 * 1024;

/// Extensiones que se copian del manual generado por mdBook.
const EXTENSIONES_MANUAL: &[&str] = &[
    "html", "css", "js", "svg", "png", "gif", "jpg", "jpeg", "woff", "woff2", "json", "ico",
];

#[derive(Debug, Clone)]
pub struct OpcionesConstruir {
    /// Carpeta `web/`.
    pub web: PathBuf,
    /// `releases.json`.
    pub releases: PathBuf,
    /// Manual ya generado por mdBook. CONTRATO-S3-MANUAL: opcional solo en
    /// el Sprint 1; al cerrar el Sprint 3 es obligatorio.
    pub manual: Option<PathBuf>,
    /// `assets/demo/magi.gif` (S2).
    pub animacion: Option<PathBuf>,
    /// CHANGELOG del escritorio de la etiqueta de la última versión publicada: lo
    /// lee la página de versiones, y tiene que traer la versión de la release.
    pub changelog_escritorio: PathBuf,
    /// CHANGELOG de la app de su etiqueta. Obligatorio si hay release de la app;
    /// sin ella no se usa y la página dice «aún sin publicar».
    pub changelog_app: Option<PathBuf>,
    /// Carpeta de salida (se sustituye entera).
    pub salida: PathBuf,
    pub homebrew: bool,
    pub aur: bool,
}

#[derive(Debug, Clone, Default)]
pub struct InformeConstruccion {
    /// Páginas generadas, por su ruta dentro del sitio.
    pub paginas: Vec<String>,
    pub version_escritorio: String,
    pub version_termux: Option<String>,
    pub version_app: Option<String>,
    /// Ficheros del manual que no se copiaron por su extensión.
    pub omitidos: Vec<String>,
}

pub fn construir(opciones: &OpcionesConstruir) -> Resultado<InformeConstruccion> {
    let salida = validar_salida(opciones)?;
    let nombre = salida
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| ErrorWeb::dato_raro("--salida", "no tiene nombre de carpeta"))?;
    let padre = salida
        .parent()
        .ok_or_else(|| ErrorWeb::dato_raro("--salida", "no tiene carpeta madre"))?
        .to_path_buf();
    let pid = std::process::id();
    let trabajo = padre.join(format!(".{nombre}.construyendo-{pid}"));
    if trabajo.exists() {
        fs::remove_dir_all(&trabajo).map_err(|e| ErrorWeb::escritura(&trabajo, e))?;
    }
    fs::create_dir(&trabajo).map_err(|e| ErrorWeb::escritura(&trabajo, e))?;

    let informe = match generar(opciones, &trabajo) {
        Ok(informe) => informe,
        Err(e) => {
            let _ = fs::remove_dir_all(&trabajo);
            return Err(e);
        }
    };
    if let Err(e) = sustituir(&trabajo, &salida, &padre, &nombre, pid) {
        let _ = fs::remove_dir_all(&trabajo);
        return Err(e);
    }
    Ok(informe)
}

/// Todo el trabajo, en la carpeta de trabajo: nada de aquí toca la salida anterior.
fn generar(o: &OpcionesConstruir, sitio: &Path) -> Resultado<InformeConstruccion> {
    let plataformas = Plataformas::cargar(&o.web.join("datos/plataformas.toml"))?;
    let textos = Textos::cargar(&o.web.join("textos"))?;
    let plantillas = Plantillas::cargar(&o.web.join("plantillas"))?;
    let documento = DocumentoReleases::leer(&o.releases)?;
    let eleccion = elegir(
        &documento,
        &plataformas,
        OpcionesDescargas {
            homebrew: o.homebrew,
            aur: o.aur,
        },
    )?;

    let entorno = Entorno {
        plantillas: &plantillas,
        textos: &textos,
        plataformas: &plataformas,
        version_manual: &eleccion.escritorio.numero,
    };

    let mut informe = InformeConstruccion {
        version_escritorio: eleccion.escritorio.numero.clone(),
        version_termux: eleccion.termux.as_ref().map(|v| v.numero.clone()),
        version_app: eleccion.app.as_ref().map(|v| v.numero.clone()),
        ..Default::default()
    };

    // Los CHANGELOG de la etiqueta de cada canal: lo que se lee es lo que se baja.
    let versiones = DatosVersiones {
        escritorio: leer_changelog(
            &o.changelog_escritorio,
            &eleccion.escritorio.etiqueta,
            &eleccion.escritorio.numero,
        )?,
        app: match (&eleccion.app, &o.changelog_app) {
            (Some(app), Some(ruta)) => Some(leer_changelog(ruta, &app.etiqueta, &app.numero)?),
            (Some(app), None) => {
                return Err(ErrorWeb::dato_raro(
                    "--changelog-app",
                    format!(
                        "hay una release de la app ({}) y no se ha dado su CHANGELOG",
                        app.etiqueta
                    ),
                ));
            }
            // Sin release de la app, su CHANGELOG no se usa: la página dice «aún sin publicar».
            (None, _) => None,
        },
    };
    let capturas = Capturas::cargar(&o.web.join("capturas"))?;

    // Páginas propias: sin JavaScript, con su CSP al final.
    for idioma in Idioma::TODOS {
        let paginas_del_idioma = [
            (
                Pagina::Portada,
                paginas::portada::renderizar(&entorno, idioma, &eleccion, &capturas)?,
            ),
            (
                Pagina::Descargas,
                paginas::descargas::renderizar(&entorno, idioma, &eleccion)?,
            ),
            (
                Pagina::Versiones,
                paginas::versiones::renderizar(&entorno, idioma, &versiones)?,
            ),
        ];
        for (pagina, html) in paginas_del_idioma {
            escribir_pagina(sitio, &rutas::fichero(pagina, idioma), &html, &mut informe)?;
        }
    }
    let html_404 = paginas::no_encontrada::renderizar(&entorno)?;
    escribir_pagina(
        sitio,
        &rutas::fichero(Pagina::NoEncontrada, Idioma::Es),
        &html_404,
        &mut informe,
    )?;

    // Estáticos de la web.
    copiar_fichero(
        &o.web.join("estilos/web.css"),
        &sitio.join("estilos/web.css"),
    )?;
    copiar_fichero(&o.web.join("favicon.svg"), &sitio.join("favicon.svg"))?;
    let mut omitidos = Vec::new();
    copiar_arbol(
        &o.web.join("fuentes"),
        &sitio.join("fuentes"),
        &["woff2", "txt"],
        &mut omitidos,
    )?;
    // De `web/capturas/` solo va la CSS: los fragmentos HTML ya están dentro de la portada.
    copiar_arbol(
        &o.web.join("capturas"),
        &sitio.join("capturas"),
        &["css"],
        &mut omitidos,
    )?;
    if let Some(animacion) = &o.animacion {
        copiar_animacion(animacion, &sitio.join("demo/magi.gif"))?;
    }

    // Manual ya generado por mdBook: cada página lleva su CSP con los hashes de sus
    // scripts en línea, calculados sobre el HTML definitivo.
    if let Some(manual) = &o.manual {
        let destino = sitio.join("manual");
        copiar_arbol(manual, &destino, EXTENSIONES_MANUAL, &mut informe.omitidos)?;
        poner_csp_al_manual(&destino, sitio, &mut informe)?;
    }

    informe.paginas.sort();
    Ok(informe)
}

/// Escribe una página propia, con la CSP puesta, en `ruta` (relativa al sitio).
fn escribir_pagina(
    sitio: &Path,
    ruta: &str,
    html: &str,
    informe: &mut InformeConstruccion,
) -> Resultado<()> {
    let html = csp::insertar(html, false)
        .map_err(|e| ErrorWeb::dato_raro(format!("página {ruta}"), e.to_string()))?;
    let destino = sitio.join(ruta);
    crear_carpeta_madre(&destino)?;
    fs::write(&destino, html).map_err(|e| ErrorWeb::escritura(&destino, e))?;
    informe.paginas.push(ruta.to_string());
    Ok(())
}

/// Lee un CHANGELOG, lo parte en versiones y comprueba que trae la de la release:
/// si no, es el fichero equivocado (otra etiqueta o el de otro canal) y la página
/// enseñaría lo que no es.
fn leer_changelog(ruta: &Path, etiqueta: &str, numero: &str) -> Resultado<Vec<EntradaChangelog>> {
    let texto = leer_texto(ruta, TOPE_CHANGELOG as u64)?;
    let origen = ruta.display().to_string();
    let entradas = entradas_de_changelog(&texto, &origen, etiqueta)?;
    if !entradas.iter().any(|e| e.numero == numero) {
        return Err(ErrorWeb::dato_raro(
            origen,
            format!("el CHANGELOG no trae la versión {numero} de la release {etiqueta}"),
        ));
    }
    Ok(entradas)
}

/// Pone la CSP del manual a cada `.html` de `carpeta`.
fn poner_csp_al_manual(
    carpeta: &Path,
    sitio: &Path,
    informe: &mut InformeConstruccion,
) -> Resultado<()> {
    let mut paginas = Vec::new();
    listar_html(carpeta, 0, &mut paginas)?;
    paginas.sort();
    for ruta in paginas {
        let relativa = ruta
            .strip_prefix(sitio)
            .unwrap_or(&ruta)
            .to_string_lossy()
            .into_owned();
        let html = leer_texto(&ruta, TOPE_HTML)?;
        let nuevo = csp::insertar(&html, true)
            .map_err(|e| ErrorWeb::dato_raro(format!("manual: {relativa}"), e.to_string()))?;
        fs::write(&ruta, nuevo).map_err(|e| ErrorWeb::escritura(&ruta, e))?;
        informe.paginas.push(relativa);
    }
    Ok(())
}

fn listar_html(carpeta: &Path, profundidad: usize, salida: &mut Vec<PathBuf>) -> Resultado<()> {
    if profundidad > PROFUNDIDAD_MAXIMA {
        return Err(ErrorWeb::dato_raro(
            carpeta.display().to_string(),
            "demasiados niveles de carpetas",
        ));
    }
    let entradas = fs::read_dir(carpeta).map_err(|e| ErrorWeb::lectura(carpeta, e))?;
    for entrada in entradas {
        let entrada = entrada.map_err(|e| ErrorWeb::lectura(carpeta, e))?;
        let ruta = entrada.path();
        let tipo = entrada
            .file_type()
            .map_err(|e| ErrorWeb::lectura(&ruta, e))?;
        if tipo.is_dir() {
            listar_html(&ruta, profundidad + 1, salida)?;
        } else if tipo.is_file() && ruta.extension().is_some_and(|x| x == "html") {
            salida.push(ruta);
        }
    }
    Ok(())
}

/// Lee un fichero de texto con tope, sin seguir enlaces simbólicos.
fn leer_texto(ruta: &Path, tope: u64) -> Resultado<String> {
    let meta = fs::symlink_metadata(ruta).map_err(|e| ErrorWeb::lectura(ruta, e))?;
    if !meta.is_file() {
        return Err(ErrorWeb::dato_raro(
            ruta.display().to_string(),
            "no es un fichero normal",
        ));
    }
    if meta.len() > tope {
        return Err(ErrorWeb::dato_raro(
            ruta.display().to_string(),
            format!("pasa del tope de {tope} bytes"),
        ));
    }
    fs::read_to_string(ruta).map_err(|e| ErrorWeb::lectura(ruta, e))
}

fn crear_carpeta_madre(destino: &Path) -> Resultado<()> {
    if let Some(madre) = destino.parent() {
        fs::create_dir_all(madre).map_err(|e| ErrorWeb::escritura(madre, e))?;
    }
    Ok(())
}

/// Copia un fichero normal (nunca un enlace simbólico) con tope de tamaño.
fn copiar_fichero(origen: &Path, destino: &Path) -> Resultado<()> {
    copiar_con_tope(origen, destino, TOPE_FICHERO)
}

fn copiar_con_tope(origen: &Path, destino: &Path, tope: u64) -> Resultado<()> {
    let meta = fs::symlink_metadata(origen).map_err(|e| ErrorWeb::lectura(origen, e))?;
    if meta.file_type().is_symlink() {
        return Err(ErrorWeb::dato_raro(
            origen.display().to_string(),
            "es un enlace simbólico: no se copia",
        ));
    }
    if !meta.is_file() {
        return Err(ErrorWeb::dato_raro(
            origen.display().to_string(),
            "no es un fichero normal",
        ));
    }
    if meta.len() > tope {
        return Err(ErrorWeb::dato_raro(
            origen.display().to_string(),
            format!("pesa {} bytes y el tope es {tope}", meta.len()),
        ));
    }
    crear_carpeta_madre(destino)?;
    // `take` acota la lectura aunque el fichero crezca entre la comprobación y la copia.
    let mut lector = fs::File::open(origen)
        .map_err(|e| ErrorWeb::lectura(origen, e))?
        .take(tope + 1);
    let mut datos = Vec::new();
    lector
        .read_to_end(&mut datos)
        .map_err(|e| ErrorWeb::lectura(origen, e))?;
    if datos.len() as u64 > tope {
        return Err(ErrorWeb::dato_raro(
            origen.display().to_string(),
            format!("pasa del tope de {tope} bytes"),
        ));
    }
    fs::write(destino, datos).map_err(|e| ErrorWeb::escritura(destino, e))
}

/// La animación de la portada: un GIF de verdad y de tamaño razonable.
fn copiar_animacion(origen: &Path, destino: &Path) -> Resultado<()> {
    let mut cabecera = [0u8; 6];
    fs::File::open(origen)
        .and_then(|mut f| f.read_exact(&mut cabecera))
        .map_err(|e| ErrorWeb::lectura(origen, e))?;
    if &cabecera != b"GIF89a" && &cabecera != b"GIF87a" {
        return Err(ErrorWeb::dato_raro(
            origen.display().to_string(),
            "la animación no es un GIF",
        ));
    }
    copiar_con_tope(origen, destino, TOPE_ANIMACION)
}

/// Copia un árbol de ficheros normales, solo con las extensiones permitidas; los
/// demás se anotan en `omitidos`. Nunca sigue enlaces simbólicos: un enlace es un
/// error, no algo que se salta.
fn copiar_arbol(
    origen: &Path,
    destino: &Path,
    extensiones: &[&str],
    omitidos: &mut Vec<String>,
) -> Resultado<()> {
    let mut cuenta = 0usize;
    copiar_arbol_nivel(
        origen,
        origen,
        destino,
        extensiones,
        0,
        &mut cuenta,
        omitidos,
    )
}

fn copiar_arbol_nivel(
    raiz: &Path,
    carpeta: &Path,
    destino_raiz: &Path,
    extensiones: &[&str],
    profundidad: usize,
    cuenta: &mut usize,
    omitidos: &mut Vec<String>,
) -> Resultado<()> {
    if profundidad > PROFUNDIDAD_MAXIMA {
        return Err(ErrorWeb::dato_raro(
            carpeta.display().to_string(),
            "demasiados niveles de carpetas",
        ));
    }
    let meta = fs::symlink_metadata(carpeta).map_err(|e| ErrorWeb::lectura(carpeta, e))?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(ErrorWeb::dato_raro(
            carpeta.display().to_string(),
            "no es una carpeta normal (¿enlace simbólico?)",
        ));
    }
    let mut entradas: Vec<_> = fs::read_dir(carpeta)
        .map_err(|e| ErrorWeb::lectura(carpeta, e))?
        .collect::<Result<_, _>>()
        .map_err(|e| ErrorWeb::lectura(carpeta, e))?;
    entradas.sort_by_key(fs::DirEntry::file_name);
    for entrada in entradas {
        let ruta = entrada.path();
        let nombre = entrada.file_name().to_string_lossy().into_owned();
        if nombre.chars().any(|c| c.is_control() || c == '\\') || nombre.contains("..") {
            return Err(ErrorWeb::dato_raro(
                ruta.display().to_string(),
                "nombre de fichero no admitido",
            ));
        }
        let tipo = fs::symlink_metadata(&ruta).map_err(|e| ErrorWeb::lectura(&ruta, e))?;
        let relativa = ruta.strip_prefix(raiz).unwrap_or(&ruta);
        if tipo.file_type().is_symlink() {
            return Err(ErrorWeb::dato_raro(
                ruta.display().to_string(),
                "es un enlace simbólico: no se copia",
            ));
        }
        if tipo.is_dir() {
            copiar_arbol_nivel(
                raiz,
                &ruta,
                destino_raiz,
                extensiones,
                profundidad + 1,
                cuenta,
                omitidos,
            )?;
            continue;
        }
        if !tipo.is_file() {
            return Err(ErrorWeb::dato_raro(
                ruta.display().to_string(),
                "ni carpeta ni fichero normal",
            ));
        }
        let permitido = ruta
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| extensiones.contains(&x.to_ascii_lowercase().as_str()));
        if !permitido {
            omitidos.push(relativa.to_string_lossy().into_owned());
            continue;
        }
        *cuenta += 1;
        if *cuenta > TOPE_FICHEROS {
            return Err(ErrorWeb::dato_raro(
                raiz.display().to_string(),
                format!("más de {TOPE_FICHEROS} ficheros"),
            ));
        }
        copiar_fichero(&ruta, &destino_raiz.join(relativa))?;
    }
    Ok(())
}

/// Comprueba la carpeta de salida y la deja absoluta y sin solaparse con ninguna
/// entrada: construirla borraría (al sustituirla) lo que no es suyo.
fn validar_salida(o: &OpcionesConstruir) -> Resultado<PathBuf> {
    if o.salida
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(ErrorWeb::dato_raro("--salida", "no puede llevar «..»"));
    }
    let nombre = o
        .salida
        .file_name()
        .ok_or_else(|| ErrorWeb::dato_raro("--salida", "no tiene nombre de carpeta"))?
        .to_os_string();
    let madre = match o.salida.parent() {
        Some(m) if !m.as_os_str().is_empty() => m.to_path_buf(),
        _ => PathBuf::from("."),
    };
    fs::create_dir_all(&madre).map_err(|e| ErrorWeb::escritura(&madre, e))?;
    let madre = fs::canonicalize(&madre).map_err(|e| ErrorWeb::lectura(&madre, e))?;
    let salida = madre.join(nombre);

    if let Ok(meta) = fs::symlink_metadata(&salida) {
        if meta.file_type().is_symlink() || !meta.is_dir() {
            return Err(ErrorWeb::dato_raro(
                salida.display().to_string(),
                "ya existe y no es una carpeta normal",
            ));
        }
    }

    let mut entradas: Vec<(&str, &Path)> = vec![("--web", &o.web), ("--releases", &o.releases)];
    if let Some(m) = &o.manual {
        entradas.push(("--manual", m));
    }
    if let Some(a) = &o.animacion {
        entradas.push(("--animacion", a));
    }
    entradas.push(("--changelog", &o.changelog_escritorio));
    if let Some(c) = &o.changelog_app {
        entradas.push(("--changelog-app", c));
    }
    for (que, ruta) in entradas {
        let ruta = fs::canonicalize(ruta).map_err(|e| ErrorWeb::lectura(ruta, e))?;
        if ruta.starts_with(&salida) || salida.starts_with(&ruta) {
            return Err(ErrorWeb::dato_raro(
                "--salida",
                format!(
                    "se solapa con {que} ({}): construir sustituiría lo que no es suyo",
                    ruta.display()
                ),
            ));
        }
    }
    Ok(salida)
}

/// Sustituye `salida` por `trabajo`. Si algo falla, deja la anterior como estaba.
fn sustituir(trabajo: &Path, salida: &Path, padre: &Path, nombre: &str, pid: u32) -> Resultado<()> {
    if fs::symlink_metadata(salida).is_err() {
        return fs::rename(trabajo, salida).map_err(|e| ErrorWeb::escritura(salida, e));
    }
    let anterior = padre.join(format!(".{nombre}.anterior-{pid}"));
    if anterior.exists() {
        fs::remove_dir_all(&anterior).map_err(|e| ErrorWeb::escritura(&anterior, e))?;
    }
    fs::rename(salida, &anterior).map_err(|e| ErrorWeb::escritura(salida, e))?;
    if let Err(e) = fs::rename(trabajo, salida) {
        // Se devuelve la anterior a su sitio.
        let _ = fs::rename(&anterior, salida);
        return Err(ErrorWeb::escritura(salida, e));
    }
    let _ = fs::remove_dir_all(&anterior);
    Ok(())
}
