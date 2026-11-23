//! Página de descargas (`/descargas/` y `/en/download/`).
//!
//! Todo lo que llega de la API (nombres, URL, sumas, fechas) se vuelve a mirar
//! aquí aunque `descargas::elegir` ya lo haya validado: las URL tienen que ser de
//! las releases del repositorio, la suma tiene que ser de 64 hexadecimales en
//! minúsculas y la fecha tiene que ser una fecha. Lo demás va a la plantilla, que
//! lo escapa (nunca con `|safe`).

use serde_json::{json, Value};

use crate::datos::{Canal, Plataforma};
use crate::descargas::{ArtefactoElegido, EleccionDescargas, VersionElegida};
use crate::error::{ErrorWeb, Resultado};
use crate::paginas::{
    ampliar, contexto_comun, fecha_legible, tamano_legible, texto_necesario, Entorno,
};
use crate::rutas::{self, Idioma, Pagina};

/// Las URL de descarga cuelgan de aquí; las de las páginas de las releases, de `PREFIJO_RELEASES`.
const PREFIJO_DESCARGAS: &str = "https://github.com/HectorEspejo/magi/releases/download/";
const PREFIJO_RELEASES: &str = "https://github.com/HectorEspejo/magi/releases/";
const REPOSITORIO: &str = "HectorEspejo/magi";
const ORDEN_HOMEBREW: &str = "brew install HectorEspejo/tap/magi-ssh";
const ORDEN_AUR: &str = "yay -S magi-ssh-bin";

/// Descargas por canal y plataforma, todas a la vista y en orden (Linux, macOS,
/// Termux y app), sin JavaScript, con versión, estado en texto, tamaño, enlace y
/// SHA-256 completo; bloque «Verificar» por canal; Homebrew y AUR solo si
/// `eleccion.homebrew` / `eleccion.aur`; sin app publicada, «aún sin publicar».
pub fn renderizar(
    e: &Entorno<'_>,
    idioma: Idioma,
    eleccion: &EleccionDescargas,
) -> Resultado<String> {
    let mut contexto = contexto_comun(e, Pagina::Descargas, idioma);
    let raiz = rutas::hasta_raiz(rutas::ruta(Pagina::Descargas, idioma));
    let datos = datos_pagina(e, idioma, eleccion, &raiz)?;
    ampliar(&mut contexto, "d", datos);
    e.plantillas.renderizar("descargas.html", &contexto)
}

/// Un tramo de ruta que el navegador toma por `.` o `..` al normalizar la URL,
/// también escrito con `%2e` (el estándar de URL los trata igual: `%2e%2e`, `.%2e`
/// y `%2e.` suben un nivel como `..`).
fn es_tramo_de_puntos(tramo: &str) -> bool {
    let decodificado = tramo.replace("%2e", ".").replace("%2E", ".");
    decodificado == "." || decodificado == ".."
}

/// Una URL de GitHub con un prefijo fijo y, detrás, solo caracteres de una ruta
/// corriente (nada de comillas, espacios, `<` ni `..`, escrito como se escriba).
fn url_permitida(url: &str, prefijo: &str) -> bool {
    let Some(resto) = url.strip_prefix(prefijo) else {
        return false;
    };
    !resto.is_empty()
        && resto
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._~%+-/".contains(&b))
        && !resto.split('/').any(es_tramo_de_puntos)
}

/// 64 hexadecimales en minúsculas.
fn suma_valida(suma: &str) -> bool {
    suma.len() == 64 && suma.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// 32 pares hexadecimales en mayúsculas separados por `:`.
fn huella_valida(huella: &str) -> bool {
    huella.len() == 95
        && huella.bytes().enumerate().all(|(i, b)| {
            if i % 3 == 2 {
                b == b':'
            } else {
                matches!(b, b'0'..=b'9' | b'A'..=b'F')
            }
        })
}

fn version_del_canal(eleccion: &EleccionDescargas, canal: Canal) -> Option<&VersionElegida> {
    match canal {
        Canal::Escritorio => Some(&eleccion.escritorio),
        Canal::Termux => eleccion.termux.as_ref(),
        Canal::App => eleccion.app.as_ref(),
    }
}

/// Familia de una plataforma: lo que va antes del primer `-` de su id (`linux`,
/// `macos`, `termux`, `android`). Agrupa las cajas de la página.
fn familia(plataforma: &Plataforma) -> &str {
    plataforma
        .id
        .split('-')
        .next()
        .unwrap_or(plataforma.id.as_str())
}

fn vista_version(
    e: &Entorno<'_>,
    idioma: Idioma,
    canal: Canal,
    version: Option<&VersionElegida>,
) -> Resultado<Value> {
    let titulo = texto_necesario(e, idioma, &format!("descargas.canal.{}", canal.id()))?;
    let Some(v) = version else {
        return Ok(json!({
            "canal": canal.id(),
            "titulo": titulo,
            "publicada": false,
            "numero": null,
            "fecha": null,
            "url": null,
        }));
    };
    let donde = format!("descargas (versión {})", v.etiqueta);
    if v.numero.trim().is_empty() {
        return Err(ErrorWeb::dato_raro(
            donde,
            "el número de versión está vacío",
        ));
    }
    if !url_permitida(&v.url, PREFIJO_RELEASES) {
        return Err(ErrorWeb::dato_raro(
            donde,
            format!(
                "la URL de la release «{}» no cuelga de {PREFIJO_RELEASES}",
                v.url
            ),
        ));
    }
    let Some(fecha) = fecha_legible(e.textos, idioma, &v.fecha) else {
        return Err(ErrorWeb::dato_raro(
            donde,
            format!("la fecha «{}» no es AAAA-MM-DD", v.fecha),
        ));
    };
    Ok(json!({
        "canal": canal.id(),
        "titulo": titulo,
        "publicada": true,
        "numero": v.numero,
        "fecha": fecha,
        "url": v.url,
    }))
}

fn vista_archivo(
    a: &ArtefactoElegido,
    plataforma: &Plataforma,
    version: &VersionElegida,
    idioma: Idioma,
) -> Resultado<Value> {
    let donde = format!("descargas (artefacto de {})", plataforma.id);
    if a.canal != plataforma.canal {
        return Err(ErrorWeb::dato_raro(
            donde,
            "el artefacto es de otro canal que su plataforma",
        ));
    }
    if a.etiqueta != version.etiqueta {
        return Err(ErrorWeb::dato_raro(
            donde,
            format!(
                "el artefacto sale de la versión «{}» y no de «{}»",
                a.etiqueta, version.etiqueta
            ),
        ));
    }
    if a.nombre.trim().is_empty() {
        return Err(ErrorWeb::dato_raro(
            donde,
            "el nombre del artefacto está vacío",
        ));
    }
    if !url_permitida(&a.url, PREFIJO_DESCARGAS) {
        return Err(ErrorWeb::dato_raro(
            donde,
            format!("la URL «{}» no cuelga de {PREFIJO_DESCARGAS}", a.url),
        ));
    }
    if !suma_valida(&a.sha256) {
        return Err(ErrorWeb::dato_raro(
            donde,
            "la suma no son 64 hexadecimales en minúsculas",
        ));
    }
    Ok(json!({
        "nombre": a.nombre,
        "url": a.url,
        "suma_url": format!("{}.sha256", a.url),
        "tamano": tamano_legible(a.tamano, idioma),
        "sha256": a.sha256,
    }))
}

fn datos_pagina(
    e: &Entorno<'_>,
    idioma: Idioma,
    eleccion: &EleccionDescargas,
    raiz: &str,
) -> Resultado<Value> {
    // Plataformas por su orden; la página no se fía de que ya vengan ordenadas.
    let mut plataformas: Vec<&Plataforma> = e.plataformas.plataforma.iter().collect();
    plataformas.sort_by(|a, b| a.orden.cmp(&b.orden).then_with(|| a.id.cmp(&b.id)));

    for artefacto in &eleccion.artefactos {
        if e.plataformas.por_id(&artefacto.plataforma).is_none() {
            return Err(ErrorWeb::dato_raro(
                "descargas (elección de versiones)",
                format!(
                    "el artefacto «{}» es de la plataforma «{}», que no está en plataformas.toml",
                    artefacto.nombre, artefacto.plataforma
                ),
            ));
        }
    }

    let mut versiones = Vec::new();
    for canal in Canal::TODOS {
        versiones.push(vista_version(
            e,
            idioma,
            canal,
            version_del_canal(eleccion, canal),
        )?);
    }

    // Cajas: plataformas seguidas de la misma familia y el mismo canal.
    let mut grupos: Vec<Grupo> = Vec::new();
    for plataforma in &plataformas {
        let fam = familia(plataforma);
        let nuevo = match grupos.last() {
            Some(g) => g.familia != fam || g.canal != plataforma.canal,
            None => true,
        };
        if nuevo {
            grupos.push(Grupo {
                familia: fam.to_string(),
                canal: plataforma.canal,
                plataformas: Vec::new(),
            });
        }
        if let Some(g) = grupos.last_mut() {
            g.plataformas.push(plataforma);
        }
    }

    let enlace_probado = format!("{raiz}manual/estado.html");
    let mut vistas_grupos = Vec::new();
    let mut verificar_comandos: Vec<(Canal, Vec<String>)> = Vec::new();
    for grupo in &grupos {
        let version = version_del_canal(eleccion, grupo.canal);
        vistas_grupos.push(vista_grupo(
            e,
            idioma,
            eleccion,
            grupo,
            version,
            raiz,
            &mut verificar_comandos,
        )?);
    }

    let verificar = vista_verificar(e, idioma, eleccion, &plataformas, &verificar_comandos)?;

    Ok(json!({
        "versiones": versiones,
        "grupos": vistas_grupos,
        "verificar": verificar,
        "enlaces": {
            "probado": enlace_probado,
        },
    }))
}

struct Grupo<'a> {
    familia: String,
    canal: Canal,
    plataformas: Vec<&'a Plataforma>,
}

fn vista_grupo(
    e: &Entorno<'_>,
    idioma: Idioma,
    eleccion: &EleccionDescargas,
    grupo: &Grupo<'_>,
    version: Option<&VersionElegida>,
    raiz: &str,
    verificar: &mut Vec<(Canal, Vec<String>)>,
) -> Resultado<Value> {
    let titulo = texto_necesario(e, idioma, &format!("descargas.grupo.{}", grupo.familia))?;

    // ¿Todas las plataformas de la caja se instalan igual? Entonces se dice una vez.
    let mut claves_instrucciones: Vec<&str> = grupo
        .plataformas
        .iter()
        .map(|p| p.instrucciones.as_str())
        .collect();
    claves_instrucciones.sort_unstable();
    claves_instrucciones.dedup();
    let clave_comun = match claves_instrucciones.as_slice() {
        [unica] if version.is_some() => Some(*unica),
        _ => None,
    };
    let comun = clave_comun.is_some();
    let instrucciones_comunes = match clave_comun {
        Some(clave) => Some(texto_necesario(e, idioma, clave)?),
        None => None,
    };

    let mut vistas = Vec::new();
    for plataforma in &grupo.plataformas {
        let etiqueta = texto_necesario(
            e,
            idioma,
            &format!("descargas.plataforma.{}", plataforma.id),
        )?;
        let estado = plataforma.estado.id();
        let estado_texto = texto_necesario(e, idioma, &format!("descargas.estado.{estado}"))?;

        let artefacto = eleccion
            .artefactos
            .iter()
            .find(|a| a.plataforma == plataforma.id);
        let (archivo, numero, comando_instalar, instrucciones) = match (version, artefacto) {
            (Some(v), Some(a)) => {
                let archivo = vista_archivo(a, plataforma, v, idioma)?;
                let orden = if grupo.familia == "termux" {
                    Some(format!("apt install ./{}", a.nombre))
                } else {
                    None
                };
                let instrucciones = if comun {
                    None
                } else {
                    Some(texto_necesario(e, idioma, &plataforma.instrucciones)?)
                };
                verificar_artefacto(verificar, grupo.canal, a);
                (Some(archivo), Some(v.numero.clone()), orden, instrucciones)
            }
            // Sin release del canal (la app aún no ha salido, Termux aún no tiene su .deb).
            (None, None) => (None, None, None, None),
            (Some(v), None) => {
                return Err(ErrorWeb::dato_raro(
                    "descargas (elección de versiones)",
                    format!(
                        "la versión «{}» está elegida pero falta el artefacto de la plataforma «{}»",
                        v.etiqueta, plataforma.id
                    ),
                ));
            }
            (None, Some(a)) => {
                return Err(ErrorWeb::dato_raro(
                    "descargas (elección de versiones)",
                    format!(
                        "el artefacto «{}» no tiene una versión elegida de su canal",
                        a.nombre
                    ),
                ));
            }
        };

        vistas.push(json!({
            "id": plataforma.id,
            "etiqueta": etiqueta,
            "estado": estado,
            "estado_texto": estado_texto,
            "probado": estado != "estable",
            "numero": numero,
            "archivo": archivo,
            "comando_instalar": comando_instalar,
            "instrucciones": instrucciones,
        }));
    }

    // Homebrew y AUR: solo con su variable a `true`.
    let mut alternativas = Vec::new();
    if grupo.familia == "linux" && eleccion.aur {
        alternativas.push(json!({
            "titulo": texto_necesario(e, idioma, "descargas.gestor.aur")?,
            "comando": ORDEN_AUR,
        }));
    }
    if grupo.familia == "macos" && eleccion.homebrew {
        alternativas.push(json!({
            "titulo": texto_necesario(e, idioma, "descargas.gestor.homebrew")?,
            "comando": ORDEN_HOMEBREW,
        }));
    }

    // La app: Obtainium y la huella del certificado, solo si hay release.
    let (obtainium, huella) = if grupo.canal == Canal::App && version.is_some() {
        let huella = &e.plataformas.app.huella_certificado;
        if !huella_valida(huella) {
            return Err(ErrorWeb::dato_raro(
                "web/datos/plataformas.toml",
                "la app tiene una release pero [app] huella_certificado está vacía o no son 32 pares hexadecimales en mayúsculas separados por «:»",
            ));
        }
        (
            json!({
                "etiquetas": e.plataformas.app.obtainium_etiquetas,
                "apk": e.plataformas.app.obtainium_apk,
            }),
            json!(huella),
        )
    } else {
        (Value::Null, Value::Null)
    };

    Ok(json!({
        "id": grupo.familia,
        "titulo": titulo,
        "canal": grupo.canal.id(),
        "capitulo": format!("{raiz}manual/{}.html", grupo.canal.capitulo()),
        "publicada": version.is_some(),
        "instrucciones": instrucciones_comunes,
        "plataformas": vistas,
        "alternativas": alternativas,
        "obtainium": obtainium,
        "huella": huella,
    }))
}

/// Anota los nombres de los artefactos de cada canal, en orden, para el bloque «Verificar».
fn verificar_artefacto(
    verificar: &mut Vec<(Canal, Vec<String>)>,
    canal: Canal,
    artefacto: &ArtefactoElegido,
) {
    match verificar.iter_mut().find(|(c, _)| *c == canal) {
        Some((_, nombres)) => nombres.push(artefacto.nombre.clone()),
        None => verificar.push((canal, vec![artefacto.nombre.clone()])),
    }
}

/// Un bloque «Verificar» por canal con algo publicado.
fn vista_verificar(
    e: &Entorno<'_>,
    idioma: Idioma,
    eleccion: &EleccionDescargas,
    plataformas: &[&Plataforma],
    nombres_por_canal: &[(Canal, Vec<String>)],
) -> Resultado<Vec<Value>> {
    let mut bloques = Vec::new();
    for canal in Canal::TODOS {
        let Some((_, nombres)) = nombres_por_canal.iter().find(|(c, _)| *c == canal) else {
            continue;
        };
        let con_apk = canal == Canal::App;
        let comandos = nombres
            .iter()
            .map(|nombre| {
                let mut lineas = vec![
                    format!("sha256sum -c {nombre}.sha256"),
                    format!("gh attestation verify {nombre} --repo {REPOSITORIO}"),
                ];
                if con_apk {
                    lineas.push(format!("apksigner verify --print-certs {nombre}"));
                }
                lineas.join("\n")
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        // En macOS puede faltar `sha256sum`: la nota va con el canal del escritorio.
        let hay_macos = canal == Canal::Escritorio
            && plataformas.iter().any(|p| {
                p.canal == canal
                    && familia(p) == "macos"
                    && eleccion.artefactos.iter().any(|a| a.plataforma == p.id)
            });
        let nota = if hay_macos {
            json!(texto_necesario(
                e,
                idioma,
                "descargas.verificar.nota_macos"
            )?)
        } else {
            Value::Null
        };
        let huella = if con_apk && version_del_canal(eleccion, canal).is_some() {
            json!(e.plataformas.app.huella_certificado)
        } else {
            Value::Null
        };

        bloques.push(json!({
            "canal": canal.id(),
            "titulo": texto_necesario(
                e,
                idioma,
                &format!("descargas.verificar.canal.{}", canal.id()),
            )?,
            "comandos": comandos,
            "nota": nota,
            "huella": huella,
        }));
    }
    Ok(bloques)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn las_urls_tienen_que_colgar_de_la_release() {
        let buena = format!("{PREFIJO_DESCARGAS}v1.6.1/magi-ssh_1.6.1_aarch64.deb");
        assert!(url_permitida(&buena, PREFIJO_DESCARGAS));
        // Puntos dentro de un nombre corriente no son un tramo de puntos.
        for corriente in [
            "v1/a%2eb",
            "v1/...",
            "v1/a..b",
            "v1/magi-ssh_1.6.1~rc.1_aarch64.deb",
        ] {
            let url = format!("{PREFIJO_DESCARGAS}{corriente}");
            assert!(url_permitida(&url, PREFIJO_DESCARGAS), "{url}");
        }
        for mala in [
            "https://evil.example/releases/download/v1/x".to_string(),
            "http://github.com/HectorEspejo/magi/releases/download/v1/x".to_string(),
            "javascript:alert(1)".to_string(),
            PREFIJO_DESCARGAS.to_string(),
            format!("{PREFIJO_DESCARGAS}v1/a\"onmouseover=\"x"),
            format!("{PREFIJO_DESCARGAS}v1/<x>"),
            format!("{PREFIJO_DESCARGAS}v1/a b"),
            format!("{PREFIJO_DESCARGAS}../../x"),
            // Un navegador normaliza `%2e%2e` como `..` y se saldría de la release.
            format!("{PREFIJO_DESCARGAS}%2e%2e/%2e%2e/otro/x"),
            format!("{PREFIJO_DESCARGAS}%2E%2E/x"),
            format!("{PREFIJO_DESCARGAS}.%2e/x"),
            format!("{PREFIJO_DESCARGAS}%2e./x"),
            format!("{PREFIJO_DESCARGAS}v1/%2e/x"),
            format!("{PREFIJO_DESCARGAS}v1/%2E"),
            format!("{PREFIJO_DESCARGAS}v1/ñ"),
            format!("{PREFIJO_DESCARGAS}v1/a\nb"),
        ] {
            assert!(!url_permitida(&mala, PREFIJO_DESCARGAS), "{mala:?}");
        }
    }

    #[test]
    fn la_suma_son_64_hexadecimales_en_minusculas() {
        assert!(suma_valida(&"a1".repeat(32)));
        assert!(!suma_valida(&"A1".repeat(32)));
        assert!(!suma_valida(&"a1".repeat(31)));
        assert!(!suma_valida(&format!("{}g", "a".repeat(63))));
        assert!(!suma_valida(""));
    }

    #[test]
    fn la_huella_son_32_pares_en_mayusculas() {
        let buena = vec!["AB"; 32].join(":");
        assert!(huella_valida(&buena));
        assert!(!huella_valida(&buena.to_lowercase()));
        assert!(!huella_valida(&buena.replace(':', "-")));
        assert!(!huella_valida(""));
        assert!(!huella_valida(&vec!["AB"; 31].join(":")));
    }
}
