//! Qué versión y qué artefactos enseña la web (informe §7.1 a §7.3).
//!
//! - Sale la mayor **por número** (no la más reciente por fecha) de cada canal,
//!   sin borradores, prereleases ni candidatas (`-rc.N`).
//! - Termux comparte release con el escritorio, pero puede ir un paso por detrás
//!   mientras `termux-release.yml` sube su `.deb`: se elige la más nueva que tenga
//!   el `.deb` **y** su `.sha256`, ambos en estado `uploaded`.
//! - Cada artefacto lleva la suma de su `.sha256` publicado, validada con
//!   `validar_suma`. Falta un artefacto o su suma en una versión elegida: falla
//!   (salvo Termux).
//! - La huella del certificado de la app es una constante de `plataformas.toml`,
//!   comparada con la de las notas de la release de la app.

use serde::Serialize;

use crate::datos::{es_huella_valida, Canal, Plataforma, Plataformas};
use crate::error::{ErrorWeb, Resultado};
use crate::ordenes::texto_seguro;
use crate::releases::{ActivoApi, DocumentoReleases, ReleaseApi, REPOSITORIO, TOPE_SHA256};

/// Opciones que vienen del entorno (`PUBLICAR_HOMEBREW` y `PUBLICAR_AUR` a `true`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpcionesDescargas {
    pub homebrew: bool,
    pub aur: bool,
}

/// Una versión publicada de un canal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VersionElegida {
    pub canal: Canal,
    /// `v1.6.1`, `android-v0.2.0`…
    pub etiqueta: String,
    /// `1.6.1`, `0.2.0`…
    pub numero: String,
    /// `published_at` tal cual lo da la API (`2026-11-22T11:04:48Z`).
    pub fecha: String,
    /// Página de la release en GitHub.
    pub url: String,
}

/// Un artefacto elegido, con su suma ya validada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ArtefactoElegido {
    /// Id de la plataforma (`linux-x86_64`…).
    pub plataforma: String,
    pub canal: Canal,
    pub nombre: String,
    /// Bytes, según la API.
    pub tamano: u64,
    /// `browser_download_url`, siempre bajo `https://github.com/HectorEspejo/magi/releases/download/`.
    pub url: String,
    /// 64 hexadecimales en minúsculas.
    pub sha256: String,
    /// Versión (etiqueta) de la que sale: Termux puede ser la anterior.
    pub etiqueta: String,
}

/// Lo que necesita la página de descargas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EleccionDescargas {
    pub escritorio: VersionElegida,
    /// La versión de Termux con `.deb` (puede ser anterior a la del escritorio).
    pub termux: Option<VersionElegida>,
    /// `None`: la app aún no tiene ninguna release («aún sin publicar»).
    pub app: Option<VersionElegida>,
    /// Ordenados como las plataformas de `plataformas.toml`.
    pub artefactos: Vec<ArtefactoElegido>,
    pub homebrew: bool,
    pub aur: bool,
}

/// Las versiones elegidas, sin sumas (lo usa `releases::descargar_releases` para
/// saber qué `.sha256` bajar).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeleccionVersiones {
    pub escritorio: VersionElegida,
    pub termux: Option<VersionElegida>,
    pub app: Option<VersionElegida>,
}

/// Estado de un activo de la API cuya subida terminó.
const ACTIVO_SUBIDO: &str = "uploaded";
/// Cifras como mucho de cada número de una versión.
const CIFRAS_MAXIMAS: usize = 4;

type Numero = (u32, u32, u32);

/// URL de descarga de un activo de una release: la única que se acepta.
pub(crate) fn url_descarga(etiqueta: &str, nombre: &str) -> String {
    format!("https://github.com/{REPOSITORIO}/releases/download/{etiqueta}/{nombre}")
}

/// URL de la página de una release: la única que se acepta.
pub(crate) fn url_release(etiqueta: &str) -> String {
    format!("https://github.com/{REPOSITORIO}/releases/tag/{etiqueta}")
}

/// De dónde sale un dato de una release, para los mensajes de error.
fn donde(etiqueta: &str) -> String {
    format!("releases.json (release {})", texto_seguro(etiqueta, 60))
}

/// Añade la release al «dónde» de un dato raro.
fn anotar(error: ErrorWeb, etiqueta: &str) -> ErrorWeb {
    match error {
        ErrorWeb::DatoRaro { donde, que } => ErrorWeb::DatoRaro {
            donde: format!("{donde} (release {})", texto_seguro(etiqueta, 60)),
            que,
        },
        otro => otro,
    }
}

/// `(mayor, menor, parche)` de una etiqueta de release estable del canal, o `None`
/// si no cumple el patrón exacto (`^v(0|[1-9]\d*)\.…$`, `^android-v…$`, cada número
/// de hasta 4 cifras, sin ceros a la izquierda ni sufijos).
pub fn numero_de_etiqueta(etiqueta: &str, canal: Canal) -> Option<(u32, u32, u32)> {
    let resto = etiqueta.strip_prefix(canal.prefijo_etiqueta())?;
    let mut partes = resto.split('.');
    let mayor = parte_de_version(partes.next()?)?;
    let menor = parte_de_version(partes.next()?)?;
    let parche = parte_de_version(partes.next()?)?;
    if partes.next().is_some() {
        return None;
    }
    Some((mayor, menor, parche))
}

/// Un número de versión: solo cifras ASCII, de 1 a `CIFRAS_MAXIMAS`, sin ceros a
/// la izquierda.
fn parte_de_version(parte: &str) -> Option<u32> {
    let bytes = parte.as_bytes();
    if bytes.is_empty() || bytes.len() > CIFRAS_MAXIMAS || !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    if bytes.len() > 1 && bytes[0] == b'0' {
        return None;
    }
    parte.parse().ok()
}

fn numero_en_texto(numero: Numero) -> String {
    format!("{}.{}.{}", numero.0, numero.1, numero.2)
}

/// `AAAA-MM-DDThh:mm:ssZ`, como lo da la API.
fn es_fecha_de_la_api(fecha: &str) -> bool {
    let b = fecha.as_bytes();
    b.len() == 20
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            19 => *c == b'Z',
            _ => c.is_ascii_digit(),
        })
}

/// Las releases publicadas del canal (ni borradores ni prereleases ni etiquetas
/// que no cumplan el patrón exacto), de la mayor a la menor **por número**.
fn publicadas(doc: &DocumentoReleases, canal: Canal) -> Resultado<Vec<(Numero, &ReleaseApi)>> {
    let mut lista: Vec<(Numero, &ReleaseApi)> = doc
        .releases
        .iter()
        .filter(|r| !r.draft && !r.prerelease)
        .filter_map(|r| numero_de_etiqueta(&r.tag_name, canal).map(|n| (n, r)))
        .collect();
    lista.sort_by_key(|(numero, _)| std::cmp::Reverse(*numero));
    if let Some(par) = lista.windows(2).find(|par| par[0].0 == par[1].0) {
        return Err(ErrorWeb::dato_raro(
            "releases.json",
            format!(
                "la etiqueta {} aparece más de una vez entre las releases publicadas",
                texto_seguro(&par[0].1.tag_name, 60)
            ),
        ));
    }
    Ok(lista)
}

pub(crate) fn release_de<'a>(doc: &'a DocumentoReleases, etiqueta: &str) -> Option<&'a ReleaseApi> {
    doc.releases
        .iter()
        .find(|r| r.tag_name == etiqueta && !r.draft && !r.prerelease)
}

/// El activo con ese nombre exacto; si hay dos, falla.
pub(crate) fn activo_unico<'a>(
    release: &'a ReleaseApi,
    nombre: &str,
) -> Resultado<Option<&'a ActivoApi>> {
    let mut coincidentes = release.assets.iter().filter(|a| a.name == nombre);
    let primero = coincidentes.next();
    if coincidentes.next().is_some() {
        return Err(ErrorWeb::dato_raro(
            donde(&release.tag_name),
            format!(
                "el activo «{}» aparece más de una vez",
                texto_seguro(nombre, 80)
            ),
        ));
    }
    Ok(primero)
}

fn activo_subido(release: &ReleaseApi, nombre: &str) -> Resultado<bool> {
    Ok(activo_unico(release, nombre)?.is_some_and(|a| a.state == ACTIVO_SUBIDO))
}

/// Valida lo que enseña una versión (fecha y página) y la arma.
fn version_elegida(
    canal: Canal,
    release: &ReleaseApi,
    numero: Numero,
) -> Resultado<VersionElegida> {
    let etiqueta = &release.tag_name;
    let fecha = match release.published_at.as_deref() {
        Some(f) if es_fecha_de_la_api(f) => f.to_string(),
        Some(f) => {
            return Err(ErrorWeb::dato_raro(
                donde(etiqueta),
                format!(
                    "published_at «{}» no tiene la forma AAAA-MM-DDThh:mm:ssZ",
                    texto_seguro(f, 40)
                ),
            ))
        }
        None => {
            return Err(ErrorWeb::dato_raro(
                donde(etiqueta),
                "falta published_at en una release publicada",
            ))
        }
    };
    let esperada = url_release(etiqueta);
    if release.html_url != esperada {
        return Err(ErrorWeb::dato_raro(
            donde(etiqueta),
            format!(
                "html_url «{}» no es la página de la release («{esperada}»)",
                texto_seguro(&release.html_url, 200)
            ),
        ));
    }
    Ok(VersionElegida {
        canal,
        etiqueta: etiqueta.clone(),
        numero: numero_en_texto(numero),
        fecha,
        url: release.html_url.clone(),
    })
}

/// Las releases candidatas de cada canal, de la mejor a la peor.
struct Candidatas<'a> {
    escritorio: Vec<(Numero, &'a ReleaseApi)>,
    /// Releases del escritorio con el `.deb` de Termux y su `.sha256` subidos.
    termux: Vec<(Numero, &'a ReleaseApi)>,
    app: Vec<(Numero, &'a ReleaseApi)>,
}

fn candidatas<'a>(
    doc: &'a DocumentoReleases,
    plataformas: &Plataformas,
) -> Resultado<Candidatas<'a>> {
    let escritorio = publicadas(doc, Canal::Escritorio)?;
    if escritorio.is_empty() {
        return Err(ErrorWeb::dato_raro(
            "releases.json",
            "no hay ninguna versión publicada del escritorio (etiqueta vX.Y.Z, sin borradores ni candidatas)",
        ));
    }
    let de_termux = plataformas.del_canal(Canal::Termux);
    let mut termux = Vec::new();
    if !de_termux.is_empty() {
        for (numero, release) in &escritorio {
            let texto = numero_en_texto(*numero);
            let mut completa = true;
            for p in &de_termux {
                let nombre = p.nombre_artefacto(&texto);
                if !activo_subido(release, &nombre)?
                    || !activo_subido(release, &format!("{nombre}.sha256"))?
                {
                    completa = false;
                    break;
                }
            }
            if completa {
                termux.push((*numero, *release));
            }
        }
    }
    let app = publicadas(doc, Canal::App)?;
    Ok(Candidatas {
        escritorio,
        termux,
        app,
    })
}

/// Elige la versión de cada canal. Sin ninguna del escritorio: falla.
pub fn elegir_versiones(
    doc: &DocumentoReleases,
    plataformas: &Plataformas,
) -> Resultado<SeleccionVersiones> {
    let c = candidatas(doc, plataformas)?;
    let (numero, release) = c.escritorio[0];
    let escritorio = version_elegida(Canal::Escritorio, release, numero)?;
    let termux = match c.termux.first() {
        Some((numero, release)) => Some(version_elegida(Canal::Termux, release, *numero)?),
        None => None,
    };
    let app = match c.app.first() {
        Some((numero, release)) => Some(version_elegida(Canal::App, release, *numero)?),
        None => None,
    };
    Ok(SeleccionVersiones {
        escritorio,
        termux,
        app,
    })
}

/// Elige versiones y artefactos y valida todo: nombres, tamaño mayor que 0, URL
/// bajo la release, sumas y, si hay app, su huella contra `plataformas.app`.
pub fn elegir(
    doc: &DocumentoReleases,
    plataformas: &Plataformas,
    opciones: OpcionesDescargas,
) -> Resultado<EleccionDescargas> {
    let c = candidatas(doc, plataformas)?;
    let (numero, release) = c.escritorio[0];
    let escritorio = version_elegida(Canal::Escritorio, release, numero)?;

    // Termux: la más nueva con el `.deb`, su `.sha256` y la suma ya bajada. Si la
    // última aún no los tiene, cae a la anterior (§7.1).
    let de_termux = plataformas.del_canal(Canal::Termux);
    let mut termux = None;
    for (numero, release) in &c.termux {
        let texto = numero_en_texto(*numero);
        let con_suma = de_termux.iter().all(|p| {
            doc.sumas.contains_key(&DocumentoReleases::clave_suma(
                &release.tag_name,
                &p.nombre_artefacto(&texto),
            ))
        });
        if con_suma {
            termux = Some(version_elegida(Canal::Termux, release, *numero)?);
            break;
        }
    }

    // App: sin ninguna release, no hay bloque ni huella que comprobar.
    let app = match c.app.first() {
        Some((numero, release)) => {
            let version = version_elegida(Canal::App, release, *numero)?;
            comprobar_huella(
                release.body.as_deref().unwrap_or(""),
                &plataformas.app.huella_certificado,
            )
            .map_err(|e| anotar(e, &release.tag_name))?;
            Some(version)
        }
        None => None,
    };

    let mut artefactos = Vec::new();
    for plataforma in &plataformas.plataforma {
        let version = match plataforma.canal {
            Canal::Escritorio => Some(&escritorio),
            Canal::Termux => termux.as_ref(),
            Canal::App => app.as_ref(),
        };
        if let Some(version) = version {
            artefactos.push(artefacto_elegido(doc, plataforma, version)?);
        }
    }

    Ok(EleccionDescargas {
        escritorio,
        termux,
        app,
        artefactos,
        homebrew: opciones.homebrew,
        aur: opciones.aur,
    })
}

/// Comprueba un activo de la release: nombre exacto, subido, con tamaño y con la
/// URL de descarga de esa release y no otra.
fn activo_correcto<'a>(release: &'a ReleaseApi, nombre: &str) -> Resultado<&'a ActivoApi> {
    let etiqueta = &release.tag_name;
    let Some(activo) = activo_unico(release, nombre)? else {
        return Err(ErrorWeb::dato_raro(
            donde(etiqueta),
            format!("falta «{}»", texto_seguro(nombre, 80)),
        ));
    };
    if activo.state != ACTIVO_SUBIDO {
        return Err(ErrorWeb::dato_raro(
            donde(etiqueta),
            format!(
                "«{}» está en estado «{}», no «{ACTIVO_SUBIDO}»",
                texto_seguro(nombre, 80),
                texto_seguro(&activo.state, 30)
            ),
        ));
    }
    if activo.size == 0 {
        return Err(ErrorWeb::dato_raro(
            donde(etiqueta),
            format!("«{}» pesa 0 bytes", texto_seguro(nombre, 80)),
        ));
    }
    let esperada = url_descarga(etiqueta, nombre);
    if activo.browser_download_url != esperada {
        return Err(ErrorWeb::dato_raro(
            donde(etiqueta),
            format!(
                "la URL de descarga de «{}» es «{}» y no la esperada «{esperada}» (solo se admiten las de https://github.com/{REPOSITORIO}/releases/download/<etiqueta>/)",
                texto_seguro(nombre, 80),
                texto_seguro(&activo.browser_download_url, 200)
            ),
        ));
    }
    Ok(activo)
}

fn artefacto_elegido(
    doc: &DocumentoReleases,
    plataforma: &Plataforma,
    version: &VersionElegida,
) -> Resultado<ArtefactoElegido> {
    let nombre = plataforma.nombre_artefacto(&version.numero);
    let Some(release) = release_de(doc, &version.etiqueta) else {
        return Err(ErrorWeb::dato_raro(
            "releases.json",
            format!("la release {} ya no está en el documento", version.etiqueta),
        ));
    };
    let activo = activo_correcto(release, &nombre)?;
    let nombre_suma = format!("{nombre}.sha256");
    let activo_suma = activo_correcto(release, &nombre_suma)?;
    if activo_suma.size > TOPE_SHA256 as u64 {
        return Err(ErrorWeb::dato_raro(
            donde(&version.etiqueta),
            format!(
                "«{nombre_suma}» pesa {} bytes y el tope son {TOPE_SHA256}",
                activo_suma.size
            ),
        ));
    }
    let clave = DocumentoReleases::clave_suma(&version.etiqueta, &nombre);
    let Some(texto) = doc.sumas.get(&clave) else {
        return Err(ErrorWeb::dato_raro(
            donde(&version.etiqueta),
            format!("falta la suma de «{nombre}» (clave «{clave}» de sumas)"),
        ));
    };
    let sha256 = validar_suma(texto, &nombre).map_err(|e| anotar(e, &version.etiqueta))?;
    Ok(ArtefactoElegido {
        plataforma: plataforma.id.clone(),
        canal: plataforma.canal,
        nombre,
        tamano: activo.size,
        url: activo.browser_download_url.clone(),
        sha256,
        etiqueta: version.etiqueta.clone(),
    })
}

/// Valida el texto de un `.sha256` (`strip`, una sola línea, `^([0-9a-f]{64}) [ *](\S+)$`
/// y nombre exacto) y devuelve la suma. Acepta el formato de `dist`
/// (`<hash> *<nombre>` seguido de una línea en blanco) y el de `sha256sum`
/// (`<hash>  <nombre>`).
pub fn validar_suma(texto: &str, nombre: &str) -> Resultado<String> {
    let donde = format!("{}.sha256", texto_seguro(nombre, 80));
    let forma = || {
        format!(
            "no tiene la forma «<64 hexadecimales en minúsculas>  {}» en una sola línea",
            texto_seguro(nombre, 80)
        )
    };
    if nombre.is_empty() || nombre.contains('/') || nombre.contains("..") {
        return Err(ErrorWeb::dato_raro(
            donde,
            "el nombre del artefacto es vacío o lleva «/» o «..»",
        ));
    }
    let limpio = texto.trim();
    if limpio.contains('\n') || limpio.contains('\r') {
        return Err(ErrorWeb::dato_raro(
            donde,
            format!("tiene más de una línea: {}", forma()),
        ));
    }
    let bytes = limpio.as_bytes();
    // Los 66 primeros bytes son ASCII si el texto es válido; si no, falla antes de cortar.
    let cabecera_valida = bytes.len() > 66
        && bytes[..64]
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
        && bytes[64] == b' '
        && (bytes[65] == b' ' || bytes[65] == b'*');
    if !cabecera_valida {
        return Err(ErrorWeb::dato_raro(donde, forma()));
    }
    let hash = &limpio[..64];
    let en_el_fichero = &limpio[66..];
    if en_el_fichero.chars().any(char::is_whitespace) {
        return Err(ErrorWeb::dato_raro(donde, forma()));
    }
    if en_el_fichero.contains('/') || en_el_fichero.contains("..") {
        return Err(ErrorWeb::dato_raro(
            donde,
            format!(
                "el nombre «{}» lleva «/» o «..»",
                texto_seguro(en_el_fichero, 80)
            ),
        ));
    }
    if en_el_fichero != nombre {
        return Err(ErrorWeb::dato_raro(
            donde,
            format!(
                "nombra «{}» y el artefacto es «{}»",
                texto_seguro(en_el_fichero, 80),
                texto_seguro(nombre, 80)
            ),
        ));
    }
    Ok(hash.to_string())
}

/// Línea de las notas de la release de la app que lleva la huella.
const MARCA_HUELLA: &str = "- Certificado de firma (SHA-256): ";

/// Compara la huella de las notas de la release de la app
/// (`- Certificado de firma (SHA-256): AA:BB:…`) con la constante del repositorio.
/// Constante vacía, notas sin huella o huellas distintas: falla enseñando las dos.
pub fn comprobar_huella(notas: &str, constante: &str) -> Resultado<()> {
    const DONDE: &str = "huella del certificado de la app";
    if constante.is_empty() {
        return Err(ErrorWeb::dato_raro(
            DONDE,
            "falta huella_certificado en web/datos/plataformas.toml",
        ));
    }
    let mut con_marca = None;
    let mut huellas: Vec<&str> = Vec::new();
    for linea in notas.lines() {
        if let Some(resto) = linea.strip_prefix(MARCA_HUELLA) {
            if es_huella_valida(resto) {
                huellas.push(resto);
            } else if con_marca.is_none() {
                con_marca = Some(resto);
            }
        }
    }
    if huellas.is_empty() {
        let detalle = match con_marca {
            Some(rara) => format!(
                " (la línea del certificado dice «{}», que no son 32 pares hexadecimales en mayúsculas)",
                texto_seguro(rara, 100)
            ),
            None => String::new(),
        };
        return Err(ErrorWeb::dato_raro(
            DONDE,
            format!("las notas de la release de la app no traen la huella{detalle}"),
        ));
    }
    // Si hay varias líneas, todas deben coincidir: una distinta es un cambio de keystore.
    if let Some(distinta) = huellas.iter().find(|h| **h != constante) {
        return Err(ErrorWeb::dato_raro(
            DONDE,
            format!("huella de la release {distinta} ≠ la del repositorio {constante}"),
        ));
    }
    Ok(())
}
