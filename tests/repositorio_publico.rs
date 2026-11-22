//! Fase 9b: el repositorio es público y esta prueba impide que entren datos
//! reales o referencias rotas (informe §7.6, T143).
//!
//! Recorre los ficheros del árbol de trabajo (los versionados y los nuevos sin
//! ignorar, para que un fichero se compruebe antes de llegar a un commit) y
//! comprueba, sin red y sin dependencias nuevas:
//!
//! - ninguna IPv4 pública fuera de los rangos de documentación;
//! - ninguna ruta `/home/<nombre>` fuera de la lista de nombres de documentación;
//! - ningún correo fuera de la lista (y de los dominios de documentación);
//! - ninguna referencia a `4d3-org` fuera del archivo de diseño;
//! - los enlaces relativos y las anclas de la documentación;
//! - la estructura de la puerta de entrada (README, manual, CHANGELOG, GIF).
//!
//! Cada excepción vive en una lista de este fichero, con su motivo. Si hace
//! falta una nueva, se añade aquí con su motivo: nunca se salta la prueba.

use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::process::Command;

// El algoritmo de anclas de GitHub vive en `magi-web` (una sola copia, D452).
use magi_web::anclas::{encabezados_de, slugs_de};

const RAIZ: &str = env!("CARGO_MANIFEST_DIR");

// ---------------------------------------------------------------------------
// Listas (cada excepción, con su motivo)
// ---------------------------------------------------------------------------

/// Ficheros que no se recorren, por prefijo.
const FUERA: &[(&str, &str)] = &[
    (".git/", "metadatos de git"),
    ("target/", "artefactos de compilación"),
    (
        "vendor/",
        "código de terceros vendorizado (tiene sus propios autores y correos)",
    ),
    (
        ".claude/worktrees/",
        "worktrees locales de las herramientas de desarrollo, no son del repositorio",
    ),
    ("visor/node_modules/", "dependencias de npm, solo en local"),
    (
        "visor/dist/",
        "JavaScript generado y minificado (versiones y URLs de terceros)",
    ),
    ("Cargo.lock", "versiones y sumas generadas"),
    ("visor/package-lock.json", "versiones y sumas generadas"),
    (
        "android/gradle/verification-metadata.xml",
        "sumas generadas",
    ),
    (
        "tests/repositorio_publico.rs",
        "esta prueba contiene los patrones que busca",
    ),
];

/// El archivo de diseño, su historia y lo que lo explica: pueden nombrar
/// `4d3-org` (los datos personales se comprueban en todos igualmente).
const ARCHIVO: &[&str] = &["docs/fase", "docs/magi-maestro.md", "docs/README.md"];

/// Nombres de `/home/<nombre>` que son de documentación o de pruebas.
const RUTAS: &[&str] = &[
    "usuario",
    "usuariob",
    "linuxbrew",
    "deploy",
    "ana",
    "ana2",
    "prueba",
    "prueba-magi",
    "pruebas-magi",
    "pruebas-magix",
    "nadie",
    "x",
    "h",
];

/// Correos exactos permitidos.
const CORREOS: &[&str] = &["hola@4d3.org"];

/// Dominios de correo permitidos (y sus subdominios).
const DOMINIOS: &[(&str, &str)] = &[
    ("ejemplo.org", "dominio de documentación del proyecto"),
    (
        "example.org",
        "dominio reservado para documentación (RFC 2606)",
    ),
    (
        "example.com",
        "dominio reservado para documentación (RFC 2606)",
    ),
    (
        "example.net",
        "dominio reservado para documentación (RFC 2606)",
    ),
    (
        "openssh.com",
        "extensiones del protocolo SSH, p. ej. posix-rename@openssh.com",
    ),
    (
        "users.noreply.github.com",
        "correo de GitHub sin dato personal",
    ),
    ("aur.archlinux.org", "usuario git del servidor de AUR"),
    (
        "github.com",
        "usuario git de GitHub (`git@github.com:…`) y direcciones de GitHub",
    ),
    (
        "axo.dev",
        "identidad del bot de dist que escribe la fórmula en el tap (release.yml)",
    ),
];

/// Excepciones por fichero y regla: `(ruta, regla, motivo)`.
const EXCEPCIONES: &[(&str, &str, &str)] = &[
    (
        "tests/termux_paquete.rs",
        "ruta",
        "fixtures negativos del detector de datos personales de la F17: rutas de otra persona inventadas a propósito",
    ),
    (
        "tests/termux_paquete.rs",
        "correo",
        "fixtures negativos del detector de datos personales de la F17: correos de otra persona inventados a propósito",
    ),
];

const LIMITE_GIF: u64 = 2 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Utilidades
// ---------------------------------------------------------------------------

fn raiz() -> &'static Path {
    Path::new(RAIZ)
}

/// Los ficheros del árbol, relativos a la raíz: versionados y nuevos sin
/// ignorar. Sin git (un tarball de las fuentes), recorre el directorio.
fn ficheros() -> Vec<String> {
    let salida = Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .current_dir(raiz())
        .output();
    let mut lista = match salida {
        Ok(s) if s.status.success() => s
            .stdout
            .split(|b| *b == 0)
            .filter(|r| !r.is_empty())
            .map(|r| String::from_utf8_lossy(r).to_string())
            .collect::<Vec<_>>(),
        _ => {
            let mut v = Vec::new();
            recorrer(raiz(), "", &mut v);
            v
        }
    };
    lista.retain(|ruta| !FUERA.iter().any(|(prefijo, _)| ruta.starts_with(prefijo)));
    // `git ls-files --cached` lista también los borrados sin confirmar.
    lista.retain(|ruta| raiz().join(ruta).is_file());
    lista.sort();
    lista.dedup();
    lista
}

fn recorrer(directorio: &Path, prefijo: &str, salida: &mut Vec<String>) {
    let Ok(entradas) = std::fs::read_dir(directorio) else {
        return;
    };
    for entrada in entradas.flatten() {
        let nombre = entrada.file_name().to_string_lossy().to_string();
        if matches!(nombre.as_str(), ".git" | "target" | "node_modules") {
            continue;
        }
        let ruta = format!("{prefijo}{nombre}");
        let camino = entrada.path();
        if camino.is_dir() {
            recorrer(&camino, &format!("{ruta}/"), salida);
        } else {
            salida.push(ruta);
        }
    }
}

/// El texto de un fichero, o `None` si es binario (un NUL en los primeros
/// 8 KiB).
fn leer_texto(ruta: &str) -> Option<String> {
    let bytes = std::fs::read(raiz().join(ruta)).ok()?;
    if bytes.iter().take(8192).any(|b| *b == 0) {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).to_string())
}

fn excepcion(ruta: &str, regla: &str) -> bool {
    EXCEPCIONES
        .iter()
        .any(|(r, g, _)| *r == ruta && *g == regla)
}

fn en_archivo(ruta: &str) -> bool {
    ARCHIVO.iter().any(|prefijo| ruta.starts_with(prefijo))
}

// ---------------------------------------------------------------------------
// Datos reales
// ---------------------------------------------------------------------------

/// Una IPv4 (cuatro grupos de 1 a 3 dígitos) que no es parte de otro número ni
/// de una versión más larga.
fn ipv4_de(linea: &str) -> Vec<Ipv4Addr> {
    let bytes: Vec<char> = linea.chars().collect();
    let mut halladas = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let inicio = i;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == '.') {
            i += 1;
        }
        let trozo: String = bytes[inicio..i].iter().collect();
        let trozo = trozo.trim_end_matches('.');
        // Un identificador pegado delante (`v1.2.3.4`, `abc1.2.3.4`) no es una IP.
        let pegado_delante =
            inicio > 0 && (bytes[inicio - 1].is_alphanumeric() || bytes[inicio - 1] == '_');
        if pegado_delante {
            continue;
        }
        let partes: Vec<&str> = trozo.split('.').collect();
        if partes.len() == 4
            && partes
                .iter()
                .all(|p| !p.is_empty() && p.len() <= 3 && p.chars().all(|c| c.is_ascii_digit()))
        {
            if let Ok(ip) = trozo.parse::<Ipv4Addr>() {
                halladas.push(ip);
            }
        }
    }
    halladas
}

/// Una IPv4 que se puede alcanzar por Internet (ni privada, ni de bucle, ni de
/// documentación, ni reservada).
fn es_publica(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || o[0] == 0
        || o[0] >= 240
        || (o[0] == 100 && (64..=127).contains(&o[1])))
}

/// Citas de especificaciones (`MS-RDPBCGR 2.2.1.13`): parecen una IP y no lo son.
fn cita_de_especificacion(linea: &str) -> bool {
    linea.contains("MS-RDP") || linea.contains("MS-RDPEGFX") || linea.contains("RFC ")
}

fn rutas_de_usuario(linea: &str) -> Vec<String> {
    let mut halladas = Vec::new();
    for (indice, _) in linea.match_indices("/home/") {
        let resto = &linea[indice + "/home/".len()..];
        let mut nombre = String::new();
        for (n, c) in resto.chars().enumerate() {
            let valido = if n == 0 {
                c.is_ascii_lowercase()
            } else {
                c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-'
            };
            if !valido {
                break;
            }
            nombre.push(c);
        }
        if !nombre.is_empty() {
            halladas.push(nombre);
        }
    }
    halladas
}

fn es_plantilla_systemd(dominio: &str) -> bool {
    // `getty@tty1.service`, `wg-quick@wg0.timer`: el dominio es una instancia.
    [
        ".service", ".timer", ".socket", ".target", ".mount", ".slice", ".path",
    ]
    .iter()
    .any(|sufijo| dominio.ends_with(sufijo))
}

fn correos_de(linea: &str) -> Vec<String> {
    let es_local = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-');
    let es_dominio = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-');
    let mut halladas = Vec::new();
    for (indice, _) in linea.match_indices('@') {
        let local: String = linea[..indice]
            .chars()
            .rev()
            .take_while(|c| es_local(*c))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let dominio: String = linea[indice + 1..]
            .chars()
            .take_while(|c| es_dominio(*c))
            .collect();
        let dominio = dominio.trim_end_matches('.').to_string();
        if local.is_empty() || !dominio.contains('.') || dominio.starts_with('.') {
            continue;
        }
        // Un dominio de correo termina en un TLD en minúsculas: `this@Clase.metodo`
        // (una etiqueta de Kotlin) no es una dirección.
        let tld = dominio.rsplit('.').next().unwrap_or("");
        let es_ip = dominio.parse::<Ipv4Addr>().is_ok();
        if !es_ip && !(tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_lowercase())) {
            continue;
        }
        halladas.push(format!("{local}@{dominio}"));
    }
    halladas
}

fn dominio_permitido(dominio: &str) -> bool {
    if DOMINIOS
        .iter()
        .any(|(d, _)| dominio == *d || dominio.ends_with(&format!(".{d}")))
    {
        return true;
    }
    // `usuario@192.0.2.10` o `usuario@127.0.0.1`: una dirección que no es de
    // Internet. Una IP pública la rechaza la regla de IP.
    dominio
        .parse::<Ipv4Addr>()
        .map(|ip| !es_publica(ip))
        .unwrap_or(false)
}

/// Comprueba el texto de un fichero. Devuelve `(línea, motivo)` de cada fallo.
fn comprobar_texto(ruta: &str, texto: &str) -> Vec<(usize, String)> {
    let mut errores = Vec::new();
    for (n, linea) in texto.lines().enumerate() {
        let n = n + 1;
        if !excepcion(ruta, "ip") {
            for ip in ipv4_de(linea) {
                if es_publica(ip) && !cita_de_especificacion(linea) {
                    errores.push((
                        n,
                        format!("IP pública fuera de los rangos de documentación: {ip}"),
                    ));
                }
            }
        }
        if !excepcion(ruta, "ruta") {
            for nombre in rutas_de_usuario(linea) {
                if !RUTAS.contains(&nombre.as_str()) {
                    errores.push((
                        n,
                        format!("ruta personal /home/{nombre} (usa /home/usuario)"),
                    ));
                }
            }
        }
        if !excepcion(ruta, "correo") {
            for correo in correos_de(linea) {
                let (_, dominio) = correo.split_once('@').unwrap_or(("", ""));
                if !CORREOS.contains(&correo.as_str())
                    && !dominio_permitido(dominio)
                    && !es_plantilla_systemd(dominio)
                {
                    errores.push((n, format!("correo fuera de la lista: {correo}")));
                }
            }
        }
        if linea.contains("4d3-org") && !en_archivo(ruta) && !excepcion(ruta, "4d3-org") {
            errores.push((
                n,
                "referencia a 4d3-org (la organización ya no vale: usa HectorEspejo)".to_string(),
            ));
        }
    }
    errores
}

// ---------------------------------------------------------------------------
// Enlaces y anclas de la documentación
// ---------------------------------------------------------------------------

/// Los ficheros de documentación cuyos enlaces se comprueban.
fn es_documentacion_con_enlaces(ruta: &str) -> bool {
    matches!(
        ruta,
        "README.md"
            | "README.en.md"
            | "CONTRIBUTING.md"
            | "CONTRIBUTING.en.md"
            | "SECURITY.md"
            | "docs/README.md"
    ) || (ruta.starts_with("docs/manual/") && ruta.ends_with(".md"))
        || (ruta.starts_with("docs/publicar") && ruta.ends_with(".md"))
}

/// Los destinos de los enlaces `](destino)` de una línea.
fn destinos_de_enlaces(linea: &str) -> Vec<String> {
    let mut destinos = Vec::new();
    let mut resto = linea;
    while let Some(i) = resto.find("](") {
        let despues = &resto[i + 2..];
        let mut profundidad = 1;
        let mut fin = None;
        for (j, c) in despues.char_indices() {
            match c {
                '(' => profundidad += 1,
                ')' => {
                    profundidad -= 1;
                    if profundidad == 0 {
                        fin = Some(j);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(fin) = fin else { break };
        let bruto = despues[..fin].trim();
        // `[a](url "título")`: solo la URL.
        let destino = bruto.split_whitespace().next().unwrap_or("");
        let destino = destino.trim_start_matches('<').trim_end_matches('>');
        if !destino.is_empty() {
            destinos.push(destino.to_string());
        }
        resto = &despues[fin + 1..];
    }
    destinos
}

fn comprobar_enlaces(ruta: &str, texto: &str) -> Vec<(usize, String)> {
    let mut errores = Vec::new();
    let directorio = Path::new(ruta).parent().unwrap_or(Path::new(""));
    let mut dentro = false;
    for (n, linea) in texto.lines().enumerate() {
        let n = n + 1;
        let l = linea.trim_start();
        if l.starts_with("```") || l.starts_with("~~~") {
            dentro = !dentro;
            continue;
        }
        if dentro {
            continue;
        }
        for destino in destinos_de_enlaces(linea) {
            if destino.starts_with("http://")
                || destino.starts_with("https://")
                || destino.starts_with("mailto:")
            {
                continue;
            }
            let (camino, ancla) = match destino.split_once('#') {
                Some((c, a)) => (c.to_string(), Some(a.to_string())),
                None => (destino.clone(), None),
            };
            let objetivo: PathBuf = if camino.is_empty() {
                PathBuf::from(ruta)
            } else {
                normalizar(&directorio.join(&camino))
            };
            let completa = raiz().join(&objetivo);
            if !completa.exists() {
                errores.push((
                    n,
                    format!(
                        "enlace roto: «{destino}» no existe ({})",
                        objetivo.display()
                    ),
                ));
                continue;
            }
            if let Some(ancla) = ancla {
                if completa.is_file() && objetivo.extension().is_some_and(|e| e == "md") {
                    let contenido = std::fs::read_to_string(&completa).unwrap_or_default();
                    if !slugs_de(&contenido).contains(&ancla.to_lowercase()) {
                        errores.push((
                            n,
                            format!("ancla rota: «{destino}» no tiene el encabezado «{ancla}»"),
                        ));
                    }
                }
            }
        }
    }
    errores
}

/// Quita los `.` y `..` de una ruta relativa sin tocar el disco.
fn normalizar(ruta: &Path) -> PathBuf {
    let mut res = PathBuf::new();
    for componente in ruta.components() {
        match componente {
            std::path::Component::ParentDir => {
                res.pop();
            }
            std::path::Component::CurDir => {}
            otro => res.push(otro),
        }
    }
    res
}

// ---------------------------------------------------------------------------
// Pruebas
// ---------------------------------------------------------------------------

fn informar(titulo: &str, fallos: Vec<String>) {
    assert!(
        fallos.is_empty(),
        "{titulo}: {} fallo(s)\n{}",
        fallos.len(),
        fallos.join("\n")
    );
}

#[test]
fn el_arbol_no_tiene_datos_reales_ni_referencias_a_la_organizacion_anterior() {
    let mut fallos = Vec::new();
    for ruta in ficheros() {
        let Some(texto) = leer_texto(&ruta) else {
            continue;
        };
        for (n, motivo) in comprobar_texto(&ruta, &texto) {
            fallos.push(format!("{ruta}:{n}: {motivo}"));
        }
    }
    informar("datos reales o referencias a 4d3-org", fallos);
}

#[test]
fn un_fichero_con_una_ip_publica_falla_diciendo_fichero_linea_y_motivo() {
    let texto = "Host nuevo\n    HostName 8.8.4.4\n    User usuario\n";
    let errores = comprobar_texto("assets/demo/ejemplo.conf", texto);
    assert_eq!(errores.len(), 1, "{errores:?}");
    assert_eq!(errores[0].0, 2);
    assert!(errores[0].1.contains("IP pública"), "{}", errores[0].1);
    // Las de documentación, las privadas y el bucle local valen.
    let bien = "HostName 192.0.2.10\nHostName 198.51.100.7\nHostName 203.0.113.9\nHostName 10.0.2.2\nHostName 127.0.0.1\n";
    assert!(comprobar_texto("a.conf", bien).is_empty());
    // Una cita de especificación o una versión no son IP.
    assert!(comprobar_texto("a.rs", "// MS-RDPBCGR 2.2.1.13").is_empty());
    assert!(comprobar_texto("a.md", "versión v1.2.3.4").is_empty());
}

#[test]
fn las_rutas_personales_los_correos_y_4d3_org_se_detectan() {
    let ruta = comprobar_texto("a.md", "cd /home/maria/proyectos");
    assert_eq!(ruta.len(), 1);
    assert!(ruta[0].1.contains("/home/maria"));
    assert!(comprobar_texto("a.md", "cd /home/usuario/proyectos").is_empty());

    let correo = comprobar_texto("a.md", "escribe a maria@dominio-real.es");
    assert_eq!(correo.len(), 1);
    assert!(correo[0].1.contains("maria@dominio-real.es"));
    for bien in [
        "hola@4d3.org",
        "usuario@ejemplo.org",
        "usuario@192.0.2.10",
        "ssh usuario@127.0.0.1 -p 12222",
        "git clone git@github.com:HectorEspejo/magi.git",
        "this@MainActivity.startActivity(intent)",
        "posix-rename@openssh.com",
        "getty@tty1.service",
        "12345+usuario@users.noreply.github.com",
    ] {
        assert!(comprobar_texto("a.md", bien).is_empty(), "{bien}");
    }

    let org = comprobar_texto("README.md", "github.com/4d3-org/magi");
    assert_eq!(org.len(), 1);
    assert!(org[0].1.contains("4d3-org"));
    assert!(comprobar_texto("docs/fase9/informe.md", "github.com/4d3-org/magi").is_empty());
    assert!(comprobar_texto("docs/README.md", "4d3-org").is_empty());
}

#[test]
fn los_enlaces_relativos_y_las_anclas_de_la_documentacion_apuntan_a_algo() {
    let mut fallos = Vec::new();
    for ruta in ficheros() {
        if !es_documentacion_con_enlaces(&ruta) {
            continue;
        }
        let Some(texto) = leer_texto(&ruta) else {
            continue;
        };
        for (n, motivo) in comprobar_enlaces(&ruta, &texto) {
            fallos.push(format!("{ruta}:{n}: {motivo}"));
        }
    }
    informar("enlaces de la documentación", fallos);
}

#[test]
fn el_comprobador_de_enlaces_distingue_lo_roto_de_lo_que_existe() {
    // El slug de GitHub: acentos y mayúsculas, signos fuera y `-1` en duplicados.
    let md = "# Título\n\n## El `Include` de `~/.ssh/config`\n\n## Sincronización del inventario\n\n## Configuración\n\n## Configuración\n\n```sh\n# no es un encabezado\n```\n";
    assert_eq!(
        slugs_de(md),
        vec![
            "título",
            "el-include-de-sshconfig",
            "sincronización-del-inventario",
            "configuración",
            "configuración-1"
        ]
    );
    // Un enlace a un fichero inexistente falla con su línea.
    let errores = comprobar_enlaces(
        "README.md",
        "uno\n[roto](docs/no-existe.md)\n[bien](CONTRIBUTING.md#licencia-de-lo-que-aportas)\n",
    );
    assert_eq!(errores.len(), 1, "{errores:?}");
    assert_eq!(errores[0].0, 2);
    // Un ancla que no existe en un fichero que sí falla.
    let errores = comprobar_enlaces(
        "README.md",
        "[a](CONTRIBUTING.md#no-esta-este-encabezado)\n",
    );
    assert_eq!(errores.len(), 1, "{errores:?}");
    // Los bloques de código no cuentan, ni las URLs absolutas.
    let ninguno = comprobar_enlaces(
        "README.md",
        "```\n[x](no-existe.md)\n```\n[web](https://example.org/x)\n",
    );
    assert!(ninguno.is_empty(), "{ninguno:?}");
}

#[test]
fn la_puerta_de_entrada_tiene_la_estructura_que_promete() {
    let mut fallos = Vec::new();
    let leer = |ruta: &str| std::fs::read_to_string(raiz().join(ruta)).unwrap_or_default();

    // README corto y con el mismo esqueleto en los dos idiomas.
    let es = leer("README.md");
    let en = leer("README.en.md");
    if es.lines().count() > 200 {
        fallos.push(format!(
            "README.md tiene {} líneas (200 como mucho)",
            es.lines().count()
        ));
    }
    if en.lines().count() > 200 {
        fallos.push(format!(
            "README.en.md tiene {} líneas (200 como mucho)",
            en.lines().count()
        ));
    }
    let niveles = |t: &str| {
        encabezados_de(t)
            .into_iter()
            .map(|(n, _)| n)
            .collect::<Vec<_>>()
    };
    if niveles(&es) != niveles(&en) {
        fallos.push(
            "README.md y README.en.md no tienen los mismos encabezados (niveles y orden)"
                .to_string(),
        );
    }
    for (nombre, texto) in [("README.md", &es), ("README.en.md", &en)] {
        if texto.lines().next() != Some("# MAGI") {
            fallos.push(format!("{nombre} debe empezar por «# MAGI»"));
        }
        // La animación, con texto alternativo.
        let con_alt = texto.lines().any(|l| {
            l.starts_with("![") && !l.starts_with("![]") && l.contains("assets/demo/magi.gif")
        });
        if !con_alt {
            fallos.push(format!(
                "{nombre} debe llevar la animación assets/demo/magi.gif con texto alternativo"
            ));
        }
    }

    // La animación pesa 2 MiB como mucho.
    match std::fs::metadata(raiz().join("assets/demo/magi.gif")) {
        Ok(m) if m.len() > LIMITE_GIF => fallos.push(format!(
            "assets/demo/magi.gif pesa {} bytes (2 MiB como mucho)",
            m.len()
        )),
        Ok(_) => {}
        Err(_) => fallos
            .push("falta assets/demo/magi.gif (se graba con scripts/grabar-demo.sh)".to_string()),
    }

    // Todos los encabezados `##` del README antiguo están en el manual, salvo
    // los que viven en CONTRIBUTING y en el README corto.
    const ANTIGUOS: &[&str] = &[
        "Requisitos",
        "Instalación",
        "Desarrollo",
        "Primeros pasos",
        "Diagnóstico",
        "El servidor de sesiones",
        "Actualizar MAGI",
        "Pestañas",
        "Tamaño de la terminal",
        "Terminal: paneles, historial y Claude Code",
        "Sesiones persistentes",
        "Archivos",
        "Túneles",
        "Snippets",
        "Deliberación MAGI",
        "Sondeo de flota",
        "Autenticación",
        "Identidades y registro",
        "Primer arranque",
        "Sincronización del inventario",
        "Escritorio remoto",
        "Servicios, contenedores y logs",
        "App Android",
        "MAGI en Termux",
        "Atajos",
        "El `Include` de `~/.ssh/config`",
        "Seguridad",
        "Tests",
        "Autocompletado y man en macOS",
        "Desinstalación",
        "Licencia",
    ];
    let mut manual = String::new();
    if let Ok(entradas) = std::fs::read_dir(raiz().join("docs/manual")) {
        for e in entradas.flatten() {
            if e.path().extension().is_some_and(|x| x == "md") {
                manual.push_str(&std::fs::read_to_string(e.path()).unwrap_or_default());
                manual.push('\n');
            }
        }
    }
    let en_manual: Vec<String> = encabezados_de(&manual)
        .into_iter()
        .filter(|(n, _)| *n == 2)
        .map(|(_, t)| t)
        .collect();
    let en_contributing: Vec<String> = encabezados_de(&leer("CONTRIBUTING.md"))
        .into_iter()
        .map(|(_, t)| t)
        .collect();
    for antiguo in ANTIGUOS {
        let destino_fuera = matches!(*antiguo, "Desarrollo" | "Tests");
        let esta = if destino_fuera {
            en_contributing.iter().any(|t| t == antiguo)
        } else if *antiguo == "Licencia" {
            encabezados_de(&es).iter().any(|(_, t)| t == antiguo)
        } else {
            en_manual.iter().any(|t| t == antiguo)
        };
        if !esta {
            fallos.push(format!("el encabezado «{antiguo}» del README antiguo no está en el manual (ni en CONTRIBUTING o README)"));
        }
    }
    if !raiz().join("docs/manual/estado.md").is_file()
        || !raiz().join("docs/manual/README.md").is_file()
    {
        fallos.push("faltan docs/manual/README.md o docs/manual/estado.md".to_string());
    }
    informar("estructura de la puerta de entrada", fallos);
}

#[test]
fn cada_version_del_changelog_empieza_por_su_resumen_bilingue() {
    let mut fallos = Vec::new();
    for ruta in ["CHANGELOG.md", "android/CHANGELOG.md"] {
        let texto = std::fs::read_to_string(raiz().join(ruta)).unwrap_or_default();
        let lineas: Vec<&str> = texto.lines().collect();
        let mut versiones = 0;
        for (i, l) in lineas.iter().enumerate() {
            let Some(resto) = l.strip_prefix("## [") else {
                continue;
            };
            let Some((version, tras)) = resto.split_once(']') else {
                continue;
            };
            if version == "Sin publicar" {
                continue;
            }
            versiones += 1;
            // `## [X.Y.Z] - AAAA-MM-DD`: de ahí saca dist las notas de la release.
            let fecha = tras.trim().strip_prefix("- ").unwrap_or("");
            let fecha_valida = fecha.len() == 10
                && fecha.chars().enumerate().all(|(k, c)| {
                    if k == 4 || k == 7 {
                        c == '-'
                    } else {
                        c.is_ascii_digit()
                    }
                });
            if !fecha_valida {
                fallos.push(format!(
                    "{ruta}:{}: «## [{version}]» debe seguir con « - AAAA-MM-DD»",
                    i + 1
                ));
            }
            let cabecera = lineas[i + 1..]
                .iter()
                .take(8)
                .copied()
                .collect::<Vec<_>>()
                .join("\n");
            if !cabecera.contains("**Resumen.**") || !cabecera.contains("**Summary.**") {
                fallos.push(format!(
                    "{ruta}:{}: la versión {version} debe empezar por **Resumen.** y **Summary.**",
                    i + 1
                ));
            }
        }
        if versiones == 0 {
            fallos.push(format!("{ruta} no tiene ninguna versión publicada"));
        }
    }
    // La versión del escritorio (sin candidata) tiene su entrada.
    let cargo = std::fs::read_to_string(raiz().join("Cargo.toml")).unwrap_or_default();
    let version = cargo
        .lines()
        .find_map(|l| l.strip_prefix("version = \""))
        .and_then(|l| l.split('"').next())
        .unwrap_or("")
        .to_string();
    if !version.is_empty() && !version.contains('-') {
        let changelog = std::fs::read_to_string(raiz().join("CHANGELOG.md")).unwrap_or_default();
        if !changelog
            .lines()
            .any(|l| l.starts_with(&format!("## [{version}] - ")))
        {
            fallos.push(format!(
                "CHANGELOG.md no tiene «## [{version}] - AAAA-MM-DD» para la versión de Cargo.toml"
            ));
        }
    }
    informar("CHANGELOG", fallos);
}

#[test]
fn la_cinta_de_la_demo_solo_usa_ordenes_de_vhs_y_no_teclas_de_funcion() {
    // VHS no tiene teclas de función (F1…F12): una cinta que las use no se
    // graba (revisión de la 9b, P1). Se comprueba sin VHS ni red.
    const ORDENES: &[&str] = &[
        "Output",
        "Set",
        "Sleep",
        "Type",
        "Hide",
        "Show",
        "Require",
        "Source",
        "Env",
        "Wait",
        "Screenshot",
        "Copy",
        "Paste",
        "Enter",
        "Space",
        "Backspace",
        "Delete",
        "Insert",
        "Ctrl",
        "Alt",
        "Shift",
        "Up",
        "Down",
        "Left",
        "Right",
        "PageUp",
        "PageDown",
        "ScrollUp",
        "ScrollDown",
        "Tab",
        "Escape",
        "End",
        "Home",
    ];
    let cinta = std::fs::read_to_string(raiz().join("assets/demo/magi.tape")).unwrap_or_default();
    assert!(!cinta.is_empty(), "falta assets/demo/magi.tape");
    let mut fallos = Vec::new();
    for (n, linea) in cinta.lines().enumerate() {
        let linea = linea.trim();
        if linea.is_empty() || linea.starts_with('#') {
            continue;
        }
        let orden = linea
            .split(|c: char| c.is_whitespace() || c == '+')
            .next()
            .unwrap_or("");
        if !ORDENES.contains(&orden) {
            fallos.push(format!(
                "assets/demo/magi.tape:{}: «{orden}» no es una orden de VHS (¿una tecla de función?)",
                n + 1
            ));
        }
    }
    informar("cinta de la demo", fallos);
}

#[test]
fn el_script_de_la_demo_para_el_servidor_aislado_sin_preguntar_y_valida_la_cinta() {
    let script = std::fs::read_to_string(raiz().join("scripts/grabar-demo.sh")).unwrap_or_default();
    assert!(!script.is_empty(), "falta scripts/grabar-demo.sh");
    let mut fallos = Vec::new();
    for (n, linea) in script.lines().enumerate() {
        if linea.trim_start().starts_with('#') || !linea.contains("servidor parar") {
            continue;
        }
        // Sin `--si` preguntaría «¿Cerrarlas?» a una entrada que nadie contesta,
        // y sin `</dev/null` leería del terminal de quien lo lanza (P2).
        if !linea.contains("--si") || !linea.contains("</dev/null") {
            fallos.push(format!(
                "scripts/grabar-demo.sh:{}: `servidor parar` sin `--si` y `</dev/null`",
                n + 1
            ));
        }
    }
    if !script.contains("vhs validate") {
        fallos.push("scripts/grabar-demo.sh no valida la cinta con `vhs validate`".to_string());
    }
    informar("script de la demo", fallos);
}
