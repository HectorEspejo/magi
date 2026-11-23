//! Coherencia de los workflows de la web (Fase 9c) con lo que dice el informe
//! (§4.1, §5.4, D440, D451 y §8) y con las reglas de `CLAUDE.md`.
//!
//! Sin dependencias nuevas: se leen `.github/workflows/web.yml`, `ci.yml` y
//! `scripts/generar-web.sh` como texto y se analizan por líneas, con la sangría
//! de YAML. Cada comprobación es una función que recibe el texto y devuelve
//! `Result<(), String>`: así las pruebas de aceptación miran los ficheros reales
//! y las pruebas `rechaza_*` demuestran, con copias estropeadas a propósito, que
//! la comprobación de verdad falla cuando el workflow está mal.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Lectura de ficheros
// ---------------------------------------------------------------------------

fn raiz() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn leer(relativa: &str) -> String {
    let ruta = raiz().join(relativa);
    fs::read_to_string(&ruta).unwrap_or_else(|e| panic!("no se puede leer {}: {e}", ruta.display()))
}

fn web_yml() -> String {
    leer(".github/workflows/web.yml")
}

fn ci_yml() -> String {
    leer(".github/workflows/ci.yml")
}

fn script_generar_web() -> String {
    leer("scripts/generar-web.sh")
}

/// Copia de `texto` con `de` sustituido por `a`. Falla si `de` no está: una
/// prueba `rechaza_*` no puede pasar por no haber estropeado nada.
fn mutar(texto: &str, de: &str, a: &str) -> String {
    assert!(
        texto.contains(de),
        "la mutación no encuentra el texto a estropear: {de:?}"
    );
    texto.replacen(de, a, 1)
}

// ---------------------------------------------------------------------------
// Análisis de YAML por sangría (lo justo para estos ficheros)
// ---------------------------------------------------------------------------

fn sangria(linea: &str) -> usize {
    linea.len() - linea.trim_start_matches(' ').len()
}

/// Una línea con contenido: ni vacía ni comentario.
fn relevante(linea: &str) -> bool {
    let t = linea.trim();
    !t.is_empty() && !t.starts_with('#')
}

fn quitar_comillas(valor: &str) -> String {
    let v = valor.trim();
    let b = v.as_bytes();
    if b.len() >= 2 && (b[0] == b'\'' || b[0] == b'"') && b[b.len() - 1] == b[0] {
        v[1..v.len() - 1].to_string()
    } else {
        v.to_string()
    }
}

/// Valor de `clave: valor` en una línea (admite el prefijo `- `).
fn valor_de(linea: &str, clave: &str) -> Option<String> {
    let t = linea.trim_start();
    let t = t.strip_prefix("- ").unwrap_or(t).trim_start();
    let resto = t.strip_prefix(clave)?.strip_prefix(':')?;
    let resto = resto.split(" #").next().unwrap_or(resto);
    Some(quitar_comillas(resto))
}

/// Líneas que cuelgan de una clave de la raíz (`permissions:`, `jobs:`…).
fn bloque_raiz<'a>(texto: &'a str, clave: &str) -> Option<Vec<&'a str>> {
    let mut dentro = false;
    let mut cuerpo = Vec::new();
    for linea in texto.lines() {
        if !dentro {
            if sangria(linea) == 0
                && linea
                    .strip_prefix(clave)
                    .is_some_and(|r| r.starts_with(':'))
            {
                dentro = true;
            }
            continue;
        }
        if relevante(linea) && sangria(linea) == 0 {
            break;
        }
        cuerpo.push(linea);
    }
    dentro.then_some(cuerpo)
}

/// Líneas que cuelgan de `clave:` (sola en su línea) a la sangría dada.
fn subbloque<'a>(lineas: &[&'a str], sangria_clave: usize, clave: &str) -> Option<Vec<&'a str>> {
    let cabecera = format!("{clave}:");
    let pos = lineas
        .iter()
        .position(|l| relevante(l) && sangria(l) == sangria_clave && l.trim() == cabecera)?;
    let mut cuerpo = Vec::new();
    for linea in &lineas[pos + 1..] {
        if relevante(linea) && sangria(linea) <= sangria_clave {
            break;
        }
        cuerpo.push(*linea);
    }
    Some(cuerpo)
}

/// `clave: valor` de un bloque de asignaciones (`permissions`, `env`…), solo
/// del nivel más exterior del bloque.
fn mapa(lineas: &[&str]) -> BTreeMap<String, String> {
    let nivel = lineas
        .iter()
        .find(|l| relevante(l))
        .map(|l| sangria(l))
        .unwrap_or(0);
    let mut m = BTreeMap::new();
    for linea in lineas {
        if !relevante(linea) || sangria(linea) != nivel {
            continue;
        }
        if let Some((clave, valor)) = linea.trim().split_once(':') {
            let valor = valor.split(" #").next().unwrap_or(valor);
            m.insert(clave.trim().to_string(), quitar_comillas(valor));
        }
    }
    m
}

/// Elementos `- valor` de un bloque de lista.
fn lista(lineas: &[&str]) -> Vec<String> {
    lineas
        .iter()
        .filter(|l| relevante(l))
        .filter_map(|l| l.trim().strip_prefix("- "))
        .map(quitar_comillas)
        .collect()
}

/// Los jobs de un workflow, en orden: (id, líneas del cuerpo).
fn trabajos(texto: &str) -> Vec<(String, Vec<&str>)> {
    let cuerpo = bloque_raiz(texto, "jobs").expect("el workflow no tiene `jobs:`");
    let mut res: Vec<(String, Vec<&str>)> = Vec::new();
    for linea in cuerpo {
        if relevante(linea) && sangria(linea) == 2 {
            if let Some(id) = linea.trim().strip_suffix(':') {
                res.push((id.to_string(), Vec::new()));
                continue;
            }
        }
        if let Some(ultimo) = res.last_mut() {
            ultimo.1.push(linea);
        }
    }
    res
}

fn ids_de_trabajos(texto: &str) -> Vec<String> {
    trabajos(texto).into_iter().map(|(id, _)| id).collect()
}

fn trabajo<'a>(texto: &'a str, id: &str) -> Result<Vec<&'a str>, String> {
    trabajos(texto)
        .into_iter()
        .find(|(i, _)| i == id)
        .map(|(_, l)| l)
        .ok_or_else(|| format!("falta el job `{id}`"))
}

/// Valor de una clave a nivel de job (sangría 4): `name`, `if`, `needs`…
fn clave_de_job(job: &[&str], clave: &str) -> Option<String> {
    job.iter()
        .filter(|l| relevante(l) && sangria(l) == 4)
        .find_map(|l| valor_de(l, clave))
}

/// Pasos de un job: cada uno, con sus líneas.
fn pasos<'a>(job: &[&'a str]) -> Vec<Vec<&'a str>> {
    let cuerpo = subbloque(job, 4, "steps").unwrap_or_default();
    let mut res: Vec<Vec<&str>> = Vec::new();
    for linea in cuerpo {
        if relevante(linea) && sangria(linea) == 6 && linea.trim_start().starts_with("- ") {
            res.push(vec![linea]);
        } else if let Some(ultimo) = res.last_mut() {
            ultimo.push(linea);
        }
    }
    res
}

/// Todo lo que sigue a `uses:` en las líneas dadas.
fn usos(lineas: &[&str]) -> Vec<String> {
    lineas
        .iter()
        .filter(|l| relevante(l))
        .filter_map(|l| valor_de(l, "uses"))
        .collect()
}

/// Las líneas de código de los `run:` (inline o de bloque `|` y `>`).
fn lineas_de_run<'a>(lineas: &[&'a str]) -> Vec<&'a str> {
    let mut res = Vec::new();
    let mut i = 0;
    while i < lineas.len() {
        let linea = lineas[i];
        i += 1;
        if !relevante(linea) {
            continue;
        }
        let t = linea.trim_start();
        let t = t.strip_prefix("- ").unwrap_or(t).trim_start();
        let Some(resto) = t.strip_prefix("run:") else {
            continue;
        };
        let resto = resto.trim();
        if resto.starts_with('|') || resto.starts_with('>') {
            let base = sangria(linea);
            while i < lineas.len() && (lineas[i].trim().is_empty() || sangria(lineas[i]) > base) {
                res.push(lineas[i]);
                i += 1;
            }
        } else {
            res.push(resto);
        }
    }
    res
}

fn todas_las_lineas(texto: &str) -> Vec<&str> {
    texto.lines().collect()
}

// ---------------------------------------------------------------------------
// Comprobaciones
// ---------------------------------------------------------------------------

const SI_CONSTRUIR: &str =
    "github.event_name != 'workflow_run' || github.event.workflow_run.conclusion == 'success'";
const SI_DESPLEGAR: &str = "github.ref == 'refs/heads/main'";

/// D451: con `workflow_run`, la web solo se regenera si la release terminó bien.
fn comprobar_solo_si_termino_bien(web: &str) -> Result<(), String> {
    let on = bloque_raiz(web, "on").ok_or("web.yml no tiene `on:`")?;
    let disparo =
        subbloque(&on, 2, "workflow_run").ok_or("web.yml no se dispara con `workflow_run`")?;
    match disparo
        .iter()
        .filter(|l| relevante(l))
        .find_map(|l| valor_de(l, "types"))
    {
        Some(t) if t == "[completed]" => {}
        otro => {
            return Err(format!(
                "`workflow_run` debe tener `types: [completed]`, tiene {otro:?}"
            ))
        }
    }
    if disparo
        .iter()
        .any(|l| relevante(l) && valor_de(l, "branches").is_some())
    {
        return Err(
            "`workflow_run` no lleva filtro de ramas: sus ejecuciones nacen de etiquetas".into(),
        );
    }

    let construir = trabajo(web, "construir")?;
    match clave_de_job(&construir, "if") {
        Some(si) if si == SI_CONSTRUIR => {}
        otro => {
            return Err(format!(
                "el job `construir` debe llevar `if: {SI_CONSTRUIR}`, lleva {otro:?}"
            ))
        }
    }
    let desplegar = trabajo(web, "desplegar")?;
    if clave_de_job(&desplegar, "needs").as_deref() != Some("construir") {
        return Err("`desplegar` debe depender solo de `construir` (`needs: construir`)".into());
    }
    // Nada que reactive un job saltado: sin eso, una release fallida no despliega.
    for linea in todas_las_lineas(web) {
        if let Some(si) = valor_de(linea, "if").filter(|_| relevante(linea)) {
            for prohibido in ["always()", "failure()", "cancelled()"] {
                if si.contains(prohibido) {
                    return Err(format!(
                        "un `if` usa {prohibido}, que reactivaría un job saltado: {si}"
                    ));
                }
            }
        }
    }
    Ok(())
}

/// §8: `contents: read` arriba y lo demás solo en el job que lo necesita.
fn comprobar_permisos(web: &str) -> Result<(), String> {
    let raiz_permisos =
        bloque_raiz(web, "permissions").ok_or("web.yml no declara `permissions:` arriba")?;
    let esperado: BTreeMap<String, String> = [("contents".to_string(), "read".to_string())]
        .into_iter()
        .collect();
    if mapa(&raiz_permisos) != esperado {
        return Err(format!(
            "arriba solo puede ir `contents: read`, hay {:?}",
            mapa(&raiz_permisos)
        ));
    }
    for (id, job) in trabajos(web) {
        let propios = subbloque(&job, 4, "permissions");
        if id == "desplegar" {
            let esperado: BTreeMap<String, String> = [
                ("contents", "read"),
                ("pages", "write"),
                ("id-token", "write"),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
            let tiene = propios.as_deref().map(mapa);
            if tiene.as_ref() != Some(&esperado) {
                return Err(format!(
                    "`desplegar` debe declarar exactamente contents: read, pages: write e id-token: write; declara {tiene:?}"
                ));
            }
            continue;
        }
        // Cualquier otro job: nada de escritura, ni sus permisos ni en ninguna línea.
        if let Some(p) = propios {
            if mapa(&p) != BTreeMap::from([("contents".to_string(), "read".to_string())]) {
                return Err(format!(
                    "el job `{id}` no puede pedir más que contents: read: {:?}",
                    mapa(&p)
                ));
            }
        }
        for linea in &job {
            let t = linea.trim();
            if relevante(linea) && (t.ends_with(": write") || t.contains("write-all")) {
                return Err(format!("el job `{id}` pide un permiso de escritura: {t}"));
            }
        }
    }
    // Los permisos de escritura no pueden estar fuera de un job (p. ej. arriba).
    let fuera_de_jobs: Vec<&str> = web
        .lines()
        .take_while(|l| !(sangria(l) == 0 && l.starts_with("jobs:")))
        .collect();
    for linea in fuera_de_jobs {
        let t = linea.trim();
        if relevante(linea) && (t.ends_with(": write") || t.contains("write-all")) {
            return Err(format!("un permiso de escritura fuera de `desplegar`: {t}"));
        }
    }
    Ok(())
}

/// D451: grupo `pages`, sin cancelar el despliegue en curso.
fn comprobar_concurrencia(web: &str) -> Result<(), String> {
    let bloque = bloque_raiz(web, "concurrency").ok_or("web.yml no declara `concurrency:`")?;
    let esperado: BTreeMap<String, String> = [("group", "pages"), ("cancel-in-progress", "false")]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    if mapa(&bloque) != esperado {
        return Err(format!(
            "la concurrencia debe ser `group: pages` y `cancel-in-progress: false`, es {:?}",
            mapa(&bloque)
        ));
    }
    Ok(())
}

/// Ni `secrets.` ni `secrets: inherit` ni nada con esa palabra, ni en comentarios
/// (los comentarios de la web hablan de «secretos»).
fn comprobar_sin_secrets(nombre: &str, lineas: &[&str]) -> Result<(), String> {
    for (i, linea) in lineas.iter().enumerate() {
        if linea.to_lowercase().contains("secrets") {
            return Err(format!(
                "{nombre}, línea {}: usa `secrets`: {}",
                i + 1,
                linea.trim()
            ));
        }
    }
    Ok(())
}

/// El despliegue solo desde `main` y solo en el job `desplegar`.
fn comprobar_despliegue_solo_desde_main(web: &str) -> Result<(), String> {
    let desplegar = trabajo(web, "desplegar")?;
    match clave_de_job(&desplegar, "if") {
        Some(si) if si == SI_DESPLEGAR => {}
        otro => {
            return Err(format!(
                "`desplegar` debe llevar `if: {SI_DESPLEGAR}`, lleva {otro:?}"
            ))
        }
    }
    let entorno =
        subbloque(&desplegar, 4, "environment").ok_or("`desplegar` no declara `environment:`")?;
    if mapa(&entorno).get("name").map(String::as_str) != Some("github-pages") {
        return Err("el entorno de `desplegar` debe ser `github-pages`".into());
    }
    if !usos(&desplegar)
        .iter()
        .any(|u| u.starts_with("actions/deploy-pages@"))
    {
        return Err("`desplegar` no usa `actions/deploy-pages`".into());
    }
    for (id, job) in trabajos(web) {
        if id == "desplegar" {
            continue;
        }
        let usos_job = usos(&job);
        if usos_job.iter().any(|u| {
            u.starts_with("actions/deploy-pages@") || u.starts_with("actions/configure-pages@")
        }) {
            return Err(format!(
                "el job `{id}` despliega o configura Pages: solo puede `desplegar`"
            ));
        }
        if subbloque(&job, 4, "environment").is_some()
            || clave_de_job(&job, "environment").is_some()
        {
            return Err(format!(
                "el job `{id}` declara un entorno: solo puede `desplegar`"
            ));
        }
    }
    Ok(())
}

/// Nombre (`name:`) de un workflow: la primera línea de la raíz.
fn nombre_de_workflow(texto: &str) -> Option<String> {
    texto
        .lines()
        .filter(|l| sangria(l) == 0 && relevante(l))
        .find_map(|l| valor_de(l, "name"))
}

/// Los nombres de `workflow_run.workflows` son los `name:` de los workflows de
/// release (si no coinciden, GitHub no dispara nada y nadie se entera).
fn comprobar_nombres_de_workflow_run(web: &str, reales: &[String]) -> Result<(), String> {
    let on = bloque_raiz(web, "on").ok_or("web.yml no tiene `on:`")?;
    let disparo =
        subbloque(&on, 2, "workflow_run").ok_or("web.yml no se dispara con `workflow_run`")?;
    let lista_workflows = subbloque(&disparo, 4, "workflows")
        .ok_or("`workflow_run` no tiene `workflows:` como lista en bloque")?;
    let declarados: BTreeSet<String> = lista(&lista_workflows).into_iter().collect();
    let reales: BTreeSet<String> = reales.iter().cloned().collect();
    if declarados.len() != 3 {
        return Err(format!(
            "`workflow_run.workflows` debe tener tres nombres, tiene {declarados:?}"
        ));
    }
    if declarados != reales {
        return Err(format!(
            "`workflow_run.workflows` ({declarados:?}) no coincide con los `name:` de los workflows de release ({reales:?})"
        ));
    }
    Ok(())
}

fn nombres_reales_de_release() -> Vec<String> {
    ["release.yml", "termux-release.yml", "android-release.yml"]
        .iter()
        .map(|f| {
            let texto = leer(&format!(".github/workflows/{f}"));
            nombre_de_workflow(&texto).unwrap_or_else(|| panic!("{f} no tiene `name:`"))
        })
        .collect()
}

/// (versión, suma) de mdBook que declara un bloque de asignaciones `clave: valor`.
fn mdbook_de_mapa(donde: &str, m: &BTreeMap<String, String>) -> Result<(String, String), String> {
    let version = m
        .get("MDBOOK_VERSION")
        .ok_or_else(|| format!("{donde}: falta MDBOOK_VERSION"))?;
    let suma = m
        .get("MDBOOK_SHA256")
        .ok_or_else(|| format!("{donde}: falta MDBOOK_SHA256"))?;
    Ok((version.clone(), suma.clone()))
}

fn mdbook_de_script(script: &str) -> Result<(String, String), String> {
    let valor = |clave: &str| -> Result<String, String> {
        let prefijo = format!("{clave}=");
        script
            .lines()
            .find_map(|l| l.strip_prefix(prefijo.as_str()))
            .map(quitar_comillas)
            .ok_or_else(|| format!("scripts/generar-web.sh: falta {clave}="))
    };
    Ok((valor("MDBOOK_VERSION")?, valor("MDBOOK_SHA256")?))
}

fn es_version(v: &str) -> bool {
    let partes: Vec<&str> = v.split('.').collect();
    partes.len() == 3
        && partes.iter().all(|p| {
            !p.is_empty()
                && p.bytes().all(|b| b.is_ascii_digit())
                && (*p == "0" || !p.starts_with('0'))
        })
}

fn es_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// mdBook, igual en `web.yml`, en el job `web` de `ci.yml` y en el script.
fn comprobar_mdbook(web: &str, ci: &str, script: &str) -> Result<(), String> {
    let env_web = bloque_raiz(web, "env").ok_or("web.yml no tiene `env:` de workflow")?;
    let en_web = mdbook_de_mapa("web.yml", &mapa(&env_web))?;

    let job_web = trabajo(ci, "web")?;
    let env_ci = subbloque(&job_web, 4, "env").ok_or("el job `web` de ci.yml no tiene `env:`")?;
    let en_ci = mdbook_de_mapa("ci.yml (job web)", &mapa(&env_ci))?;
    // Solo en el job web: ni arriba ni en otros jobs de ci.yml.
    let apariciones = ci
        .lines()
        .filter(|l| relevante(l) && l.trim_start().starts_with("MDBOOK_VERSION:"))
        .count();
    if apariciones != 1 {
        return Err(format!(
            "ci.yml declara MDBOOK_VERSION {apariciones} veces: debe ser una, en el job `web`"
        ));
    }

    let en_script = mdbook_de_script(script)?;

    if !es_version(&en_web.0) {
        return Err(format!("MDBOOK_VERSION no es X.Y.Z: {}", en_web.0));
    }
    if !es_sha256(&en_web.1) {
        return Err(format!(
            "MDBOOK_SHA256 no son 64 hexadecimales en minúsculas: {}",
            en_web.1
        ));
    }
    if en_web != en_ci || en_web != en_script {
        return Err(format!(
            "mdBook no está fijado igual: web.yml {en_web:?}, ci.yml {en_ci:?}, scripts/generar-web.sh {en_script:?}"
        ));
    }

    // Los tres comprueban la suma antes de usar el binario y solo por https.
    for (donde, lineas) in [
        ("web.yml", lineas_de_run(&todas_las_lineas(web))),
        ("ci.yml (job web)", lineas_de_run(&job_web)),
        ("scripts/generar-web.sh", todas_las_lineas(script)),
    ] {
        let todo = lineas.join("\n");
        if !todo.contains("sha256sum -c -") {
            return Err(format!(
                "{donde}: no comprueba la suma con `sha256sum -c -`"
            ));
        }
        if !todo.contains("curl --proto '=https' --tlsv1.2") {
            return Err(format!(
                "{donde}: la descarga de mdBook debe ir con `curl --proto '=https' --tlsv1.2`"
            ));
        }
        if !todo.contains("https://github.com/rust-lang/mdBook/releases/download/v") {
            return Err(format!("{donde}: no descarga mdBook de su release oficial"));
        }
    }
    for (donde, lineas) in [
        ("web.yml", lineas_de_run(&todas_las_lineas(web))),
        ("ci.yml (job web)", lineas_de_run(&job_web)),
    ] {
        let todo = lineas.join("\n");
        if !todo.contains("${MDBOOK_SHA256}") || !todo.contains("${MDBOOK_VERSION}") {
            return Err(format!("{donde}: el paso de instalación debe usar las variables MDBOOK_*, no copias de sus valores"));
        }
    }
    Ok(())
}

/// Versiones mayores vigentes de las acciones de Pages (comprobadas con
/// `gh api repos/actions/<acción>/releases/latest` al escribir el workflow).
const ACCIONES_DE_PAGES: [(&str, &str); 3] = [
    ("actions/upload-pages-artifact", "v5"),
    ("actions/configure-pages", "v6"),
    ("actions/deploy-pages", "v5"),
];

/// Todo lo que usa `web.yml`, y nada más.
const ACCIONES_PERMITIDAS_WEB_YML: [&str; 6] = [
    "actions/checkout@v6",
    "dtolnay/rust-toolchain@stable",
    "Swatinem/rust-cache@v2",
    "actions/upload-pages-artifact@v5",
    "actions/configure-pages@v6",
    "actions/deploy-pages@v5",
];

fn comprobar_acciones_de_pages(web: &str) -> Result<(), String> {
    let usadas = usos(&todas_las_lineas(web));
    for (accion, mayor) in ACCIONES_DE_PAGES {
        let con_version = format!("{accion}@{mayor}");
        if !usadas.contains(&con_version) {
            return Err(format!("web.yml debe usar {con_version}"));
        }
        if let Some(otra) = usadas
            .iter()
            .find(|u| u.starts_with(&format!("{accion}@")) && **u != con_version)
        {
            return Err(format!("web.yml usa {otra}, no la vigente {con_version}"));
        }
    }
    for u in &usadas {
        if !ACCIONES_PERMITIDAS_WEB_YML.contains(&u.as_str()) {
            return Err(format!(
                "web.yml usa una acción que no está en la lista permitida: {u} (si es deliberado, añádela a la prueba)"
            ));
        }
    }
    Ok(())
}

/// Los siete jobs de `ci.yml` que dan los ocho checks obligatorios de `main`
/// (el de la matriz sale dos veces): (id, `name:`).
const JOBS_OBLIGATORIOS: [(&str, &str); 7] = [
    ("formato", "Formato"),
    ("clippy", "Clippy"),
    ("tests-linux", "Tests Linux (${{ matrix.os }})"),
    ("macos", "macOS (arm64)"),
    ("visor", "Visor web (TypeScript)"),
    ("deny", "Licencias y avisos (cargo-deny)"),
    ("aur-plantilla", "Plantilla AUR"),
];

/// Los nombres con que salen en la PR los ocho checks de la regla de `main`
/// (`docs/publicar.md` §4).
const CHECKS_OBLIGATORIOS: [&str; 8] = [
    "Formato",
    "Clippy",
    "Tests Linux (ubuntu-22.04)",
    "Tests Linux (ubuntu-24.04)",
    "macOS (arm64)",
    "Visor web (TypeScript)",
    "Licencias y avisos (cargo-deny)",
    "Plantilla AUR",
];

fn comprobar_ocho_checks(ci: &str) -> Result<(), String> {
    let mut nombres_de_check: Vec<String> = Vec::new();
    for (id, nombre) in JOBS_OBLIGATORIOS {
        let job = trabajo(ci, id)?;
        let tiene = clave_de_job(&job, "name");
        if tiene.as_deref() != Some(nombre) {
            return Err(format!("el job `{id}` debe llamarse {nombre:?} (es un check obligatorio), se llama {tiene:?}"));
        }
        // Un check obligatorio no puede quedar condicionado ni encadenado.
        for clave in ["if", "needs"] {
            if clave_de_job(&job, clave).is_some() {
                return Err(format!(
                    "el job obligatorio `{id}` no puede llevar `{clave}:`"
                ));
            }
        }
        if id == "tests-linux" {
            let estrategia =
                subbloque(&job, 4, "strategy").ok_or("`tests-linux` perdió su `strategy:`")?;
            let matriz =
                subbloque(&estrategia, 6, "matrix").ok_or("`tests-linux` perdió su `matrix:`")?;
            let so = matriz
                .iter()
                .filter(|l| relevante(l))
                .find_map(|l| valor_de(l, "os"))
                .ok_or("la matriz de `tests-linux` perdió `os:`")?;
            if so != "[ubuntu-22.04, ubuntu-24.04]" {
                return Err(format!(
                    "la matriz de `tests-linux` debe ser [ubuntu-22.04, ubuntu-24.04], es {so}"
                ));
            }
            for sistema in ["ubuntu-22.04", "ubuntu-24.04"] {
                nombres_de_check.push(nombre.replace("${{ matrix.os }}", sistema));
            }
        } else {
            nombres_de_check.push(nombre.to_string());
        }
    }
    let esperados: Vec<String> = CHECKS_OBLIGATORIOS.iter().map(|s| s.to_string()).collect();
    nombres_de_check.sort();
    let mut ordenados = esperados.clone();
    ordenados.sort();
    if nombres_de_check != ordenados {
        return Err(format!(
            "los checks obligatorios cambiaron: {nombres_de_check:?} frente a {esperados:?}"
        ));
    }
    // Orden de los jobs: los de siempre, intactos y en su sitio, y los de la web al final.
    let ids = ids_de_trabajos(ci);
    let esperado_ids = [
        "formato",
        "clippy",
        "tests-linux",
        "macos",
        "visor",
        "deny",
        "aur-plantilla",
        "cambios-termux",
        "termux",
        "cambios-web",
        "web",
    ];
    if ids.len() < esperado_ids.len() || ids[..esperado_ids.len()] != esperado_ids {
        return Err(format!("el orden de los jobs de ci.yml cambió: {ids:?}"));
    }
    Ok(())
}

/// Palabras que un job de comprobación de la web no puede tener.
const PROHIBIDO_EN_JOBS_WEB_DE_CI: [&str; 7] = [
    "deploy-pages",
    "upload-pages-artifact",
    "configure-pages",
    "id-token",
    "environment",
    "pages:",
    ": write",
];

fn comprobar_jobs_web_de_ci(ci: &str) -> Result<(), String> {
    for (id, job) in [
        ("cambios-web", trabajo(ci, "cambios-web")?),
        ("web", trabajo(ci, "web")?),
    ] {
        for linea in &job {
            for prohibido in PROHIBIDO_EN_JOBS_WEB_DE_CI {
                if relevante(linea) && linea.contains(prohibido) {
                    return Err(format!("el job `{id}` de ci.yml no puede desplegar ni pedir permisos ({prohibido}): {}", linea.trim()));
                }
            }
        }
        let permitidas: &[&str] = if id == "cambios-web" {
            &["actions/checkout@v4"]
        } else {
            &[
                "actions/checkout@v4",
                "dtolnay/rust-toolchain@stable",
                "Swatinem/rust-cache@v2",
            ]
        };
        for u in usos(&job) {
            if !permitidas.contains(&u.as_str()) {
                return Err(format!(
                    "el job `{id}` de ci.yml usa una acción no prevista: {u}"
                ));
            }
        }
    }
    // Nada de Pages en ningún sitio de ci.yml, y permisos arriba solo de lectura.
    for linea in ci.lines().filter(|l| relevante(l)) {
        if linea.contains("deploy-pages") || linea.contains("upload-pages-artifact") {
            return Err(format!("ci.yml no despliega en Pages: {}", linea.trim()));
        }
    }
    let permisos = bloque_raiz(ci, "permissions").ok_or("ci.yml no declara `permissions:`")?;
    if mapa(&permisos) != BTreeMap::from([("contents".to_string(), "read".to_string())]) {
        return Err(format!(
            "ci.yml debe tener solo `contents: read` arriba: {:?}",
            mapa(&permisos)
        ));
    }

    let cambios = trabajo(ci, "cambios-web")?;
    if clave_de_job(&cambios, "name").as_deref() != Some("¿Toca la web?") {
        return Err("el job `cambios-web` debe llamarse «¿Toca la web?»".into());
    }
    if clave_de_job(&cambios, "if").as_deref() != Some("github.event_name == 'pull_request'") {
        return Err("`cambios-web` solo corre en pull requests".into());
    }
    let salidas = subbloque(&cambios, 4, "outputs").ok_or("`cambios-web` no declara `outputs:`")?;
    if !mapa(&salidas).contains_key("web") {
        return Err("`cambios-web` debe tener la salida `web`".into());
    }

    let web = trabajo(ci, "web")?;
    if clave_de_job(&web, "name").as_deref() != Some("Web (construir y comprobar)") {
        return Err("el job `web` debe llamarse «Web (construir y comprobar)»".into());
    }
    if clave_de_job(&web, "needs").as_deref() != Some("cambios-web") {
        return Err("el job `web` debe depender de `cambios-web`".into());
    }
    if clave_de_job(&web, "if").as_deref() != Some("needs.cambios-web.outputs.web == 'true'") {
        return Err("el job `web` solo corre si `cambios-web` dice que la PR toca la web".into());
    }
    if clave_de_job(&web, "timeout-minutes").as_deref() != Some("20") {
        return Err("el job `web` lleva `timeout-minutes: 20`".into());
    }
    // Construye dos veces: el manual de la etiqueta publicada y el de la rama.
    let ordenes: Vec<String> = lineas_de_run(&web)
        .into_iter()
        .map(|l| l.trim().to_string())
        .filter(|l| l.starts_with("scripts/generar-web.sh"))
        .collect();
    let esperadas = [
        "scripts/generar-web.sh",
        "scripts/generar-web.sh --con-manual-de-la-rama",
    ];
    if ordenes != esperadas {
        return Err(format!(
            "el job `web` debe ejecutar {esperadas:?}, ejecuta {ordenes:?}"
        ));
    }
    Ok(())
}

/// Ninguna expresión `${{ }}` dentro de un script: lo de fuera va por `env`.
fn comprobar_sin_expresiones_en_run(nombre: &str, lineas: &[&str]) -> Result<(), String> {
    for linea in lineas_de_run(lineas) {
        if linea.contains("${{") {
            return Err(format!(
                "{nombre}: una expresión `${{{{ }}}}` dentro de `run:` (pásala por `env`): {}",
                linea.trim()
            ));
        }
    }
    Ok(())
}

/// `GH_TOKEN` y `PUBLICAR_*` solo en el `env` del paso que ejecuta el script.
fn comprobar_entorno_del_script(web: &str) -> Result<(), String> {
    let construir = trabajo(web, "construir")?;
    let env_raiz = bloque_raiz(web, "env").ok_or("web.yml no tiene `env:`")?;
    for (clave, _) in mapa(&env_raiz) {
        if clave == "GH_TOKEN" || clave.starts_with("PUBLICAR_") {
            return Err(format!(
                "{clave} no va en el `env` del workflow, sino en el del paso del script"
            ));
        }
    }
    for (id, job) in trabajos(web) {
        if let Some(env_job) = subbloque(&job, 4, "env") {
            if mapa(&env_job).contains_key("GH_TOKEN") {
                return Err(format!(
                    "GH_TOKEN en el `env` del job `{id}`: debe ir en el del paso"
                ));
            }
        }
    }
    let de_paso: Vec<Vec<&str>> = pasos(&construir)
        .into_iter()
        .filter(|p| p.iter().any(|l| l.contains("GH_TOKEN")))
        .collect();
    if de_paso.len() != 1 {
        return Err(format!(
            "GH_TOKEN debe aparecer en un solo paso, aparece en {}",
            de_paso.len()
        ));
    }
    let paso = &de_paso[0];
    let env_paso = subbloque(paso, 8, "env").ok_or("el paso del script no tiene `env:`")?;
    let m = mapa(&env_paso);
    let esperado: BTreeMap<String, String> = [
        ("GH_TOKEN", "${{ github.token }}"),
        ("PUBLICAR_HOMEBREW", "${{ vars.PUBLICAR_HOMEBREW }}"),
        ("PUBLICAR_AUR", "${{ vars.PUBLICAR_AUR }}"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    if m != esperado {
        return Err(format!(
            "el `env` del paso del script debe ser {esperado:?}, es {m:?}"
        ));
    }
    if lineas_de_run(paso)
        .iter()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        != ["scripts/generar-web.sh"]
    {
        return Err("el paso con GH_TOKEN debe ejecutar solo `scripts/generar-web.sh`".into());
    }
    // El artefacto de Pages es el sitio que deja el script, y va después de él.
    let todos = pasos(&construir);
    let pos_script = todos
        .iter()
        .position(|p| {
            lineas_de_run(p)
                .iter()
                .any(|l| l.trim() == "scripts/generar-web.sh")
        })
        .ok_or("`construir` no ejecuta el script")?;
    let pos_subida = todos
        .iter()
        .position(|p| {
            usos(p)
                .iter()
                .any(|u| u.starts_with("actions/upload-pages-artifact@"))
        })
        .ok_or("`construir` no sube el artefacto de Pages")?;
    let pos_mdbook = todos
        .iter()
        .position(|p| lineas_de_run(p).iter().any(|l| l.contains("mdbook")))
        .ok_or("`construir` no instala mdBook")?;
    if !(pos_mdbook < pos_script && pos_script < pos_subida) {
        return Err("el orden de `construir` debe ser: instalar mdBook, ejecutar el script, subir el artefacto".into());
    }
    let ruta = todos[pos_subida]
        .iter()
        .find_map(|l| valor_de(l, "path"))
        .ok_or("la subida a Pages no tiene `path:`")?;
    if ruta != "target/web/sitio" {
        return Err(format!(
            "la subida a Pages debe ser `path: target/web/sitio`, es {ruta:?}"
        ));
    }
    Ok(())
}

/// `push` a `main` con las rutas de la web (y sin el manual) y a mano.
fn comprobar_disparos(web: &str) -> Result<(), String> {
    let on = bloque_raiz(web, "on").ok_or("web.yml no tiene `on:`")?;
    let eventos: BTreeSet<String> = on
        .iter()
        .filter(|l| relevante(l) && sangria(l) == 2)
        .filter_map(|l| l.trim().split(':').next().map(str::to_string))
        .collect();
    let esperados: BTreeSet<String> = ["workflow_run", "push", "workflow_dispatch"]
        .into_iter()
        .map(str::to_string)
        .collect();
    if eventos != esperados {
        return Err(format!(
            "web.yml se dispara con {eventos:?}, debe ser {esperados:?} (nunca con pull_request)"
        ));
    }
    let push = subbloque(&on, 2, "push").ok_or("falta `push`")?;
    let ramas = push
        .iter()
        .filter(|l| relevante(l))
        .find_map(|l| valor_de(l, "branches"));
    if ramas.as_deref() != Some("[main]") {
        return Err(format!("`push` debe ser solo a `main`: {ramas:?}"));
    }
    let rutas = lista(&subbloque(&push, 4, "paths").ok_or("`push` no filtra por rutas")?);
    let esperadas: BTreeSet<String> = [
        "web/**",
        "crates/magi-web/**",
        "assets/demo/**",
        ".github/workflows/web.yml",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    let tiene: BTreeSet<String> = rutas.iter().cloned().collect();
    if tiene != esperadas || rutas.len() != esperadas.len() {
        return Err(format!("las rutas del `push` deben ser {esperadas:?}, son {rutas:?} (el manual sale de la etiqueta, no de main)"));
    }
    Ok(())
}

/// Rutas que hacen correr el job `web` de `ci.yml`.
const RUTAS_QUE_ACTIVAN_LA_WEB: [&str; 11] = [
    "web/*",
    "crates/magi-web/*",
    "assets/demo/*",
    "docs/manual/*",
    "scripts/generar-web.sh",
    "scripts/generar-capturas.sh",
    "tests/capturas_web.rs",
    ".github/workflows/web.yml",
    ".github/workflows/ci.yml",
    "Cargo.toml",
    "Cargo.lock",
];

fn comprobar_filtro_de_cambios_web(ci: &str) -> Result<(), String> {
    let job = trabajo(ci, "cambios-web")?;
    // Sin los comentarios: una ruta que solo se nombra en uno no está vigilada.
    let codigo = lineas_de_run(&job)
        .into_iter()
        .filter(|l| relevante(l))
        .collect::<Vec<_>>()
        .join("\n");
    // Los patrones del `case`, de la línea `case "$fichero" in` a su `esac`.
    let inicio = codigo
        .find("case \"$fichero\" in")
        .ok_or("`cambios-web` no tiene su `case`")?;
    let fin = codigo[inicio..]
        .find("esac")
        .ok_or("el `case` de `cambios-web` no cierra")?
        + inicio;
    let caso = &codigo[inicio..fin];
    let patrones: BTreeSet<&str> = caso
        .split(|c: char| c.is_whitespace() || c == '|' || c == '\\' || c == ')')
        .filter(|t| !t.is_empty())
        .collect();
    for ruta in RUTAS_QUE_ACTIVAN_LA_WEB {
        if !patrones.contains(ruta) {
            return Err(format!("el filtro de `cambios-web` no vigila {ruta}"));
        }
    }
    // Los nombres, separados por NUL (sin `-z`, git entrecomilla los no ASCII), y
    // la lista en un fichero (con `< <(git …)` un fallo de git pasa inadvertido).
    if !codigo.contains("git diff --name-only -z --no-renames HEAD^1 HEAD > \"$cambiados\"") {
        return Err(
            "`cambios-web` debe mirar `git diff --name-only -z --no-renames HEAD^1 HEAD` y dejarlo en un fichero"
                .into(),
        );
    }
    if !codigo.contains("read -r -d ''") || !codigo.contains("done < \"$cambiados\"") {
        return Err("`cambios-web` debe leer la lista con `read -r -d ''` desde el fichero".into());
    }
    if codigo.contains("< <(") {
        return Err("`cambios-web` no puede leer de `< <(…)`: ignora el fallo de git".into());
    }
    if !codigo.contains("web=$cambia") {
        return Err("`cambios-web` debe escribir la salida `web`".into());
    }
    Ok(())
}

fn comprobar_script(script: &str) -> Result<(), String> {
    if script.lines().next() != Some("#!/usr/bin/env bash") {
        return Err("el script debe empezar por `#!/usr/bin/env bash`".into());
    }
    if !script.lines().any(|l| l == "set -euo pipefail") {
        return Err("falta `set -euo pipefail`".into());
    }
    for linea in script.lines().filter(|l| relevante(l)) {
        let t = linea.trim();
        if t == "set -x" || t.starts_with("set -x ") || t.contains("xtrace") {
            return Err(format!(
                "el script no debe activar la traza (`set -x`): {t}"
            ));
        }
        if t.contains("${{") {
            return Err(format!("una expresión de GitHub Actions en el script: {t}"));
        }
    }
    for necesario in [
        "--sin-red",
        "--con-manual-de-la-rama",
        "--ayuda",
        "Uso:",
        "crates/magi-web/tests/fijos/releases/sin-red.json",
        "target/web",
        "sitio",
        "MAGI_WEB_MDBOOK",
        "mktemp -d",
        "trap ",
        "cargo build --release --locked -p magi-web",
        "releases --salida",
        "construir --web web",
        "--animacion assets/demo/magi.gif",
        "--homebrew",
        "--aur",
        "PUBLICAR_HOMEBREW",
        "PUBLICAR_AUR",
        "comprobar",
    ] {
        if !script.contains(necesario) {
            return Err(format!("al script le falta {necesario:?}"));
        }
    }
    // Solo el valor exacto `true` enciende Homebrew y AUR.
    for variable in ["PUBLICAR_HOMEBREW", "PUBLICAR_AUR"] {
        let comparacion = format!("\"${{{variable}:-}}\" == \"true\"");
        if !script.contains(&comparacion) {
            return Err(format!(
                "{variable} debe compararse exactamente con \"true\""
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Pruebas de aceptación (los ficheros reales)
// ---------------------------------------------------------------------------

#[test]
fn web_yml_solo_corre_si_la_release_termino_bien() {
    comprobar_solo_si_termino_bien(&web_yml()).unwrap();
}

#[test]
fn los_permisos_de_web_yml_van_por_job() {
    comprobar_permisos(&web_yml()).unwrap();
}

#[test]
fn web_yml_tiene_concurrencia_pages_sin_cancelar() {
    comprobar_concurrencia(&web_yml()).unwrap();
}

#[test]
fn ningun_workflow_de_la_web_usa_secrets() {
    let web = web_yml();
    comprobar_sin_secrets("web.yml", &todas_las_lineas(&web)).unwrap();
    let ci = ci_yml();
    for id in ["cambios-web", "web"] {
        let job = trabajo(&ci, id).unwrap();
        comprobar_sin_secrets(&format!("ci.yml (job {id})"), &job).unwrap();
    }
    // El script no pide ningún secreto: el token de la API llega por GH_TOKEN.
    let script = script_generar_web();
    comprobar_sin_secrets("scripts/generar-web.sh", &todas_las_lineas(&script)).unwrap();
}

#[test]
fn desplegar_solo_desde_main() {
    comprobar_despliegue_solo_desde_main(&web_yml()).unwrap();
}

#[test]
fn los_nombres_de_workflow_run_coinciden_con_los_name_de_los_workflows() {
    let reales = nombres_reales_de_release();
    assert_eq!(
        reales.iter().collect::<BTreeSet<_>>().len(),
        3,
        "los tres workflows de release deben tener nombres distintos: {reales:?}"
    );
    comprobar_nombres_de_workflow_run(&web_yml(), &reales).unwrap();
}

#[test]
fn mdbook_esta_fijado_igual_en_web_yml_ci_y_el_script() {
    comprobar_mdbook(&web_yml(), &ci_yml(), &script_generar_web()).unwrap();
}

#[test]
fn las_acciones_de_pages_son_las_vigentes() {
    comprobar_acciones_de_pages(&web_yml()).unwrap();
}

#[test]
fn ci_conserva_los_ocho_checks_obligatorios() {
    comprobar_ocho_checks(&ci_yml()).unwrap();
}

#[test]
fn los_jobs_web_de_ci_no_despliegan() {
    comprobar_jobs_web_de_ci(&ci_yml()).unwrap();
}

// Otras comprobaciones de coherencia que no pide el nombre de ninguna prueba de
// aceptación, pero que protegen lo mismo.

#[test]
fn web_yml_se_dispara_con_los_eventos_previstos() {
    comprobar_disparos(&web_yml()).unwrap();
}

#[test]
fn web_yml_pasa_lo_de_fuera_por_env_y_no_por_run() {
    let web = web_yml();
    comprobar_sin_expresiones_en_run("web.yml", &todas_las_lineas(&web)).unwrap();
    let ci = ci_yml();
    for id in ["cambios-web", "web"] {
        comprobar_sin_expresiones_en_run(&format!("ci.yml (job {id})"), &trabajo(&ci, id).unwrap())
            .unwrap();
    }
}

#[test]
fn web_yml_da_el_token_solo_al_paso_del_script_y_sube_el_sitio() {
    comprobar_entorno_del_script(&web_yml()).unwrap();
}

#[test]
fn cambios_web_vigila_las_rutas_de_la_web() {
    comprobar_filtro_de_cambios_web(&ci_yml()).unwrap();
}

#[test]
fn generar_web_sh_cumple_su_contrato() {
    comprobar_script(&script_generar_web()).unwrap();
}

#[cfg(unix)]
#[test]
fn generar_web_sh_es_ejecutable() {
    use std::os::unix::fs::PermissionsExt;
    let ruta = raiz().join("scripts/generar-web.sh");
    let modo = fs::metadata(&ruta).unwrap().permissions().mode();
    assert!(
        modo & 0o111 != 0,
        "{} debe ser ejecutable (modo {modo:o})",
        ruta.display()
    );
}

// ---------------------------------------------------------------------------
// Pruebas del propio análisis: copias estropeadas a propósito deben fallar
// ---------------------------------------------------------------------------

fn debe_fallar(resultado: Result<(), String>, contiene: &str) {
    match resultado {
        Ok(()) => panic!("la comprobación debía fallar (esperaba un error con {contiene:?})"),
        Err(e) => assert!(
            e.contains(contiene),
            "el error no dice lo esperado: {contiene:?} no está en {e:?}"
        ),
    }
}

#[test]
fn rechaza_un_job_construir_sin_condicion_de_exito() {
    let web = mutar(&web_yml(), &format!("    if: {SI_CONSTRUIR}\n"), "");
    debe_fallar(comprobar_solo_si_termino_bien(&web), "`if:");
    // Una condición que no cubre los otros disparos (un `&&`) tampoco vale.
    let web = mutar(
        &web_yml(),
        "github.event_name != 'workflow_run' ||",
        "github.event_name == 'workflow_run' &&",
    );
    debe_fallar(comprobar_solo_si_termino_bien(&web), "`if:");
    let web = mutar(&web_yml(), "types: [completed]", "types: [requested]");
    debe_fallar(comprobar_solo_si_termino_bien(&web), "types");
}

#[test]
fn rechaza_un_if_que_reactiva_un_job_saltado() {
    let web = mutar(
        &web_yml(),
        "if: github.ref == 'refs/heads/main'",
        "if: always() && github.ref == 'refs/heads/main'",
    );
    debe_fallar(comprobar_solo_si_termino_bien(&web), "always()");
}

#[test]
fn rechaza_un_filtro_de_ramas_en_workflow_run() {
    let web = mutar(
        &web_yml(),
        "    types: [completed]\n",
        "    types: [completed]\n    branches: [main]\n",
    );
    debe_fallar(comprobar_solo_si_termino_bien(&web), "ramas");
}

#[test]
fn rechaza_permisos_de_escritura_arriba_o_en_construir() {
    let web = mutar(
        &web_yml(),
        "permissions:\n  contents: read\n\nconcurrency",
        "permissions:\n  contents: read\n  pages: write\n\nconcurrency",
    );
    debe_fallar(comprobar_permisos(&web), "arriba");
    let web = mutar(
        &web_yml(),
        "    timeout-minutes: 20\n",
        "    timeout-minutes: 20\n    permissions:\n      contents: write\n",
    );
    debe_fallar(comprobar_permisos(&web), "construir");
    let web = mutar(&web_yml(), "      id-token: write\n", "");
    debe_fallar(comprobar_permisos(&web), "desplegar");
    let web = mutar(
        &web_yml(),
        "      contents: read\n      pages: write",
        "      pages: write",
    );
    debe_fallar(comprobar_permisos(&web), "desplegar");
}

#[test]
fn rechaza_la_concurrencia_que_cancela_o_cambia_de_grupo() {
    let web = mutar(
        &web_yml(),
        "cancel-in-progress: false",
        "cancel-in-progress: true",
    );
    debe_fallar(comprobar_concurrencia(&web), "concurrencia");
    let web = mutar(&web_yml(), "group: pages", "group: web-${{ github.ref }}");
    debe_fallar(comprobar_concurrencia(&web), "concurrencia");
    let sin = mutar(
        &web_yml(),
        "concurrency:\n  group: pages\n  cancel-in-progress: false\n",
        "",
    );
    debe_fallar(comprobar_concurrencia(&sin), "concurrency");
}

#[test]
fn rechaza_cualquier_uso_de_secrets() {
    for intruso in [
        "          TOKEN: ${{ secrets.PAGES_TOKEN }}\n",
        "    secrets: inherit\n",
        "      - run: echo \"${{ toJSON(secrets) }}\"\n",
    ] {
        let web = format!("{}{intruso}", web_yml());
        debe_fallar(
            comprobar_sin_secrets("web.yml", &todas_las_lineas(&web)),
            "secrets",
        );
    }
}

#[test]
fn rechaza_desplegar_sin_main_o_fuera_de_su_job() {
    let web = mutar(&web_yml(), "    if: github.ref == 'refs/heads/main'\n", "");
    debe_fallar(comprobar_despliegue_solo_desde_main(&web), "main");
    let web = mutar(
        &web_yml(),
        "github.ref == 'refs/heads/main'",
        "github.ref != 'refs/heads/pruebas'",
    );
    debe_fallar(comprobar_despliegue_solo_desde_main(&web), "main");
    // Un segundo job que también despliega.
    let web = web_yml()
        + "\n  otro:\n    runs-on: ubuntu-22.04\n    steps:\n      - uses: actions/deploy-pages@v5\n";
    debe_fallar(comprobar_despliegue_solo_desde_main(&web), "otro");
    let web = mutar(
        &web_yml(),
        "      name: github-pages\n",
        "      name: produccion\n",
    );
    debe_fallar(comprobar_despliegue_solo_desde_main(&web), "github-pages");
}

#[test]
fn rechaza_nombres_de_workflow_run_que_no_existen() {
    let reales = nombres_reales_de_release();
    let web = mutar(
        &web_yml(),
        "      - Release Termux\n",
        "      - Release de Termux\n",
    );
    debe_fallar(
        comprobar_nombres_de_workflow_run(&web, &reales),
        "no coincide",
    );
    let web = mutar(&web_yml(), "      - Release Android\n", "");
    debe_fallar(
        comprobar_nombres_de_workflow_run(&web, &reales),
        "tres nombres",
    );
    // El nombre real de un workflow cambió y web.yml no.
    let mut cambiados = reales.clone();
    cambiados[0] = "Publicar".to_string();
    debe_fallar(
        comprobar_nombres_de_workflow_run(&web_yml(), &cambiados),
        "no coincide",
    );
}

#[test]
fn los_nombres_de_los_workflows_de_release_son_los_esperados() {
    // Si alguno cambia, web.yml deja de dispararse sin avisar: que lo diga esta
    // prueba y no un despliegue que nunca llega.
    assert_eq!(
        nombres_reales_de_release(),
        ["Release", "Release Termux", "Release Android"]
    );
}

#[test]
fn rechaza_un_mdbook_fijado_distinto() {
    let web = web_yml();
    let ci = ci_yml();
    let script = script_generar_web();
    // Otra suma en ci.yml.
    let ci_malo = mutar(
        &ci,
        "MDBOOK_SHA256: '5222beabd3e37dc5be0d18ff99b79058469354db5c220153a1b92db5ba12be89'",
        "MDBOOK_SHA256: '0222beabd3e37dc5be0d18ff99b79058469354db5c220153a1b92db5ba12be89'",
    );
    debe_fallar(
        comprobar_mdbook(&web, &ci_malo, &script),
        "no está fijado igual",
    );
    // Otra versión en el script.
    let script_malo = mutar(&script, "MDBOOK_VERSION=0.5.4", "MDBOOK_VERSION=0.5.3");
    debe_fallar(
        comprobar_mdbook(&web, &ci, &script_malo),
        "no está fijado igual",
    );
    // Una suma que no es una suma.
    let web_malo = mutar(
        &web,
        "MDBOOK_SHA256: '5222beabd3e37dc5be0d18ff99b79058469354db5c220153a1b92db5ba12be89'",
        "MDBOOK_SHA256: '5222BEABD3E37DC5BE0D18FF99B79058469354DB5C220153A1B92DB5BA12BE89'",
    );
    debe_fallar(
        comprobar_mdbook(&web_malo, &ci, &script),
        "64 hexadecimales",
    );
    // Sin comprobar la suma.
    let web_sin_suma = mutar(&web, "| sha256sum -c -", "| cat");
    debe_fallar(
        comprobar_mdbook(&web_sin_suma, &ci, &script),
        "sha256sum -c -",
    );
    // Descarga sin forzar https.
    let ci_http = mutar(
        &ci,
        "curl --proto '=https' --tlsv1.2 -fsSL -o \"${RUNNER_TEMP}/${archivo}\"",
        "curl -fsSL -o \"${RUNNER_TEMP}/${archivo}\"",
    );
    debe_fallar(comprobar_mdbook(&web, &ci_http, &script), "https");
    // Copias de los valores en el paso de instalación en vez de las variables.
    let ci_copia = mutar(
        &ci,
        "echo \"${MDBOOK_SHA256}  ",
        "echo \"5222beabd3e37dc5be0d18ff99b79058469354db5c220153a1b92db5ba12be89  ",
    );
    debe_fallar(
        comprobar_mdbook(&web, &ci_copia, &script),
        "variables MDBOOK_",
    );
}

#[test]
fn rechaza_acciones_de_pages_antiguas_o_desconocidas() {
    let web = mutar(
        &web_yml(),
        "actions/deploy-pages@v5",
        "actions/deploy-pages@v4",
    );
    debe_fallar(comprobar_acciones_de_pages(&web), "deploy-pages");
    let web = mutar(
        &web_yml(),
        "actions/configure-pages@v6",
        "actions/configure-pages@v5",
    );
    debe_fallar(comprobar_acciones_de_pages(&web), "configure-pages");
    let web = mutar(
        &web_yml(),
        "actions/upload-pages-artifact@v5",
        "actions/upload-pages-artifact@main",
    );
    debe_fallar(comprobar_acciones_de_pages(&web), "upload-pages-artifact");
    let web = mutar(
        &web_yml(),
        "      - uses: actions/configure-pages@v6\n",
        "      - uses: actions/configure-pages@v6\n      - uses: alguien/despliega@v1\n",
    );
    debe_fallar(comprobar_acciones_de_pages(&web), "alguien/despliega");
}

#[test]
fn rechaza_que_ci_pierda_o_renombre_un_check_obligatorio() {
    let ci = mutar(
        &ci_yml(),
        "    name: Formato\n",
        "    name: Formato del código\n",
    );
    debe_fallar(comprobar_ocho_checks(&ci), "Formato");
    let ci = mutar(&ci_yml(), "    name: macOS (arm64)\n", "    name: macOS\n");
    debe_fallar(comprobar_ocho_checks(&ci), "macos");
    let ci = mutar(
        &ci_yml(),
        "os: [ubuntu-22.04, ubuntu-24.04]",
        "os: [ubuntu-24.04]",
    );
    debe_fallar(comprobar_ocho_checks(&ci), "matriz");
    // Un job obligatorio condicionado a una ruta deja de ser un check.
    let ci = mutar(
        &ci_yml(),
        "  clippy:\n    name: Clippy\n",
        "  clippy:\n    name: Clippy\n    if: github.event_name == 'pull_request'\n",
    );
    debe_fallar(comprobar_ocho_checks(&ci), "`if:`");
    // Un job de los de siempre desaparece o cambia de sitio.
    let ci = mutar(&ci_yml(), "  cambios-web:\n", "  cambios-webs:\n");
    debe_fallar(comprobar_jobs_web_de_ci(&ci), "cambios-web");
    let ci = mutar(
        &ci_yml(),
        "  termux:\n    name: Paquete de Termux (.deb)\n",
        "  termux2:\n    name: Paquete de Termux (.deb)\n",
    );
    debe_fallar(comprobar_ocho_checks(&ci), "orden");
}

#[test]
fn rechaza_que_los_jobs_web_de_ci_desplieguen_o_pidan_permisos() {
    let ci = mutar(
        &ci_yml(),
        "        run: scripts/generar-web.sh --con-manual-de-la-rama",
        "        run: scripts/generar-web.sh --con-manual-de-la-rama\n      - uses: actions/deploy-pages@v5",
    );
    debe_fallar(comprobar_jobs_web_de_ci(&ci), "deploy-pages");
    let ci = mutar(
        &ci_yml(),
        "    timeout-minutes: 20\n    env:\n      # Los mismos",
        "    timeout-minutes: 20\n    permissions:\n      id-token: write\n    env:\n      # Los mismos",
    );
    debe_fallar(comprobar_jobs_web_de_ci(&ci), "id-token");
    let ci = mutar(
        &ci_yml(),
        "    needs: cambios-web\n    if: needs.cambios-web.outputs.web == 'true'\n",
        "    if: needs.cambios-web.outputs.web == 'true'\n",
    );
    debe_fallar(comprobar_jobs_web_de_ci(&ci), "depender");
    let ci = mutar(
        &ci_yml(),
        "        run: scripts/generar-web.sh --con-manual-de-la-rama",
        "        run: scripts/generar-web.sh",
    );
    debe_fallar(comprobar_jobs_web_de_ci(&ci), "ejecutar");
    let ci = mutar(&ci_yml(), "      - uses: dtolnay/rust-toolchain@stable\n      - uses: Swatinem/rust-cache@v2\n        with:\n          key: web\n", "      - uses: dtolnay/rust-toolchain@stable\n      - uses: extra/accion@v1\n      - uses: Swatinem/rust-cache@v2\n        with:\n          key: web\n");
    debe_fallar(comprobar_jobs_web_de_ci(&ci), "extra/accion");
}

#[test]
fn rechaza_expresiones_dentro_de_run() {
    let web = mutar(
        &web_yml(),
        "        run: scripts/generar-web.sh\n",
        "        run: scripts/generar-web.sh ${{ github.event.workflow_run.head_branch }}\n",
    );
    debe_fallar(
        comprobar_sin_expresiones_en_run("web.yml", &todas_las_lineas(&web)),
        "env",
    );
    // En un bloque `|`, no solo en un `run:` de una línea.
    let web = mutar(
        &web_yml(),
        "          mdbook --version\n",
        "          mdbook --version\n          echo \"${{ github.head_ref }}\"\n",
    );
    debe_fallar(
        comprobar_sin_expresiones_en_run("web.yml", &todas_las_lineas(&web)),
        "env",
    );
}

#[test]
fn rechaza_el_token_fuera_del_paso_del_script() {
    // Arriba, en el `env` del workflow.
    let web = mutar(
        &web_yml(),
        "  RUST_BACKTRACE: 1\n",
        "  RUST_BACKTRACE: 1\n  GH_TOKEN: ${{ github.token }}\n",
    );
    debe_fallar(comprobar_entorno_del_script(&web), "GH_TOKEN");
    // Una variable de publicación que se lee de otro sitio.
    let web = mutar(
        &web_yml(),
        "${{ vars.PUBLICAR_AUR }}",
        "${{ vars.PUBLICAR_HOMEBREW }}",
    );
    debe_fallar(comprobar_entorno_del_script(&web), "env");
    // El sitio que se sube no es el que deja el script.
    let web = mutar(&web_yml(), "path: target/web/sitio", "path: target/web");
    debe_fallar(comprobar_entorno_del_script(&web), "target/web/sitio");
}

#[test]
fn rechaza_disparos_no_previstos() {
    let web = mutar(
        &web_yml(),
        "  workflow_dispatch:\n",
        "  workflow_dispatch:\n  pull_request:\n",
    );
    debe_fallar(comprobar_disparos(&web), "pull_request");
    let web = mutar(
        &web_yml(),
        "      - 'assets/demo/**'\n",
        "      - 'assets/demo/**'\n      - 'docs/manual/**'\n",
    );
    debe_fallar(comprobar_disparos(&web), "manual");
    let web = mutar(
        &web_yml(),
        "branches: [main]",
        "branches: [main, desarrollo]",
    );
    debe_fallar(comprobar_disparos(&web), "main");
}

#[test]
fn rechaza_un_filtro_de_cambios_que_olvida_rutas() {
    let ci = mutar(&ci_yml(), "                | docs/manual/* \\\n", "");
    debe_fallar(comprobar_filtro_de_cambios_web(&ci), "docs/manual/*");
    let ci = mutar(
        &ci_yml(),
        "                | Cargo.lock)\n",
        "                | Cargo.locks)\n",
    );
    debe_fallar(comprobar_filtro_de_cambios_web(&ci), "Cargo.lock");
    let ci = mutar(&ci_yml(), "-z --no-renames HEAD^1 HEAD", "-z HEAD^1 HEAD");
    debe_fallar(comprobar_filtro_de_cambios_web(&ci), "--no-renames");
    let ci = mutar(
        &ci_yml(),
        "--name-only -z --no-renames",
        "--name-only --no-renames",
    );
    debe_fallar(comprobar_filtro_de_cambios_web(&ci), "-z");
    let ci = mutar(&ci_yml(), "read -r -d ''", "read -r");
    debe_fallar(comprobar_filtro_de_cambios_web(&ci), "read -r -d ''");
    let ci = mutar(
        &ci_yml(),
        "done < \"$cambiados\"",
        "done < <(git diff --name-only -z HEAD^1 HEAD)",
    );
    debe_fallar(comprobar_filtro_de_cambios_web(&ci), "read -r -d ''");
    // Una ruta que solo sale en un comentario no cuenta como vigilada.
    let ci = mutar(
        &ci_yml(),
        "                | docs/manual/* \\\n",
        "                # | docs/manual/* \\\n",
    );
    debe_fallar(comprobar_filtro_de_cambios_web(&ci), "docs/manual/*");
}

#[test]
fn rechaza_un_script_sin_contrato() {
    let script = script_generar_web();
    debe_fallar(
        comprobar_script(&mutar(&script, "set -euo pipefail\n", "set -eu\n")),
        "pipefail",
    );
    debe_fallar(
        comprobar_script(&mutar(
            &script,
            "set -euo pipefail\n",
            "set -euo pipefail\nset -x\n",
        )),
        "traza",
    );
    debe_fallar(
        comprobar_script(&mutar(
            &script,
            "\"${PUBLICAR_AUR:-}\" == \"true\"",
            "-n \"${PUBLICAR_AUR:-}\"",
        )),
        "PUBLICAR_AUR",
    );
    // Todas las apariciones: con una sola quedaría el resto y no se notaría.
    debe_fallar(
        comprobar_script(&script.replace("--con-manual-de-la-rama", "--manual-de-rama")),
        "--con-manual-de-la-rama",
    );
    debe_fallar(
        comprobar_script(&script.replacen("#!/usr/bin/env bash", "#!/bin/sh", 1)),
        "bash",
    );
}

#[test]
fn el_analisis_de_yaml_distingue_niveles_y_comentarios() {
    // Guarda del propio analizador: las demás pruebas dependen de esto.
    let texto = "\
name: Ejemplo
# on: comentario
on:
  push:
    branches: [main]
permissions:
  contents: read
jobs:
  uno:
    name: Uno
    if: a == 'x' || b == 'y'
    steps:
      - name: Paso
        run: |
          echo uno
          echo dos
      - run: echo tres
  dos:
    name: Dos # comentario
";
    assert_eq!(nombre_de_workflow(texto).as_deref(), Some("Ejemplo"));
    assert_eq!(ids_de_trabajos(texto), ["uno", "dos"]);
    let uno = trabajo(texto, "uno").unwrap();
    assert_eq!(
        clave_de_job(&uno, "if").as_deref(),
        Some("a == 'x' || b == 'y'")
    );
    assert_eq!(clave_de_job(&uno, "name").as_deref(), Some("Uno"));
    assert_eq!(
        clave_de_job(&trabajo(texto, "dos").unwrap(), "name").as_deref(),
        Some("Dos")
    );
    assert_eq!(
        lineas_de_run(&uno)
            .iter()
            .map(|l| l.trim())
            .collect::<Vec<_>>(),
        ["echo uno", "echo dos", "echo tres"]
    );
    assert_eq!(pasos(&uno).len(), 2);
    assert_eq!(
        mapa(&bloque_raiz(texto, "permissions").unwrap()),
        BTreeMap::from([("contents".to_string(), "read".to_string())])
    );
    assert!(bloque_raiz(texto, "on").is_some());
    assert!(bloque_raiz(texto, "concurrency").is_none());
    assert_eq!(quitar_comillas("'0.5.4'"), "0.5.4");
    assert_eq!(quitar_comillas("a == 'x'"), "a == 'x'");
    assert!(
        es_version("0.5.4")
            && !es_version("0.05.4")
            && !es_version("0.5")
            && !es_version("0.5.4-rc")
    );
    assert!(es_sha256(&"a".repeat(64)) && !es_sha256(&"A".repeat(64)) && !es_sha256("abc"));
}

// ---------------------------------------------------------------------------
// Ejecución real: git y bash de verdad, sin red
// ---------------------------------------------------------------------------
//
// Las comprobaciones de arriba leen el texto de los ficheros; estas ejecutan el
// código que hay en ellos. Un patrón que está escrito pero no casa, o un fallo
// de git que se traga el script, solo se ven así.

#[cfg(unix)]
mod ejecucion_real {
    use super::*;
    use std::path::Path;
    use std::process::{Command, Output};

    /// Un cambio de la PR sobre el repositorio de prueba.
    enum Cambio<'a> {
        /// Crea o sobrescribe un fichero.
        Escribir(&'a str),
        Borrar(&'a str),
        Mover(&'a str, &'a str),
    }

    /// `git` sin la configuración del equipo (firmas, ganchos, editores) y con una
    /// identidad de prueba.
    fn git(dir: &Path, args: &[&str]) -> Output {
        Command::new("git")
            .current_dir(dir)
            .args([
                "-c",
                "user.name=prueba",
                "-c",
                "user.email=prueba@ejemplo.org",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_MERGE_AUTOEDIT", "no")
            .env("HOME", dir)
            .env("XDG_CONFIG_HOME", dir)
            .output()
            .unwrap_or_else(|e| {
                panic!("no se pudo ejecutar git (hace falta para esta prueba): {e}")
            })
    }

    fn git_bien(dir: &Path, args: &[&str]) {
        let salida = git(dir, args);
        assert!(
            salida.status.success(),
            "git {args:?} falló: {}",
            String::from_utf8_lossy(&salida.stderr)
        );
    }

    fn escribir_en(repo: &Path, ruta: &str, contenido: &str) {
        let destino = repo.join(ruta);
        fs::create_dir_all(destino.parent().unwrap()).unwrap();
        fs::write(destino, contenido).unwrap();
    }

    /// Resultado de ejecutar el `run:` de `cambios-web` en un repositorio con esta
    /// forma: `base` en `main`, `cambios` en la rama de la PR y, si `fusion`, un
    /// commit de fusión como el que deja `actions/checkout` en una PR. Con
    /// `git_roto`, `git` falla al calcular la diferencia.
    fn ejecutar_filtro(
        ci: &str,
        base: &[&str],
        cambios: &[Cambio],
        fusion: bool,
        git_roto: bool,
    ) -> (bool, String, String) {
        let temporal = tempfile::tempdir().unwrap();
        let repo = temporal.path().join("repo");
        fs::create_dir(&repo).unwrap();
        git_bien(&repo, &["init", "-q"]);
        git_bien(&repo, &["symbolic-ref", "HEAD", "refs/heads/main"]);
        for ruta in base {
            escribir_en(&repo, ruta, "base\n");
        }
        git_bien(&repo, &["add", "-A"]);
        git_bien(&repo, &["commit", "-q", "-m", "base"]);
        if fusion {
            git_bien(&repo, &["checkout", "-q", "-b", "pr"]);
        }
        for cambio in cambios {
            match cambio {
                Cambio::Escribir(ruta) => escribir_en(&repo, ruta, "cambio\n"),
                Cambio::Borrar(ruta) => fs::remove_file(repo.join(ruta)).unwrap(),
                Cambio::Mover(de, a) => {
                    fs::create_dir_all(repo.join(a).parent().unwrap()).unwrap();
                    fs::rename(repo.join(de), repo.join(a)).unwrap();
                }
            }
        }
        git_bien(&repo, &["add", "-A"]);
        git_bien(&repo, &["commit", "-q", "-m", "pr"]);
        if fusion {
            git_bien(&repo, &["checkout", "-q", "main"]);
            git_bien(
                &repo,
                &["merge", "-q", "--no-ff", "--no-edit", "-m", "fusion", "pr"],
            );
        }

        let job = trabajo(ci, "cambios-web").unwrap();
        let guion = temporal.path().join("filtro.sh");
        fs::write(&guion, lineas_de_run(&job).join("\n")).unwrap();
        let salida = temporal.path().join("salida");
        fs::write(&salida, "").unwrap();
        let runner = temporal.path().join("runner");
        fs::create_dir(&runner).unwrap();

        let mut orden = Command::new("bash");
        orden
            .current_dir(&repo)
            .arg(&guion)
            .env("GITHUB_OUTPUT", &salida)
            .env("RUNNER_TEMP", &runner)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("HOME", &repo);
        if git_roto {
            // Un `git` delante del verdadero que falla solo al calcular la diferencia.
            let real = Command::new("sh")
                .args(["-c", "command -v git"])
                .output()
                .unwrap();
            let real = String::from_utf8(real.stdout).unwrap().trim().to_string();
            let bin = temporal.path().join("bin");
            fs::create_dir(&bin).unwrap();
            let falso = bin.join("git");
            fs::write(
                &falso,
                format!("#!/bin/sh\nif [ \"$1\" = diff ]; then echo 'git roto' >&2; exit 128; fi\nexec '{real}' \"$@\"\n"),
            )
            .unwrap();
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&falso, fs::Permissions::from_mode(0o755)).unwrap();
            let path = format!(
                "{}:{}",
                bin.display(),
                std::env::var("PATH").unwrap_or_default()
            );
            orden.env("PATH", path);
        }
        let resultado = orden
            .output()
            .unwrap_or_else(|e| panic!("no se pudo ejecutar bash: {e}"));
        (
            resultado.status.success(),
            fs::read_to_string(&salida).unwrap(),
            String::from_utf8_lossy(&resultado.stderr).into_owned(),
        )
    }

    /// `web=true` o `web=false` según el filtro (falla si el job falla).
    fn el_filtro_dice(ci: &str, base: &[&str], cambios: &[Cambio]) -> String {
        let (bien, salida, errores) = ejecutar_filtro(ci, base, cambios, true, false);
        assert!(bien, "el `run:` de `cambios-web` falló: {errores}");
        salida.trim().to_string()
    }

    #[test]
    fn cambios_web_corre_el_job_web_para_cada_ruta_vigilada_y_solo_para_ellas() {
        let ci = ci_yml();
        let base = ["src/principal.rs", "web/a.html"];
        // Cada ruta de la lista activa el job, y no por casualidad de otra.
        for patron in RUTAS_QUE_ACTIVAN_LA_WEB {
            let ruta = patron.replace('*', "algo.txt");
            assert_eq!(
                el_filtro_dice(&ci, &base, &[Cambio::Escribir(&ruta)]),
                "web=true",
                "un cambio en {ruta} debía activar el job web"
            );
        }
        // Lo que no toca la web, no lo activa: ni el código del escritorio, ni un
        // directorio que solo se le parece.
        for ruta in [
            "src/principal.rs",
            "docs/fase9c/informe.md",
            "docs/manuales/x.md",
            "webinar/index.html",
            "crates/magi-nucleo/src/lib.rs",
            ".github/workflows/release.yml",
        ] {
            assert_eq!(
                el_filtro_dice(&ci, &base, &[Cambio::Escribir(ruta)]),
                "web=false",
                "un cambio en {ruta} no debía activar el job web"
            );
        }
    }

    #[test]
    fn cambios_web_no_pierde_ficheros_con_nombres_raros() {
        // Sin `-z`, git entrecomilla y escapa estos nombres («"docs/manual/\303\261…"»)
        // y ningún patrón casa: la PR pasaría sin que el job web corriera.
        let ci = ci_yml();
        let base = ["src/principal.rs"];
        for ruta in [
            "docs/manual/ñandú.md",
            "web/mi captura.html",
            "web/con\"comillas\".html",
            "web/con\\barra.html",
            "web/con\ttabulador.html",
        ] {
            assert_eq!(
                el_filtro_dice(&ci, &base, &[Cambio::Escribir(ruta)]),
                "web=true",
                "un cambio en {ruta:?} debía activar el job web"
            );
        }
    }

    #[test]
    fn cambios_web_ve_los_borrados_y_los_movimientos_hacia_fuera() {
        let ci = ci_yml();
        let base = ["src/principal.rs", "web/a.html", "docs/manual/uno.md"];
        assert_eq!(
            el_filtro_dice(&ci, &base, &[Cambio::Borrar("web/a.html")]),
            "web=true"
        );
        // Mover un fichero vigilado fuera de las rutas del filtro: sin
        // `--no-renames`, git solo daría el destino y nadie lo vería.
        assert_eq!(
            el_filtro_dice(&ci, &base, &[Cambio::Mover("web/a.html", "src/a.html")]),
            "web=true"
        );
        assert_eq!(
            el_filtro_dice(
                &ci,
                &base,
                &[Cambio::Mover("docs/manual/uno.md", "docs/otro/uno.md")]
            ),
            "web=true"
        );
    }

    #[test]
    fn cambios_web_sin_commit_de_fusion_asume_que_toca_la_web() {
        let (bien, salida, _) = ejecutar_filtro(
            &ci_yml(),
            &["src/principal.rs"],
            &[Cambio::Escribir("src/otro.rs")],
            false,
            false,
        );
        assert!(bien);
        assert_eq!(salida.trim(), "web=true");
    }

    #[test]
    fn cambios_web_falla_si_git_falla_y_no_dice_que_no_toca_la_web() {
        // Antes, `done < <(git diff …)` ignoraba el fallo de git: la lista salía
        // vacía y el job web se saltaba sin avisar.
        let (bien, salida, _) = ejecutar_filtro(
            &ci_yml(),
            &["src/principal.rs"],
            &[Cambio::Escribir("web/a.html")],
            true,
            true,
        );
        assert!(!bien, "con git roto el job debe fallar, no seguir");
        assert!(
            !salida.contains("web=false"),
            "con git roto no puede escribir web=false: {salida:?}"
        );
    }

    // -----------------------------------------------------------------------
    // scripts/generar-web.sh: lo que no necesita red ni compilar nada
    // -----------------------------------------------------------------------

    fn ruta_del_script() -> PathBuf {
        raiz().join("scripts/generar-web.sh")
    }

    #[test]
    fn generar_web_sh_tiene_sintaxis_valida_y_sus_codigos_de_salida() {
        let sintaxis = Command::new("bash")
            .arg("-n")
            .arg(ruta_del_script())
            .output()
            .unwrap();
        assert!(
            sintaxis.status.success(),
            "bash -n: {}",
            String::from_utf8_lossy(&sintaxis.stderr)
        );
        // La ayuda sale con 0 antes de compilar o tocar la red.
        for opcion in ["--ayuda", "-h"] {
            let ayuda = Command::new("bash")
                .arg(ruta_del_script())
                .arg(opcion)
                .output()
                .unwrap();
            assert_eq!(ayuda.status.code(), Some(0), "{opcion}");
            assert!(String::from_utf8_lossy(&ayuda.stdout).contains("Uso:"));
        }
        // Una opción desconocida es un uso incorrecto (2), también tras una buena,
        // y no hace nada antes de rechazarla.
        for argumentos in [&["--no-existe"][..], &["--sin-red", "--no-existe"][..]] {
            let mal = Command::new("bash")
                .arg(ruta_del_script())
                .args(argumentos)
                .output()
                .unwrap();
            assert_eq!(mal.status.code(), Some(2), "{argumentos:?}");
            assert!(String::from_utf8_lossy(&mal.stderr).contains("opción desconocida"));
        }
    }

    /// Un `mdbook` falso que dice ser `version` (o lo que sea) y su carpeta.
    fn mdbook_falso(dir: &Path, nombre: &str, salida: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        fs::create_dir_all(dir).unwrap();
        let ruta = dir.join(nombre);
        fs::write(&ruta, format!("#!/bin/sh\necho '{salida}'\n")).unwrap();
        fs::set_permissions(&ruta, fs::Permissions::from_mode(0o755)).unwrap();
        ruta
    }

    /// `obtener_mdbook` del script cargado con `source` (no se ejecuta nada más),
    /// con un entorno vacío: sin HOME ni caché, así nunca llega a descargar.
    fn obtener_mdbook(path_delante: &Path, entorno: &[(&str, &Path)]) -> Output {
        let path = format!(
            "{}:{}",
            path_delante.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut orden = Command::new("bash");
        orden
            .args([
                "-c",
                "source \"$1\" && obtener_mdbook && echo \"$mdbook\"",
                "bash",
            ])
            .arg(ruta_del_script())
            .env_clear()
            .env("PATH", path);
        for (clave, valor) in entorno {
            orden.env(clave, valor);
        }
        orden.output().unwrap()
    }

    #[test]
    fn generar_web_sh_elige_el_mdbook_fijado_y_no_descarga_si_no_puede() {
        let fijada = mdbook_de_script(&script_generar_web()).unwrap().0;
        let temporal = tempfile::tempdir().unwrap();
        let en_path = temporal.path().join("path");
        let otro = temporal.path().join("otro");

        // El del PATH, si es exactamente la versión fijada.
        let bueno = mdbook_falso(&en_path, "mdbook", &format!("mdbook v{fijada}"));
        let salida = obtener_mdbook(&en_path, &[]);
        assert!(
            salida.status.success(),
            "{}",
            String::from_utf8_lossy(&salida.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&salida.stdout).trim(),
            bueno.display().to_string()
        );

        // MAGI_WEB_MDBOOK manda sobre el PATH, y avisa si no es la fijada.
        let propio = mdbook_falso(&otro, "mdbook-propio", "mdbook v0.0.1");
        let salida = obtener_mdbook(&en_path, &[("MAGI_WEB_MDBOOK", &propio)]);
        assert!(salida.status.success());
        assert_eq!(
            String::from_utf8_lossy(&salida.stdout).trim(),
            propio.display().to_string()
        );
        assert!(String::from_utf8_lossy(&salida.stderr).contains("no la fijada"));

        // Algo que no es un ejecutable no vale.
        let salida = obtener_mdbook(&en_path, &[("MAGI_WEB_MDBOOK", &otro)]);
        assert_eq!(salida.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&salida.stderr).contains("no es un ejecutable"));

        // Otra versión en el PATH, o una salida que no es exactamente «mdbook vX.Y.Z»
        // (una candidata, texto de más), no se acepta: sin caché ni HOME, falla antes
        // de descargar y nunca usa el binario del PATH.
        for salida_falsa in [
            "mdbook v0.0.1".to_string(),
            format!("mdbook v{fijada}-rc.1"),
            format!("mdbook v{fijada} (otro)"),
            "hola".to_string(),
        ] {
            let malo = temporal.path().join("malo");
            mdbook_falso(&malo, "mdbook", &salida_falsa);
            let salida = obtener_mdbook(&malo, &[]);
            assert_eq!(salida.status.code(), Some(1), "{salida_falsa:?}");
            let errores = String::from_utf8_lossy(&salida.stderr);
            assert!(
                errores.contains("no la fijada"),
                "{salida_falsa:?}: {errores}"
            );
            assert!(errores.contains("HOME"), "{salida_falsa:?}: {errores}");
            assert!(
                !String::from_utf8_lossy(&salida.stdout).contains("malo"),
                "se usó un mdbook que no es el fijado: {salida_falsa:?}"
            );
        }
    }
}
