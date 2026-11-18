//! El paquete `.deb` de Termux (Fase 17, Sprint 3; informe §5.1 y §5.7).
//!
//! Tres grupos:
//!
//! 1. **El `.deb` armado.** Lee el que diga `MAGI_DEB` y comprueba su
//!    estructura: las rutas exactas de `dpkg-deb --contents` (con un respaldo de
//!    `ar` + `tar` + `xz` si no hay `dpkg-deb`), los modos (`bin/magi` en 700),
//!    el `control` (`Architecture: aarch64`, `Suggests: termux-api, openssh`,
//!    ningún `Depends`, `Installed-Size` coherente) y `readelf -d` del binario
//!    extraído. Sin `MAGI_DEB` la prueba se omite con un aviso; con
//!    `MAGI_EXIGIR_DEB=1` falla (lo pone el job `termux` de `ci.yml`). No se usa
//!    `CI`: GitHub la pone en todos los jobs, también en los que nunca tendrán
//!    un `.deb` (desviación 3 de la fase).
//! 2. **El script `scripts/paquete-termux.sh`.** Se prueba con un doble de
//!    `dpkg-deb` (que anota sus argumentos y el árbol que recibe y arma un `.deb`
//!    de verdad con `ar`, `tar` y `xz`) y con ELF mínimos generados aquí: la
//!    versión, la tilde de las candidatas, las bibliotecas del binario, los
//!    fallos y que el resultado es reproducible. Con el `dpkg-deb` real, si lo
//!    hay, se arma además un paquete de verdad.
//! 3. **Los ficheros de publicación**: el disparo de `termux-release.yml` (que no
//!    se dispara con `android-v*`), sus permisos por job y secretos, el nombre
//!    de los ficheros y el paso que falla si un adjunto cambia de nombre (se
//!    ejecuta de verdad, con un `gh` falso), el job y el filtro de rutas de
//!    `ci.yml`, la receta de TUR y que los documentos no lleven datos personales.
//!
//! El nombre del fichero de una candidata lleva el guion de la etiqueta
//! (`magi-ssh_1.6.0-rc.1_aarch64.deb`) aunque el `Version` del `control` lleve la
//! tilde (`1.6.0~rc.1`): GitHub puede cambiar la tilde del nombre de un adjunto
//! por un punto y el `.sha256` dejaría de cuadrar.
//!
//! Todo es de Linux: el script y el doble usan herramientas de GNU.
#![cfg(target_os = "linux")]

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const RAIZ: &str = env!("CARGO_MANIFEST_DIR");
const PREFIJO: &str = "data/data/com.termux/files/usr";
/// Lo único que puede pedir el binario (desviación 1 de la fase).
const BIBLIOTECAS_PERMITIDAS: [&str; 5] =
    ["libc.so", "libm.so", "libdl.so", "liblog.so", "libz.so"];
/// Ficheros del paquete (§5.1) con su modo tal y como los lista `dpkg-deb`.
const FICHEROS: [(&str, &str); 8] = [
    ("-rwx------", "bin/magi"),
    ("-rw-r--r--", "share/man/man1/magi.1.gz"),
    ("-rw-r--r--", "share/bash-completion/completions/magi"),
    ("-rw-r--r--", "share/zsh/site-functions/_magi"),
    ("-rw-r--r--", "share/fish/vendor_completions.d/magi.fish"),
    ("-rw-r--r--", "share/doc/magi-ssh/LICENSE-MIT"),
    ("-rw-r--r--", "share/doc/magi-ssh/LICENSE-APACHE"),
    ("-rw-r--r--", "share/doc/magi-ssh/README.md"),
];
/// Directorios bajo `usr/` (el resto de ancestros se añaden aparte).
const SUBDIRECTORIOS: [&str; 12] = [
    "bin",
    "share",
    "share/bash-completion",
    "share/bash-completion/completions",
    "share/doc",
    "share/doc/magi-ssh",
    "share/fish",
    "share/fish/vendor_completions.d",
    "share/man",
    "share/man/man1",
    "share/zsh",
    "share/zsh/site-functions",
];
/// Herramientas que necesitan el script y el doble (el resto ya viene con el
/// sistema base).
const HERRAMIENTAS_SCRIPT: [&str; 20] = [
    "bash",
    "readelf",
    "gzip",
    "tar",
    "xz",
    "ar",
    "sha256sum",
    "install",
    "find",
    "touch",
    "awk",
    "sort",
    "sed",
    "grep",
    "mktemp",
    "diff",
    "wc",
    "cut",
    "stat",
    "chmod",
];

// ---------------------------------------------------------------------------
// Ayudas comunes
// ---------------------------------------------------------------------------

fn exigir() -> bool {
    std::env::var("MAGI_EXIGIR_DEB").is_ok_and(|valor| valor == "1")
}

/// Omite la prueba con un aviso (solo se ve con `--nocapture`) o, con
/// `MAGI_EXIGIR_DEB=1`, la hace fallar.
fn omitir(motivo: &str) {
    assert!(
        !exigir(),
        "termux_paquete: {motivo} (MAGI_EXIGIR_DEB=1 exige el .deb y las herramientas)"
    );
    eprintln!("termux_paquete: OMITIDA: {motivo}");
}

fn buscar(nombre: &str) -> Option<PathBuf> {
    let camino = std::env::var_os("PATH")?;
    std::env::split_paths(&camino)
        .map(|dir| dir.join(nombre))
        .find(|ruta| ruta.is_file())
}

fn raiz(relativa: &str) -> PathBuf {
    Path::new(RAIZ).join(relativa)
}

fn leer_texto(relativa: &str) -> String {
    fs::read_to_string(raiz(relativa)).unwrap_or_else(|e| panic!("no se pudo leer {relativa}: {e}"))
}

/// Ejecuta una orden que debe salir bien y devuelve su salida estándar.
fn ejecutar(orden: &mut Command) -> String {
    let salida = orden
        .output()
        .unwrap_or_else(|e| panic!("no se pudo lanzar {orden:?}: {e}"));
    assert!(
        salida.status.success(),
        "{orden:?} falló ({}): {}",
        salida.status,
        String::from_utf8_lossy(&salida.stderr)
    );
    String::from_utf8_lossy(&salida.stdout).into_owned()
}

fn bytes_a_texto(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

// ---------------------------------------------------------------------------
// Lo que se espera del paquete (§5.1), escrito aparte del script a propósito
// ---------------------------------------------------------------------------

/// Ruta (como la lista `dpkg-deb`) → modo, de todo lo que debe tener el `.deb`.
fn rutas_esperadas() -> BTreeMap<String, String> {
    let mut esperadas = BTreeMap::new();
    let mut ancestros = vec![String::new()];
    let mut acumulado = String::new();
    for componente in PREFIJO.split('/') {
        if !acumulado.is_empty() {
            acumulado.push('/');
        }
        acumulado.push_str(componente);
        ancestros.push(acumulado.clone());
    }
    for directorio in ancestros {
        let ruta = if directorio.is_empty() {
            "./".to_string()
        } else {
            format!("./{directorio}/")
        };
        esperadas.insert(ruta, "drwxr-xr-x".to_string());
    }
    for sub in SUBDIRECTORIOS {
        esperadas.insert(format!("./{PREFIJO}/{sub}/"), "drwxr-xr-x".to_string());
    }
    for (modo, fichero) in FICHEROS {
        esperadas.insert(format!("./{PREFIJO}/{fichero}"), modo.to_string());
    }
    esperadas
}

// ---------------------------------------------------------------------------
// Lectura de un .deb
// ---------------------------------------------------------------------------

struct Miembro {
    nombre: String,
    datos: Vec<u8>,
}

/// Una entrada de `dpkg-deb --contents`.
#[derive(Debug)]
struct Entrada {
    modo: String,
    propietario: String,
    tamano: u64,
    ruta: String,
}

/// Lee los miembros de un fichero `ar` (el formato del `.deb`).
fn leer_ar(ruta: &Path) -> Vec<Miembro> {
    let bytes =
        fs::read(ruta).unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", ruta.display()));
    assert!(
        bytes.starts_with(b"!<arch>\n"),
        "{} no es un fichero ar (un .deb empieza por !<arch>)",
        ruta.display()
    );
    let mut posicion = 8;
    let mut miembros = Vec::new();
    while posicion < bytes.len() {
        assert!(
            posicion + 60 <= bytes.len(),
            "cabecera ar truncada en {posicion}"
        );
        let cabecera = &bytes[posicion..posicion + 60];
        assert_eq!(
            &cabecera[58..60],
            b"`\n",
            "cabecera ar mal cerrada en {posicion}"
        );
        let nombre = bytes_a_texto(&cabecera[0..16])
            .trim_end_matches([' ', '/'])
            .to_string();
        let tamano: usize = bytes_a_texto(&cabecera[48..58])
            .trim()
            .parse()
            .expect("tamaño de miembro ar no numérico");
        posicion += 60;
        assert!(
            posicion + tamano <= bytes.len(),
            "miembro ar «{nombre}» truncado"
        );
        miembros.push(Miembro {
            nombre,
            datos: bytes[posicion..posicion + tamano].to_vec(),
        });
        posicion = (posicion + tamano + (tamano & 1)).min(bytes.len());
    }
    miembros
}

struct Deb {
    ruta: PathBuf,
    miembros: Vec<Miembro>,
    /// Los miembros escritos a disco, para pasárselos a `tar`.
    temporal: tempfile::TempDir,
}

impl Deb {
    fn abrir(ruta: &Path) -> Self {
        let miembros = leer_ar(ruta);
        let temporal = tempfile::tempdir().expect("directorio temporal");
        for miembro in &miembros {
            fs::write(temporal.path().join(&miembro.nombre), &miembro.datos)
                .expect("miembro del .deb");
        }
        Self {
            ruta: ruta.to_path_buf(),
            miembros,
            temporal,
        }
    }

    fn miembro(&self, nombre: &str) -> PathBuf {
        self.temporal.path().join(nombre)
    }

    /// `dpkg-deb --contents` o, sin `dpkg-deb`, `tar -tv` del `data.tar.xz`.
    fn listado(&self, dpkg_deb: Option<&Path>) -> Vec<Entrada> {
        let texto = match dpkg_deb {
            Some(dpkg) => ejecutar(Command::new(dpkg).arg("--contents").arg(&self.ruta)),
            None => ejecutar(
                Command::new("tar")
                    .arg("-tvJf")
                    .arg(self.miembro("data.tar.xz")),
            ),
        };
        texto
            .lines()
            .map(|linea| {
                let campos: Vec<&str> = linea.split_whitespace().collect();
                assert!(
                    campos.len() >= 6,
                    "línea de contenido no entendida: «{linea}»"
                );
                Entrada {
                    modo: campos[0].to_string(),
                    propietario: campos[1].to_string(),
                    tamano: campos[2]
                        .parse()
                        .unwrap_or_else(|_| panic!("tamaño en «{linea}»")),
                    ruta: campos[5..].join(" "),
                }
            })
            .collect()
    }

    /// El texto del `control`: `dpkg-deb --field` o, sin él, el de `control.tar.xz`.
    fn control(&self, dpkg_deb: Option<&Path>) -> String {
        match dpkg_deb {
            Some(dpkg) => ejecutar(Command::new(dpkg).arg("--field").arg(&self.ruta)),
            None => ejecutar(
                Command::new("tar")
                    .arg("-xJOf")
                    .arg(self.miembro("control.tar.xz"))
                    .arg("./control"),
            ),
        }
    }

    /// Extrae todo el `data.tar.xz` en un directorio nuevo.
    fn extraer(&self) -> tempfile::TempDir {
        let destino = tempfile::tempdir().expect("directorio de extracción");
        ejecutar(
            Command::new("tar")
                .arg("-xJf")
                .arg(self.miembro("data.tar.xz"))
                .arg("-C")
                .arg(destino.path()),
        );
        destino
    }
}

/// Campos del `control`, en orden; las líneas de continuación se unen con `\n`.
fn campos_de_control(texto: &str) -> Vec<(String, String)> {
    let mut campos: Vec<(String, String)> = Vec::new();
    for linea in texto.lines() {
        if linea.starts_with(' ') || linea.starts_with('\t') {
            let (_, valor) = campos.last_mut().expect("continuación sin campo");
            valor.push('\n');
            valor.push_str(linea);
        } else if let Some((nombre, valor)) = linea.split_once(':') {
            campos.push((nombre.to_string(), valor.trim().to_string()));
        } else if !linea.is_empty() {
            panic!("línea del control no entendida: «{linea}»");
        }
    }
    campos
}

/// El nombre del fichero que corresponde a la versión del `control`: la tilde de
/// una candidata se escribe como guion (la etiqueta), no como tilde.
fn nombre_de_fichero(version_deb: &str) -> String {
    format!(
        "magi-ssh_{}_aarch64.deb",
        version_deb.replace("~rc.", "-rc.")
    )
}

fn version_de_debian_valida(version: &str) -> bool {
    let (base, rc) = match version.split_once("~rc.") {
        Some((base, rc)) => (base, Some(rc)),
        None => (version, None),
    };
    let partes: Vec<&str> = base.split('.').collect();
    partes.len() == 3
        && partes.iter().all(|p| {
            !p.is_empty()
                && p.bytes().all(|b| b.is_ascii_digit())
                && (*p == "0" || !p.starts_with('0'))
        })
        && rc.is_none_or(|n| {
            !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) && !n.starts_with('0')
        })
}

// ---------------------------------------------------------------------------
// Las comprobaciones sobre un .deb (las comparten MAGI_DEB y las del script)
// ---------------------------------------------------------------------------

fn comprobar_estructura_ar(deb: &Deb) {
    let nombres: Vec<&str> = deb.miembros.iter().map(|m| m.nombre.as_str()).collect();
    assert_eq!(
        nombres,
        ["debian-binary", "control.tar.xz", "data.tar.xz"],
        "un .deb armado con -Zxz lleva estos tres miembros, en este orden"
    );
    assert_eq!(
        deb.miembros[0].datos, b"2.0\n",
        "debian-binary debe decir 2.0"
    );
}

fn comprobar_rutas_y_modos(entradas: &[Entrada]) {
    let esperado = rutas_esperadas();
    let mut real = BTreeMap::new();
    for entrada in entradas {
        assert!(
            real.insert(entrada.ruta.clone(), entrada.modo.clone())
                .is_none(),
            "la ruta {} sale dos veces",
            entrada.ruta
        );
        assert_eq!(
            entrada.propietario, "root/root",
            "{} debe ser de root:root (--root-owner-group)",
            entrada.ruta
        );
    }
    let faltan: Vec<_> = esperado.keys().filter(|r| !real.contains_key(*r)).collect();
    let sobran: Vec<_> = real.keys().filter(|r| !esperado.contains_key(*r)).collect();
    assert!(
        faltan.is_empty() && sobran.is_empty(),
        "rutas del .deb distintas de §5.1. Faltan: {faltan:?}. Sobran: {sobran:?}"
    );
    for (ruta, modo) in &esperado {
        assert_eq!(&real[ruta], modo, "modo de {ruta}");
    }
    assert_eq!(
        real[&format!("./{PREFIJO}/bin/magi")],
        "-rwx------",
        "bin/magi va en 700, como los paquetes de Termux"
    );
}

/// Comprueba el `control`; devuelve la versión de Debian que declara.
fn comprobar_control(texto: &str, entradas: &[Entrada]) -> String {
    let campos = campos_de_control(texto);
    let valor = |nombre: &str| -> Option<&str> {
        let mut iguales = campos
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case(nombre));
        let primero = iguales.next().map(|(_, v)| v.as_str());
        assert!(
            iguales.next().is_none(),
            "el campo {nombre} del control sale dos veces"
        );
        primero
    };
    assert_eq!(valor("Package"), Some("magi-ssh"));
    assert_eq!(
        valor("Architecture"),
        Some("aarch64"),
        "el paquete es solo aarch64"
    );
    assert_eq!(valor("Maintainer"), Some("4d3"));
    assert_eq!(valor("Suggests"), Some("termux-api, openssh"));
    assert_eq!(valor("Homepage"), Some("https://github.com/4d3-org/magi"));
    assert!(
        valor("Description").is_some_and(|d| !d.lines().next().unwrap_or("").is_empty()),
        "falta la Description"
    );
    for prohibido in ["Depends", "Pre-Depends"] {
        assert!(
            valor(prohibido).is_none(),
            "el control no debe llevar {prohibido}: el binario solo pide bibliotecas del NDK estable"
        );
    }
    let version = valor("Version").expect("falta Version").to_string();
    assert!(
        version_de_debian_valida(&version),
        "Version «{version}» no es X.Y.Z ni X.Y.Z~rc.N"
    );

    // Installed-Size, en KiB: la suma de los ficheros (los directorios no
    // pesan en el cálculo del script; se admite un margen por si algún día se
    // calcula con du).
    let declarado: u64 = valor("Installed-Size")
        .expect("falta Installed-Size")
        .parse()
        .expect("Installed-Size no es un número");
    let bytes: u64 = entradas
        .iter()
        .filter(|e| e.modo.starts_with('-'))
        .map(|e| e.tamano)
        .sum();
    let minimo = bytes.div_ceil(1024);
    assert!(
        (minimo..=minimo + 64).contains(&declarado),
        "Installed-Size {declarado} KiB no cuadra con los {minimo} KiB de los ficheros"
    );
    version
}

fn comprobar_bibliotecas(binario: &Path) {
    assert!(buscar("readelf").is_some(), "readelf no está en el PATH");
    let cabecera = ejecutar(Command::new("readelf").arg("-h").arg(binario));
    assert!(
        cabecera
            .lines()
            .any(|l| l.trim().starts_with("Machine:") && l.contains("AArch64")),
        "el binario del .deb no es aarch64:\n{cabecera}"
    );
    let dinamica = ejecutar(Command::new("readelf").arg("-d").arg(binario));
    let pedidas: Vec<String> = dinamica
        .lines()
        .filter(|l| l.contains("(NEEDED)"))
        .filter_map(|l| {
            let inicio = l.find('[')? + 1;
            let fin = l.rfind(']')?;
            Some(l[inicio..fin].to_string())
        })
        .collect();
    assert!(
        pedidas.iter().any(|b| b == "libc.so"),
        "el binario no pide libc.so: {pedidas:?}"
    );
    let fuera: Vec<&String> = pedidas
        .iter()
        .filter(|b| !BIBLIOTECAS_PERMITIDAS.contains(&b.as_str()))
        .collect();
    assert!(
        fuera.is_empty(),
        "readelf -d pide bibliotecas fuera de {BIBLIOTECAS_PERMITIDAS:?}: {fuera:?} \
         (con libc++_shared.so el paquete tendría que depender de libc++)"
    );
}

/// El contenido instalado coincide con lo versionado en el repositorio.
fn comprobar_contenido(extraido: &Path) {
    let usr = extraido.join(PREFIJO);
    let gunzip = ejecutar(
        Command::new("gzip")
            .arg("-dc")
            .arg(usr.join("share/man/man1/magi.1.gz")),
    );
    assert_eq!(
        gunzip,
        leer_texto("assets/man/magi.1"),
        "la página man no es la de assets/"
    );
    for (instalado, origen) in [
        (
            "share/bash-completion/completions/magi",
            "assets/completions/magi.bash",
        ),
        ("share/zsh/site-functions/_magi", "assets/completions/_magi"),
        (
            "share/fish/vendor_completions.d/magi.fish",
            "assets/completions/magi.fish",
        ),
        ("share/doc/magi-ssh/LICENSE-MIT", "LICENSE-MIT"),
        ("share/doc/magi-ssh/LICENSE-APACHE", "LICENSE-APACHE"),
    ] {
        assert_eq!(
            fs::read_to_string(usr.join(instalado)).unwrap_or_else(|e| panic!("{instalado}: {e}")),
            leer_texto(origen),
            "{instalado} no coincide con {origen}"
        );
    }
    let readme =
        fs::read_to_string(usr.join("share/doc/magi-ssh/README.md")).expect("README del .deb");
    assert!(
        readme.starts_with("# MAGI"),
        "el README del .deb no es el de MAGI"
    );
    let mode = fs::metadata(usr.join("bin/magi"))
        .expect("bin/magi")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o700, "bin/magi extraído");
}

/// Todo lo anterior sobre un `.deb`. `dpkg_deb`: el real, si lo hay.
fn comprobar_todo(ruta: &Path, dpkg_deb: Option<&Path>) -> String {
    let deb = Deb::abrir(ruta);
    comprobar_estructura_ar(&deb);
    let entradas = deb.listado(dpkg_deb);
    comprobar_rutas_y_modos(&entradas);
    let version = comprobar_control(&deb.control(dpkg_deb), &entradas);
    let nombre = ruta
        .file_name()
        .and_then(|n| n.to_str())
        .expect("nombre del .deb");
    assert_eq!(
        nombre,
        nombre_de_fichero(&version),
        "el fichero se llama magi-ssh_<versión>_aarch64.deb (con guion en las candidatas)"
    );
    let extraido = deb.extraer();
    comprobar_bibliotecas(&extraido.path().join(PREFIJO).join("bin/magi"));
    comprobar_contenido(extraido.path());
    version
}

// ---------------------------------------------------------------------------
// Grupo 1: el .deb de MAGI_DEB
// ---------------------------------------------------------------------------

/// El `.deb` que dice `MAGI_DEB`. Sin él, omite (o falla con `MAGI_EXIGIR_DEB=1`).
fn deb_de_la_variable() -> Option<PathBuf> {
    match std::env::var_os("MAGI_DEB") {
        Some(ruta) if !ruta.is_empty() => {
            let ruta = PathBuf::from(ruta);
            assert!(
                ruta.is_file(),
                "MAGI_DEB apunta a {}, que no es un fichero",
                ruta.display()
            );
            Some(ruta)
        }
        _ => {
            omitir("MAGI_DEB no apunta a ningún .deb: no hay paquete que comprobar");
            None
        }
    }
}

#[test]
fn el_deb_tiene_la_estructura_de_un_deb_con_xz() {
    let Some(ruta) = deb_de_la_variable() else {
        return;
    };
    comprobar_estructura_ar(&Deb::abrir(&ruta));
}

#[test]
fn el_deb_lista_exactamente_las_rutas_y_modos_del_informe() {
    let Some(ruta) = deb_de_la_variable() else {
        return;
    };
    let dpkg = buscar("dpkg-deb");
    let entradas = Deb::abrir(&ruta).listado(dpkg.as_deref());
    comprobar_rutas_y_modos(&entradas);
}

#[test]
fn el_control_del_deb_es_aarch64_sugiere_termux_api_y_openssh_y_no_depende_de_nada() {
    let Some(ruta) = deb_de_la_variable() else {
        return;
    };
    let dpkg = buscar("dpkg-deb");
    let deb = Deb::abrir(&ruta);
    let entradas = deb.listado(dpkg.as_deref());
    let version = comprobar_control(&deb.control(dpkg.as_deref()), &entradas);
    assert_eq!(
        ruta.file_name().and_then(|n| n.to_str()),
        Some(nombre_de_fichero(&version).as_str()),
        "el fichero se llama magi-ssh_<versión>_aarch64.deb (con guion en las candidatas)"
    );
}

#[test]
fn el_binario_del_deb_solo_pide_bibliotecas_del_ndk_estable() {
    let Some(ruta) = deb_de_la_variable() else {
        return;
    };
    if buscar("readelf").is_none() {
        omitir("falta readelf (binutils)");
        return;
    }
    let extraido = Deb::abrir(&ruta).extraer();
    comprobar_bibliotecas(&extraido.path().join(PREFIJO).join("bin/magi"));
}

#[test]
fn el_deb_instala_lo_que_versiona_el_repositorio() {
    let Some(ruta) = deb_de_la_variable() else {
        return;
    };
    let extraido = Deb::abrir(&ruta).extraer();
    comprobar_contenido(extraido.path());
}

// ---------------------------------------------------------------------------
// Grupo 2: scripts/paquete-termux.sh con un doble de dpkg-deb
// ---------------------------------------------------------------------------

/// Doble de `dpkg-deb` para probar el script sin `dpkg`. Anota sus argumentos,
/// la fecha que recibe, el árbol que le dan y el `control`, y arma un `.deb` de
/// verdad (formato ar con `debian-binary`, `control.tar.xz` y `data.tar.xz`).
///
/// `DOBLE_DPKG_ESTRAGO`: `extra-fichero` (añade un fichero que no debe estar),
/// `depends` (añade un `Depends`) o `falla` (sale con error).
const DOBLE_DPKG_DEB: &str = r##"#!/bin/sh
set -eu

registro="${DOBLE_DPKG_REGISTRO:-/dev/null}"
{
  printf '%s\n' "$*"
  printf 'SOURCE_DATE_EPOCH=%s\n' "${SOURCE_DATE_EPOCH:-}"
} >> "$registro"

modo=''
comprimido=''
raiz_root=''
pos1=''
pos2=''
for argumento in "$@"; do
  case "$argumento" in
    -Zxz) comprimido=xz ;;
    --root-owner-group) raiz_root=1 ;;
    -b | --build) modo=build ;;
    -c | --contents) modo=contents ;;
    -f | --field) modo=field ;;
    -*)
      echo "dpkg-deb: opción no admitida por el doble: $argumento" >&2
      exit 2
      ;;
    *)
      if [ -z "$pos1" ]; then pos1="$argumento"; else pos2="$argumento"; fi
      ;;
  esac
done

case "$modo" in
  build)
    [ "$comprimido" = xz ] || { echo "dpkg-deb: falta -Zxz" >&2; exit 2; }
    [ "$raiz_root" = 1 ] || { echo "dpkg-deb: falta --root-owner-group" >&2; exit 2; }
    [ "${DOBLE_DPKG_ESTRAGO:-}" != falla ] || { echo "dpkg-deb: error simulado" >&2; exit 2; }
    arbol="$pos1"
    destino="$pos2"
    [ -f "$arbol/DEBIAN/control" ] || { echo "dpkg-deb: no hay DEBIAN/control" >&2; exit 2; }
    permisos="$(stat -c %a "$arbol/DEBIAN")"
    case "$permisos" in
      755 | 775) ;;
      *)
        echo "dpkg-deb: el directorio de control tiene permisos $permisos (deben ser 755)" >&2
        exit 2
        ;;
    esac
    {
      echo '--- arbol'
      (cd "$arbol" && find . -printf '%M %P\n' | sort)
      echo '--- control'
      cat "$arbol/DEBIAN/control"
    } >> "$registro"

    trabajo="$(mktemp -d)"
    trap 'rm -rf "$trabajo"' EXIT
    epoca="${SOURCE_DATE_EPOCH:-0}"
    if [ "${DOBLE_DPKG_ESTRAGO:-}" = depends ]; then
      echo 'Depends: libc++' >> "$arbol/DEBIAN/control"
    fi
    if [ "${DOBLE_DPKG_ESTRAGO:-}" = extra-fichero ]; then
      mkdir -p "$arbol/data/data/com.termux/files/usr/etc"
      echo extra > "$arbol/data/data/com.termux/files/usr/etc/extra"
    fi
    tar -C "$arbol/DEBIAN" --sort=name --owner=root:0 --group=root:0 --mtime="@$epoca" \
      -cf - . | xz -c > "$trabajo/control.tar.xz"
    tar -C "$arbol" --exclude=./DEBIAN --sort=name --owner=root:0 --group=root:0 \
      --mtime="@$epoca" -cf - . | xz -c > "$trabajo/data.tar.xz"
    printf '2.0\n' > "$trabajo/debian-binary"
    (cd "$trabajo" && ar rcD paquete.deb debian-binary control.tar.xz data.tar.xz)
    mv -f "$trabajo/paquete.deb" "$destino"
    echo "dpkg-deb: construyendo el paquete en «$destino» (doble)."
    ;;
  contents)
    ar p "$pos1" data.tar.xz | tar -tvJf -
    ;;
  field)
    ar p "$pos1" control.tar.xz | tar -xJOf - ./control
    ;;
  *)
    echo "dpkg-deb: modo no admitido por el doble: $*" >&2
    exit 2
    ;;
esac
"##;

/// ELF64 de 64 bits mínimo, PIE, con una sección dinámica que pide las
/// bibliotecas dadas: lo justo para que `readelf -h` y `readelf -d` lo lean.
/// `maquina`: 183 (aarch64) o 62 (x86_64).
fn elf(maquina: u16, bibliotecas: &[&str]) -> Vec<u8> {
    let mut tabla = vec![0u8];
    let mut desplazamientos = Vec::new();
    for biblioteca in bibliotecas {
        desplazamientos.push(tabla.len() as u64);
        tabla.extend_from_slice(biblioteca.as_bytes());
        tabla.push(0);
    }
    let (inicio_cabeceras, num_cabeceras, tam_cabecera) = (64u64, 2u64, 56u64);
    let inicio_tabla = inicio_cabeceras + num_cabeceras * tam_cabecera;
    let inicio_dinamica = (inicio_tabla + tabla.len() as u64 + 7) & !7;
    let mut dinamica = Vec::new();
    for desplazamiento in desplazamientos {
        dinamica.extend_from_slice(&1i64.to_le_bytes()); // DT_NEEDED
        dinamica.extend_from_slice(&desplazamiento.to_le_bytes());
    }
    for (etiqueta, valor) in [(5i64, inicio_tabla), (10, tabla.len() as u64), (0, 0)] {
        // DT_STRTAB, DT_STRSZ y DT_NULL
        dinamica.extend_from_slice(&etiqueta.to_le_bytes());
        dinamica.extend_from_slice(&valor.to_le_bytes());
    }
    let total = inicio_dinamica + dinamica.len() as u64;

    let mut salida = Vec::with_capacity(total as usize);
    salida.extend_from_slice(b"\x7fELF");
    salida.extend_from_slice(&[2, 1, 1, 0]);
    salida.extend_from_slice(&[0u8; 8]);
    salida.extend_from_slice(&3u16.to_le_bytes()); // ET_DYN
    salida.extend_from_slice(&maquina.to_le_bytes());
    salida.extend_from_slice(&1u32.to_le_bytes());
    salida.extend_from_slice(&0u64.to_le_bytes()); // entrada
    salida.extend_from_slice(&inicio_cabeceras.to_le_bytes());
    salida.extend_from_slice(&0u64.to_le_bytes()); // sin secciones
    salida.extend_from_slice(&0u32.to_le_bytes());
    for valor in [64u16, tam_cabecera as u16, num_cabeceras as u16, 64, 0, 0] {
        salida.extend_from_slice(&valor.to_le_bytes());
    }
    // PT_LOAD sobre todo el fichero y PT_DYNAMIC sobre la sección dinámica.
    for (tipo, banderas, desplazamiento, tamano, alineacion) in [
        (1u32, 5u32, 0u64, total, 0x1000u64),
        (2, 6, inicio_dinamica, dinamica.len() as u64, 8),
    ] {
        salida.extend_from_slice(&tipo.to_le_bytes());
        salida.extend_from_slice(&banderas.to_le_bytes());
        for valor in [
            desplazamiento,
            desplazamiento,
            desplazamiento,
            tamano,
            tamano,
            alineacion,
        ] {
            salida.extend_from_slice(&valor.to_le_bytes());
        }
    }
    salida.extend_from_slice(&tabla);
    salida.resize(inicio_dinamica as usize, 0);
    salida.extend_from_slice(&dinamica);
    salida
}

/// Un ELF aarch64 como el de Termux: pide lo que puede pedir.
fn elf_bueno() -> Vec<u8> {
    elf(183, &["libdl.so", "libz.so", "libm.so", "libc.so"])
}

struct Resultado {
    codigo: Option<i32>,
    salida: String,
    error: String,
}

impl Resultado {
    fn desde(salida: Output) -> Self {
        Self {
            codigo: salida.status.code(),
            salida: bytes_a_texto(&salida.stdout),
            error: bytes_a_texto(&salida.stderr),
        }
    }

    fn exito(&self) {
        assert_eq!(
            self.codigo,
            Some(0),
            "el script debía salir bien.\nstdout: {}\nstderr: {}",
            self.salida,
            self.error
        );
    }

    fn fallo(&self, codigo: i32, fragmento: &str) {
        assert_eq!(
            self.codigo,
            Some(codigo),
            "el script debía salir con {codigo}.\nstdout: {}\nstderr: {}",
            self.salida,
            self.error
        );
        assert!(
            self.error.contains(fragmento),
            "el error debía decir «{fragmento}» y dice: {}",
            self.error
        );
    }
}

/// Cómo se lanza el script en una prueba.
struct Lanzamiento<'a> {
    /// ¿El doble de `dpkg-deb` va delante en el `PATH`?
    con_doble: bool,
    /// Valor de `SOURCE_DATE_EPOCH` (`None`: sin ponerla).
    epoca: Option<&'a str>,
    /// Directorio de salida (`None`: el del escenario).
    salida: Option<&'a Path>,
    extra: &'a [(&'a str, &'a str)],
}

impl Default for Lanzamiento<'_> {
    fn default() -> Self {
        Self {
            con_doble: true,
            epoca: Some("1700000000"),
            salida: None,
            extra: &[],
        }
    }
}

struct Escenario {
    _directorio: tempfile::TempDir,
    base: PathBuf,
    bin: PathBuf,
    salida: PathBuf,
    registro: PathBuf,
    elf: PathBuf,
    bash: PathBuf,
}

impl Escenario {
    /// `None` (y la prueba se omite) si falta alguna herramienta.
    fn nuevo() -> Option<Self> {
        let faltan: Vec<&str> = HERRAMIENTAS_SCRIPT
            .iter()
            .copied()
            .filter(|h| buscar(h).is_none())
            .collect();
        if !faltan.is_empty() {
            omitir(&format!(
                "faltan herramientas para probar el script: {}",
                faltan.join(", ")
            ));
            return None;
        }
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let base = directorio.path().to_path_buf();
        let bin = base.join("bin");
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(base.join("tmp")).unwrap();
        let doble = bin.join("dpkg-deb");
        fs::write(&doble, DOBLE_DPKG_DEB).unwrap();
        fs::set_permissions(&doble, fs::Permissions::from_mode(0o755)).unwrap();
        let elf_ruta = base.join("magi");
        fs::write(&elf_ruta, elf_bueno()).unwrap();
        Some(Self {
            salida: base.join("salida"),
            registro: base.join("registro.txt"),
            bash: buscar("bash").expect("bash"),
            _directorio: directorio,
            base,
            bin,
            elf: elf_ruta,
        })
    }

    fn script(&self) -> PathBuf {
        raiz("scripts/paquete-termux.sh")
    }

    fn lanzar(&self, argumentos: &[&str], lanzamiento: &Lanzamiento) -> Resultado {
        let original = std::env::var("PATH").unwrap_or_default();
        let camino = if lanzamiento.con_doble {
            format!("{}:{original}", self.bin.display())
        } else {
            original
        };
        let salida = lanzamiento.salida.unwrap_or(&self.salida);
        let mut orden = Command::new(&self.bash);
        orden
            .arg(self.script())
            .args(argumentos)
            .current_dir(&self.base)
            .env_clear()
            .env("PATH", camino)
            .env("HOME", &self.base)
            .env("TMPDIR", self.base.join("tmp"))
            .env("MAGI_DEB_SALIDA", salida)
            .env("DOBLE_DPKG_REGISTRO", &self.registro);
        if let Some(epoca) = lanzamiento.epoca {
            orden.env("SOURCE_DATE_EPOCH", epoca);
        }
        for (nombre, valor) in lanzamiento.extra {
            orden.env(nombre, valor);
        }
        Resultado::desde(orden.output().expect("lanzar el script"))
    }

    /// El script con el binario bueno, el doble y los valores por defecto.
    fn armar(&self, version: &str) -> Resultado {
        self.lanzar(
            &[version, self.elf.to_str().unwrap()],
            &Lanzamiento::default(),
        )
    }

    fn registro(&self) -> String {
        fs::read_to_string(&self.registro).unwrap_or_default()
    }

    fn escribir_binario(&self, nombre: &str, contenido: &[u8]) -> PathBuf {
        let ruta = self.base.join(nombre);
        fs::write(&ruta, contenido).unwrap();
        ruta
    }

    fn debs_en(&self, directorio: &Path) -> Vec<String> {
        fs::read_dir(directorio)
            .map(|entradas| {
                entradas
                    .filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| n.contains(".deb"))
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[test]
fn el_script_arma_el_deb_con_las_opciones_y_el_arbol_del_informe() {
    let Some(esc) = Escenario::nuevo() else {
        return;
    };
    let resultado = esc.armar("1.6.0");
    resultado.exito();

    let deb = esc.salida.join("magi-ssh_1.6.0_aarch64.deb");
    assert!(deb.is_file(), "no se creó {}", deb.display());
    // `dpkg-deb -Zxz --root-owner-group --build <árbol> <fichero>`.
    let registro = esc.registro();
    let invocacion = registro.lines().next().expect("el doble no anotó nada");
    assert!(
        invocacion.starts_with("-Zxz --root-owner-group --build "),
        "argumentos de dpkg-deb: {invocacion}"
    );
    assert!(
        invocacion.contains("/pkg "),
        "el árbol se arma en un directorio temporal: {invocacion}"
    );
    assert!(
        invocacion.ends_with("/magi-ssh_1.6.0_aarch64.deb"),
        "{invocacion}"
    );

    // El árbol que recibe dpkg-deb: modos de lo instalado y DEBIAN en 755.
    let arbol: Vec<&str> = registro
        .lines()
        .skip_while(|l| *l != "--- arbol")
        .skip(1)
        .take_while(|l| *l != "--- control")
        .collect();
    for esperado in [
        format!("-rwx------ {PREFIJO}/bin/magi"),
        format!("-rw-r--r-- {PREFIJO}/share/man/man1/magi.1.gz"),
        "drwxr-xr-x DEBIAN".to_string(),
        "-rw-r--r-- DEBIAN/control".to_string(),
    ] {
        assert!(
            arbol.contains(&esperado.as_str()),
            "falta «{esperado}» en el árbol: {arbol:#?}"
        );
    }
    assert!(
        arbol
            .iter()
            .all(|l| !l.starts_with("-rwxr") && !l.contains("rwxrwx")),
        "modos inesperados en el árbol: {arbol:#?}"
    );

    // Lo armado: la comprobación completa, sin usar el doble para leerlo.
    assert_eq!(comprobar_todo(&deb, None), "1.6.0");

    // El .sha256: «<suma>  <fichero>», y vale para `sha256sum -c`.
    let suma =
        fs::read_to_string(esc.salida.join("magi-ssh_1.6.0_aarch64.deb.sha256")).expect(".sha256");
    let real = ejecutar(Command::new("sha256sum").arg(&deb));
    assert_eq!(
        suma,
        format!(
            "{}  magi-ssh_1.6.0_aarch64.deb\n",
            real.split_whitespace().next().unwrap()
        )
    );
    ejecutar(
        Command::new("sha256sum")
            .arg("-c")
            .arg("magi-ssh_1.6.0_aarch64.deb.sha256")
            .current_dir(&esc.salida),
    );
}

#[test]
fn una_candidata_rc_lleva_la_tilde_en_la_version_y_el_guion_en_el_nombre_del_fichero() {
    let Some(esc) = Escenario::nuevo() else {
        return;
    };
    esc.armar("1.6.0-rc.1").exito();
    // El `Version` del control lleva tilde (ordena antes que 1.6.0; con guion
    // ordenaría después)...
    let deb = esc.salida.join("magi-ssh_1.6.0-rc.1_aarch64.deb");
    assert!(
        deb.is_file(),
        "falta el .deb con guion: {:?}",
        esc.debs_en(&esc.salida)
    );
    assert_eq!(comprobar_todo(&deb, None), "1.6.0~rc.1");
    assert!(
        !esc.registro().contains("Version: 1.6.0-rc"),
        "la versión de Debian no lleva guion"
    );
    // ...pero ningún fichero de la salida lleva tilde en el nombre: GitHub puede
    // cambiarla por un punto y el .sha256 ya no cuadraría con el adjunto.
    let nombres = esc.debs_en(&esc.salida);
    assert_eq!(
        {
            let mut ordenados = nombres.clone();
            ordenados.sort();
            ordenados
        },
        [
            "magi-ssh_1.6.0-rc.1_aarch64.deb",
            "magi-ssh_1.6.0-rc.1_aarch64.deb.sha256"
        ],
        "ficheros de la salida"
    );
    assert!(
        nombres.iter().all(|n| !n.contains('~')),
        "el nombre de un adjunto no lleva tilde: {nombres:?}"
    );
    // El .sha256 nombra el fichero con guion y vale para `sha256sum -c`.
    let suma = fs::read_to_string(esc.salida.join("magi-ssh_1.6.0-rc.1_aarch64.deb.sha256"))
        .expect(".sha256");
    assert!(
        suma.trim_end()
            .ends_with("  magi-ssh_1.6.0-rc.1_aarch64.deb"),
        "el .sha256 debe nombrar el fichero con guion: {suma}"
    );
    ejecutar(
        Command::new("sha256sum")
            .arg("-c")
            .arg("magi-ssh_1.6.0-rc.1_aarch64.deb.sha256")
            .current_dir(&esc.salida),
    );
    // dpkg-deb recibe el fichero con el nombre con guion.
    assert!(
        esc.registro()
            .lines()
            .next()
            .is_some_and(|l| l.ends_with("/magi-ssh_1.6.0-rc.1_aarch64.deb")),
        "{}",
        esc.registro()
    );
}

#[test]
fn las_versiones_mal_formadas_se_rechazan_antes_de_tocar_nada() {
    let Some(esc) = Escenario::nuevo() else {
        return;
    };
    for version in [
        "1.6",
        "v1.6.0",
        "01.6.0",
        "1.06.0",
        "1.6.0-rc.0",
        "1.6.0-rc.",
        "1.6.0-beta.1",
        "1.6.0+1",
        "1.6.0 ",
        "",
        "-1.6.0",
        "1.6.0-rc.1-rc.2",
        "1.6.0;touch x",
        "1.6.0-rc.1\n",
    ] {
        let resultado = esc.lanzar(
            &[version, esc.elf.to_str().unwrap()],
            &Lanzamiento::default(),
        );
        resultado.fallo(2, "no es una versión válida");
    }
    assert!(
        esc.registro().is_empty(),
        "no debía llegar a llamar a dpkg-deb"
    );
    assert!(!esc.salida.exists(), "no debía crear la salida");
    // Argumentos de más o de menos.
    esc.lanzar(&[], &Lanzamiento::default())
        .fallo(2, "falta la versión");
    esc.lanzar(&["1.6.0", "a", "b"], &Lanzamiento::default())
        .fallo(2, "demasiados argumentos");
}

#[test]
fn sin_dpkg_deb_sale_con_1_y_dice_como_instalarlo() {
    let Some(esc) = Escenario::nuevo() else {
        return;
    };
    let vacio = esc.base.join("vacio");
    fs::create_dir_all(&vacio).unwrap();
    let resultado = Resultado::desde(
        Command::new(&esc.bash)
            .arg(esc.script())
            .arg("1.6.0")
            .arg(&esc.elf)
            .current_dir(&esc.base)
            .env_clear()
            .env("PATH", &vacio)
            .env("MAGI_DEB_SALIDA", &esc.salida)
            .output()
            .expect("lanzar el script"),
    );
    resultado.fallo(1, "no se encuentra «dpkg-deb»");
    assert!(
        resultado.error.contains("sudo pacman -S dpkg"),
        "dice cómo instalarlo: {}",
        resultado.error
    );
    assert!(!esc.salida.exists(), "no debía crear la salida");
}

#[test]
fn un_binario_que_no_vale_no_se_empaqueta() {
    let Some(esc) = Escenario::nuevo() else {
        return;
    };
    let casos: Vec<(&str, Vec<u8>, &str)> = vec![
        ("no_elf", b"esto no es un ELF".to_vec(), "no es un ELF"),
        ("x86_64", elf(62, &["libc.so"]), "no es aarch64"),
        (
            "con_libcxx",
            elf(183, &["libc.so", "libc++_shared.so"]),
            "libc++_shared.so",
        ),
        ("glibc", elf(183, &["libc.so.6"]), "libc.so.6"),
        ("sin_libc", elf(183, &["libm.so"]), "no pide libc.so"),
        ("vacio", Vec::new(), "está vacío"),
    ];
    for (nombre, contenido, fragmento) in casos {
        let ruta = esc.escribir_binario(nombre, &contenido);
        let resultado = esc.lanzar(&["1.6.0", ruta.to_str().unwrap()], &Lanzamiento::default());
        resultado.fallo(1, fragmento);
    }
    // La explicación del caso de C++: de qué depende el paquete y qué hacer.
    let ruta = esc.base.join("con_libcxx");
    let resultado = esc.lanzar(&["1.6.0", ruta.to_str().unwrap()], &Lanzamiento::default());
    assert!(
        resultado.error.contains("libc++") && resultado.error.contains("Depends"),
        "debe explicar que el paquete tendría que depender de libc++: {}",
        resultado.error
    );
    // Un binario que no existe y un directorio.
    let falta = esc.base.join("no_existe");
    esc.lanzar(&["1.6.0", falta.to_str().unwrap()], &Lanzamiento::default())
        .fallo(1, "no existe el binario");
    esc.lanzar(
        &["1.6.0", esc.bin.to_str().unwrap()],
        &Lanzamiento::default(),
    )
    .fallo(1, "no es un fichero normal");

    assert!(
        esc.registro().is_empty(),
        "con un binario malo no se llama a dpkg-deb"
    );
    assert!(
        esc.debs_en(&esc.salida).is_empty(),
        "no debe quedar ningún .deb"
    );
}

#[test]
fn lo_que_dpkg_deb_deja_fuera_de_lo_esperado_hace_fallar_y_no_deja_deb() {
    let Some(esc) = Escenario::nuevo() else {
        return;
    };
    let ruta = esc.elf.to_str().unwrap().to_string();
    for (estrago, codigo, fragmento) in [
        (
            "extra-fichero",
            1,
            "el contenido del paquete no es el esperado",
        ),
        ("depends", 1, "lleva Depends"),
        ("falla", 1, "dpkg-deb no pudo armar el paquete"),
    ] {
        let extra = [("DOBLE_DPKG_ESTRAGO", estrago)];
        let resultado = esc.lanzar(
            &["1.6.0", &ruta],
            &Lanzamiento {
                extra: &extra,
                ..Lanzamiento::default()
            },
        );
        resultado.fallo(codigo, fragmento);
        assert!(
            esc.debs_en(&esc.salida).is_empty(),
            "con «{estrago}» no debe quedar ningún .deb"
        );
    }
}

#[test]
fn dos_ejecuciones_con_las_mismas_entradas_dan_el_mismo_deb() {
    let Some(esc) = Escenario::nuevo() else {
        return;
    };
    let (a, b, c) = (esc.base.join("a"), esc.base.join("b"), esc.base.join("c"));
    let ruta = esc.elf.to_str().unwrap().to_string();
    for (salida, epoca) in [(&a, "1700000000"), (&b, "1700000000"), (&c, "1700000001")] {
        let resultado = esc.lanzar(
            &["1.6.0", &ruta],
            &Lanzamiento {
                salida: Some(salida.as_path()),
                epoca: Some(epoca),
                ..Lanzamiento::default()
            },
        );
        resultado.exito();
    }
    let leer = |dir: &Path| fs::read(dir.join("magi-ssh_1.6.0_aarch64.deb")).unwrap();
    assert_eq!(leer(&a), leer(&b), "mismas entradas, mismo .deb");
    assert_ne!(
        leer(&a),
        leer(&c),
        "SOURCE_DATE_EPOCH debe llegar a dpkg-deb"
    );
    // El .sha256 también es el mismo.
    let suma = |dir: &Path| fs::read(dir.join("magi-ssh_1.6.0_aarch64.deb.sha256")).unwrap();
    assert_eq!(suma(&a), suma(&b));
    // La época sale de SOURCE_DATE_EPOCH tal cual (la ve el doble).
    assert!(esc.registro().contains("SOURCE_DATE_EPOCH=1700000001"));
}

#[test]
fn sin_source_date_epoch_las_marcas_de_tiempo_salen_del_ultimo_commit() {
    let Some(esc) = Escenario::nuevo() else {
        return;
    };
    let commit = Command::new("git")
        .args(["log", "-1", "--format=%ct"])
        .current_dir(RAIZ)
        .output();
    let Some(commit) = commit.ok().filter(|o| o.status.success()) else {
        omitir(
            "no hay git o no es un repositorio: no se puede comprobar la fecha del último commit",
        );
        return;
    };
    let esperada = bytes_a_texto(&commit.stdout).trim().to_string();
    let ruta = esc.elf.to_str().unwrap().to_string();
    // El script lee la fecha con git: necesita git en el PATH (ya lo está).
    esc.lanzar(
        &["1.6.0", &ruta],
        &Lanzamiento {
            epoca: None,
            ..Lanzamiento::default()
        },
    )
    .exito();
    assert!(
        esc.registro()
            .contains(&format!("SOURCE_DATE_EPOCH={esperada}")),
        "debía usar la fecha del último commit ({esperada}): {}",
        esc.registro()
    );
    // Una época mal formada se rechaza.
    esc.lanzar(
        &["1.6.0", &ruta],
        &Lanzamiento {
            epoca: Some("ayer"),
            ..Lanzamiento::default()
        },
    )
    .fallo(1, "SOURCE_DATE_EPOCH no es un número");
}

#[test]
fn comprobar_binario_solo_mira_el_binario_y_no_necesita_dpkg_deb() {
    let Some(esc) = Escenario::nuevo() else {
        return;
    };
    let sin_doble = Lanzamiento {
        con_doble: false,
        ..Lanzamiento::default()
    };
    let bueno = esc.elf.to_str().unwrap().to_string();
    let resultado = esc.lanzar(&["--comprobar-binario", &bueno], &sin_doble);
    resultado.exito();
    assert!(
        resultado.salida.contains("Binario correcto"),
        "{}",
        resultado.salida
    );
    let malo = esc.escribir_binario("malo", &elf(183, &["libc.so", "libc++_shared.so"]));
    esc.lanzar(&["--comprobar-binario", malo.to_str().unwrap()], &sin_doble)
        .fallo(1, "libc++_shared.so");
    esc.lanzar(&["--comprobar-binario", "uno", "dos"], &sin_doble)
        .fallo(2, "admite como mucho un argumento");
    assert!(esc.registro().is_empty(), "no debe llamar a dpkg-deb");
    assert!(!esc.salida.exists());
}

#[test]
fn con_el_dpkg_deb_real_se_arma_un_paquete_valido_y_reproducible() {
    let Some(esc) = Escenario::nuevo() else {
        return;
    };
    let Some(dpkg) = buscar("dpkg-deb") else {
        omitir("no hay dpkg-deb real en el PATH (en Arch: sudo pacman -S dpkg)");
        return;
    };
    let ruta = esc.elf.to_str().unwrap().to_string();
    let (a, b) = (esc.base.join("real_a"), esc.base.join("real_b"));
    for salida in [&a, &b] {
        let resultado = esc.lanzar(
            &["1.6.0-rc.2", &ruta],
            &Lanzamiento {
                con_doble: false,
                salida: Some(salida.as_path()),
                ..Lanzamiento::default()
            },
        );
        resultado.exito();
    }
    // El fichero con guion y la versión del control con tilde.
    let deb = a.join("magi-ssh_1.6.0-rc.2_aarch64.deb");
    assert_eq!(comprobar_todo(&deb, Some(&dpkg)), "1.6.0~rc.2");
    assert_eq!(
        fs::read(&deb).unwrap(),
        fs::read(b.join("magi-ssh_1.6.0-rc.2_aarch64.deb")).unwrap(),
        "con SOURCE_DATE_EPOCH fijo, dos ejecuciones dan el mismo .deb"
    );
}

#[test]
fn el_elf_de_prueba_lo_entiende_readelf() {
    // Si esto falla, los demás resultados de este grupo no valen: el ELF a mano
    // tiene que decir lo que se le ha puesto.
    let Some(esc) = Escenario::nuevo() else {
        return;
    };
    let cabecera = ejecutar(Command::new("readelf").arg("-h").arg(&esc.elf));
    assert!(cabecera.contains("AArch64"), "{cabecera}");
    let dinamica = ejecutar(Command::new("readelf").arg("-d").arg(&esc.elf));
    for biblioteca in ["libdl.so", "libz.so", "libm.so", "libc.so"] {
        assert!(dinamica.contains(&format!("[{biblioteca}]")), "{dinamica}");
    }
}

// ---------------------------------------------------------------------------
// Grupo 3: workflow, CI y receta de TUR
// ---------------------------------------------------------------------------

/// Lo que dice el workflow bajo `on.push.tags` (los patrones, sin comillas).
fn patrones_de_tags(texto: &str) -> Vec<String> {
    let mut dentro = false;
    let mut patrones = Vec::new();
    for linea in texto.lines() {
        if linea.trim_end() == "    tags:" {
            dentro = true;
            continue;
        }
        if dentro {
            match linea.trim_start().strip_prefix("- ") {
                Some(patron) if linea.starts_with("      - ") => {
                    patrones.push(
                        patron
                            .trim()
                            .trim_matches('\'')
                            .trim_matches('"')
                            .to_string(),
                    );
                }
                _ => break,
            }
        }
    }
    patrones
}

/// Un comparador de patrones de etiquetas de GitHub solo para lo que usa el
/// workflow: letras y signos literales y `[0-9]+`.
fn encaja(patron: &str, texto: &str) -> bool {
    fn rec(patron: &[u8], texto: &[u8]) -> bool {
        let Some(&primero) = patron.first() else {
            return texto.is_empty();
        };
        if patron.starts_with(b"[0-9]+") {
            let digitos = texto.iter().take_while(|b| b.is_ascii_digit()).count();
            return (1..=digitos).rev().any(|n| rec(&patron[6..], &texto[n..]));
        }
        assert!(
            !matches!(primero, b'*' | b'?' | b'[' | b'+' | b'!'),
            "el comparador de la prueba no admite «{}» en el patrón",
            primero as char
        );
        texto.first() == Some(&primero) && rec(&patron[1..], &texto[1..])
    }
    rec(patron.as_bytes(), texto.as_bytes())
}

#[test]
fn termux_release_se_dispara_con_las_etiquetas_de_version_y_nunca_con_android_v() {
    let texto = leer_texto(".github/workflows/termux-release.yml");
    let patrones = patrones_de_tags(&texto);
    assert_eq!(
        patrones,
        ["v[0-9]+.[0-9]+.[0-9]+", "v[0-9]+.[0-9]+.[0-9]+-rc.[0-9]+"],
        "patrones de etiqueta de termux-release.yml"
    );
    let dispara = |etiqueta: &str| patrones.iter().any(|p| encaja(p, etiqueta));
    for si in ["v1.6.0", "v1.6.0-rc.1", "v10.20.30", "v1.6.0-rc.12"] {
        assert!(dispara(si), "{si} debe disparar termux-release.yml");
    }
    for no in [
        "android-v0.3.0",
        "android-v0.3.0-rc.1",
        "1.6.0",
        "v1.6",
        "v1.6.0.1",
        "v1.6.0-beta.1",
        "v1.6.0-rc.",
        "xv1.6.0",
    ] {
        assert!(!dispara(no), "{no} no debe disparar termux-release.yml");
    }
    // Y dist sigue sin dispararse con las de la app Android.
    assert!(
        leer_texto(".github/workflows/release.yml").contains("- '!android-v*'"),
        "release.yml debe seguir excluyendo android-v*"
    );
    assert!(
        texto.contains("workflow_dispatch:"),
        "falta el disparo manual"
    );
}

/// El texto de un workflow sin sus comentarios de YAML: lo que se ejecuta, no
/// lo que se explica.
fn sin_comentarios(texto: &str) -> String {
    texto
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// El trozo de un workflow que va de la línea `  <job>:` (dos espacios, bajo
/// `jobs:`) a la siguiente clave de ese nivel. El texto va ya sin comentarios.
fn trozo_de_job(texto: &str, job: &str) -> String {
    let cabecera = format!("  {job}:");
    let mut dentro = false;
    let mut trozo = Vec::new();
    for linea in texto.lines() {
        let sangria = linea.len() - linea.trim_start().len();
        if dentro && !linea.trim().is_empty() && sangria <= 2 {
            break;
        }
        if linea == cabecera {
            dentro = true;
        }
        if dentro {
            trozo.push(linea);
        }
    }
    assert!(!trozo.is_empty(), "no hay un job «{job}» en el workflow");
    trozo.join("\n")
}

/// Los nombres de los jobs de un workflow (claves de dos espacios bajo `jobs:`).
fn nombres_de_jobs(texto: &str) -> Vec<String> {
    let despues = texto.split("\njobs:\n").nth(1).expect("falta jobs:");
    despues
        .lines()
        .filter(|l| l.starts_with("  ") && !l.starts_with("   "))
        .filter_map(|l| l.trim().strip_suffix(':').map(str::to_string))
        .collect()
}

/// El bloque `permissions:` que empieza en la línea con esa sangría: clave →
/// valor. Sin bloque, vacío.
fn permisos_de(texto: &str, sangria: usize) -> BTreeMap<String, String> {
    let cabecera = format!("{}permissions:", " ".repeat(sangria));
    let mut permisos = BTreeMap::new();
    let mut dentro = false;
    for linea in texto.lines() {
        if dentro {
            let actual = linea.len() - linea.trim_start().len();
            if linea.trim().is_empty() || actual <= sangria {
                break;
            }
            let (clave, valor) = linea
                .trim()
                .split_once(':')
                .unwrap_or_else(|| panic!("permiso no entendido: «{linea}»"));
            // `dist` escribe los permisos entre comillas.
            permisos.insert(
                clave.trim().trim_matches('"').to_string(),
                valor.trim().trim_matches('"').to_string(),
            );
        } else if linea == cabecera {
            dentro = true;
        }
    }
    permisos
}

fn mapa(pares: &[(&str, &str)]) -> BTreeMap<String, String> {
    pares
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// Qué permisos lleva cada job de `termux-release.yml` (sin comentarios): arriba,
/// solo lectura; `verificar`, solo lectura; `compilar`, lectura y atestación (no
/// escribe en el repositorio); `publicar`, la única escritura. Devuelve el
/// primer fallo, para poder probar que una versión movida de job se detecta.
fn comprobar_permisos_por_job(texto: &str) -> Result<(), String> {
    let arriba = texto.split("\njobs:\n").next().unwrap_or_default();
    let esperado_arriba = mapa(&[("contents", "read")]);
    if permisos_de(arriba, 0) != esperado_arriba {
        return Err(format!(
            "permisos de todo el workflow: {:?}, y deben ser {esperado_arriba:?}",
            permisos_de(arriba, 0)
        ));
    }
    let jobs = nombres_de_jobs(texto);
    if jobs != ["verificar", "compilar", "publicar"] {
        return Err(format!(
            "jobs del workflow: {jobs:?}; si hay otro, hay que decidir sus permisos aquí"
        ));
    }
    let verificar = trozo_de_job(texto, "verificar");
    if verificar.contains(": write") {
        return Err("verificar no debe poder escribir nada".into());
    }
    // Sin bloque propio hereda `contents: read`; con él, nada más que eso.
    let propios = permisos_de(&verificar, 4);
    if !propios.is_empty() && propios != esperado_arriba {
        return Err(format!("permisos de verificar: {propios:?}"));
    }
    let compilar = trozo_de_job(texto, "compilar");
    let esperado_compilar = mapa(&[
        ("contents", "read"),
        ("id-token", "write"),
        ("attestations", "write"),
    ]);
    if permisos_de(&compilar, 4) != esperado_compilar {
        return Err(format!(
            "permisos de compilar: {:?}, y deben ser {esperado_compilar:?}",
            permisos_de(&compilar, 4)
        ));
    }
    let publicar = trozo_de_job(texto, "publicar");
    let esperado_publicar = mapa(&[("contents", "write")]);
    if permisos_de(&publicar, 4) != esperado_publicar {
        return Err(format!(
            "permisos de publicar: {:?}, y deben ser {esperado_publicar:?}",
            permisos_de(&publicar, 4)
        ));
    }
    // Y cada cosa en su job: la atestación al compilar, la subida al publicar.
    for (trozo, nombre, debe, no_debe) in [
        (
            &compilar,
            "compilar",
            "attest-build-provenance",
            "gh release",
        ),
        (
            &publicar,
            "publicar",
            "gh release upload",
            "attest-build-provenance",
        ),
        (&verificar, "verificar", "", "gh release"),
    ] {
        if !trozo.contains(debe) || trozo.contains(no_debe) {
            return Err(format!("{nombre} debe llevar «{debe}» y no «{no_debe}»"));
        }
    }
    Ok(())
}

#[test]
fn termux_release_da_a_cada_job_solo_los_permisos_que_necesita() {
    let texto = sin_comentarios(&leer_texto(".github/workflows/termux-release.yml"));
    comprobar_permisos_por_job(&texto).unwrap();

    // El comprobador sirve: contar cuántas veces sale «contents: write» no vería
    // que se mueve de job; este sí. Cada alteración deja el mismo número de
    // «contents: write», «id-token: write» y «attestations: write» que el original.
    let (antes, publicar) = texto.split_once("\n  publicar:\n").unwrap();
    let publicar = format!("\n  publicar:\n{publicar}");
    let moverlo = [
        // contents: write pasa de publicar a compilar.
        format!(
            "{}{}",
            antes.replacen(
                "      contents: read\n      id-token: write\n",
                "      contents: write\n      id-token: write\n",
                1
            ),
            publicar.replacen("      contents: write\n", "      contents: read\n", 1)
        ),
        // id-token: write pasa de compilar a publicar.
        format!(
            "{}{}",
            antes.replacen("      id-token: write\n", "      id-token: read\n", 1),
            publicar.replacen(
                "      contents: write\n",
                "      contents: write\n      id-token: write\n",
                1
            )
        ),
        // attestations: write pasa de compilar a publicar.
        format!(
            "{}{}",
            antes.replacen(
                "      attestations: write\n",
                "      attestations: read\n",
                1
            ),
            publicar.replacen(
                "      contents: write\n",
                "      contents: write\n      attestations: write\n",
                1
            )
        ),
        // verificar gana un permiso de escritura (con su propio bloque).
        texto.replacen(
            "  verificar:\n",
            "  verificar:\n    permissions:\n      contents: write\n",
            1,
        ),
        // Arriba, escritura para todos.
        texto.replacen(
            "permissions:\n  contents: read\n",
            "permissions:\n  contents: write\n",
            1,
        ),
    ];
    for (n, alterado) in moverlo.iter().enumerate() {
        assert_ne!(alterado, &texto, "la alteración {n} no cambió nada");
        if n < 3 {
            // Las tres primeras mueven un permiso de job sin cambiar cuántas
            // veces sale: contarlas, como hacía esta prueba, no las vería.
            for permiso in ["contents: write", "id-token: write", "attestations: write"] {
                assert_eq!(
                    alterado.matches(permiso).count(),
                    1,
                    "la alteración {n} debía dejar «{permiso}» una sola vez"
                );
            }
        }
        assert!(
            comprobar_permisos_por_job(alterado).is_err(),
            "la alteración {n} (un permiso en otro job) debía detectarse"
        );
    }
}

/// El paso de un workflow con ese nombre: de su línea `- name: …` a la del
/// siguiente paso (o al final del job).
fn paso_de(texto: &str, nombre: &str) -> String {
    let cabecera = format!("- name: {nombre}");
    let mut dentro = false;
    let mut sangria_paso = 0;
    let mut paso = Vec::new();
    for linea in texto.lines() {
        let sangria = linea.len() - linea.trim_start().len();
        if dentro && !linea.trim().is_empty() && sangria <= sangria_paso {
            break;
        }
        if !dentro && linea.trim() == cabecera {
            dentro = true;
            sangria_paso = sangria;
        }
        if dentro {
            paso.push(linea);
        }
    }
    assert!(!paso.is_empty(), "no hay un paso «{nombre}» en el workflow");
    paso.join("\n")
}

/// El cuerpo del `run: |` de un paso, sin la sangría: el script tal cual.
fn cuerpo_de_run(paso: &str) -> String {
    let mut lineas = paso.lines();
    let sangria_run = loop {
        let linea = lineas.next().expect("el paso no tiene run: |");
        if linea.trim() == "run: |" {
            break linea.len() - linea.trim_start().len();
        }
    };
    lineas
        .take_while(|l| l.trim().is_empty() || l.len() - l.trim_start().len() > sangria_run)
        .map(|l| l.get(sangria_run + 2..).unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Ejecuta el script de un paso con bash y un entorno mínimo.
fn ejecutar_paso(cuerpo: &str, camino: &str, entorno: &[(&str, &str)]) -> Resultado {
    ejecutar_paso_en(cuerpo, camino, entorno, None)
}

/// Lo mismo, en un directorio de trabajo (el de un paso que usa git).
fn ejecutar_paso_en(
    cuerpo: &str,
    camino: &str,
    entorno: &[(&str, &str)],
    directorio: Option<&Path>,
) -> Resultado {
    let bash = buscar("bash").expect("bash");
    let mut orden = Command::new(bash);
    orden
        .arg("-c")
        .arg(cuerpo)
        .env_clear()
        .env("PATH", camino)
        .envs(entorno.iter().copied());
    if let Some(directorio) = directorio {
        orden.current_dir(directorio);
    }
    Resultado::desde(orden.output().expect("lanzar el paso"))
}

/// Un repositorio de git de prueba, aislado de la configuración del usuario
/// (sin firma de commits, sin plantillas ni ganchos globales).
struct RepoGit {
    _directorio: tempfile::TempDir,
    ruta: PathBuf,
}

impl RepoGit {
    /// `None` (y la prueba se omite) si no hay git.
    fn nuevo() -> Option<Self> {
        if buscar("git").is_none() {
            omitir("no hay git para probar los pasos que lo usan");
            return None;
        }
        let directorio = tempfile::tempdir().expect("directorio temporal");
        let ruta = directorio.path().join("repo");
        fs::create_dir_all(&ruta).unwrap();
        let repo = Self {
            _directorio: directorio,
            ruta,
        };
        repo.git(&["init", "-q", "-b", "main"]);
        Some(repo)
    }

    /// El entorno con el que corre git (y los pasos que lo llaman).
    fn entorno(&self) -> Vec<(String, String)> {
        let casa = self.ruta.parent().unwrap().display().to_string();
        [
            ("HOME", casa.as_str()),
            ("GIT_CONFIG_GLOBAL", "/dev/null"),
            ("GIT_CONFIG_NOSYSTEM", "1"),
            ("GIT_AUTHOR_NAME", "Prueba"),
            ("GIT_AUTHOR_EMAIL", "prueba@ejemplo.org"),
            ("GIT_COMMITTER_NAME", "Prueba"),
            ("GIT_COMMITTER_EMAIL", "prueba@ejemplo.org"),
            ("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z"),
            ("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
    }

    /// Ejecuta git en el repositorio y devuelve su salida (sin el salto final).
    fn git(&self, argumentos: &[&str]) -> String {
        let salida = Command::new(buscar("git").expect("git"))
            .args(argumentos)
            .current_dir(&self.ruta)
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .envs(self.entorno())
            .output()
            .expect("lanzar git");
        let resultado = Resultado::desde(salida);
        assert_eq!(
            resultado.codigo,
            Some(0),
            "git {argumentos:?} debía salir bien: {}",
            resultado.error
        );
        resultado.salida.trim().to_string()
    }

    fn escribir(&self, relativa: &str, contenido: &str) {
        let ruta = self.ruta.join(relativa);
        fs::create_dir_all(ruta.parent().unwrap()).unwrap();
        fs::write(ruta, contenido).unwrap();
    }

    /// Confirma lo que haya y devuelve el SHA del commit.
    fn confirmar(&self, mensaje: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", mensaje]);
        self.git(&["rev-parse", "HEAD"])
    }

    /// Ejecuta el script de un paso de workflow dentro del repositorio, con
    /// estas variables además del entorno de git.
    fn paso(&self, cuerpo: &str, variables: &[(&str, &str)]) -> Resultado {
        let entorno = self.entorno();
        let mut todas: Vec<(&str, &str)> = entorno
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        todas.extend_from_slice(variables);
        ejecutar_paso_en(
            cuerpo,
            &std::env::var("PATH").unwrap_or_default(),
            &todas,
            Some(&self.ruta),
        )
    }
}

/// Las salidas (`clave=valor`) que un paso dejó en `GITHUB_OUTPUT`.
fn salidas_del_paso(fichero: &Path) -> BTreeMap<String, String> {
    fs::read_to_string(fichero)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn termux_release_nombra_el_fichero_de_una_candidata_con_guion_igual_que_el_script() {
    let texto = sin_comentarios(&leer_texto(".github/workflows/termux-release.yml"));
    // Nadie compone el nombre con la versión de Debian (la de la tilde).
    assert!(
        !texto.contains("magi-ssh_${VERSION_DEB}") && !texto.contains("magi-ssh_${version_deb}"),
        "el nombre del fichero no se compone con la versión de Debian (lleva tilde)"
    );
    for fijo in [
        "nombre_deb: ${{ steps.etiqueta.outputs.nombre_deb }}",
        "NOMBRE_DEB: ${{ needs.verificar.outputs.nombre_deb }}",
    ] {
        assert!(texto.contains(fijo), "falta «{fijo}»");
    }

    // El paso que decide el nombre, ejecutado de verdad con distintas etiquetas.
    let paso = paso_de(&texto, "Comprobar el formato de la etiqueta");
    let cuerpo = cuerpo_de_run(&paso);
    let camino = std::env::var("PATH").unwrap_or_default();
    let directorio = tempfile::tempdir().expect("directorio temporal");
    let salida_del_paso = directorio.path().join("salidas_del_paso");
    let lanzar = |etiqueta: &str| -> (Resultado, BTreeMap<String, String>) {
        let _ = fs::remove_file(&salida_del_paso);
        let resultado = ejecutar_paso(
            &cuerpo,
            &camino,
            &[
                ("TAG", etiqueta),
                ("GITHUB_OUTPUT", salida_del_paso.to_str().unwrap()),
            ],
        );
        let salidas = fs::read_to_string(&salida_del_paso)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        (resultado, salidas)
    };
    for (etiqueta, version, version_deb, prerelease) in [
        ("v1.6.0", "1.6.0", "1.6.0", "false"),
        ("v1.6.0-rc.1", "1.6.0-rc.1", "1.6.0~rc.1", "true"),
        (
            "v10.20.30-rc.12",
            "10.20.30-rc.12",
            "10.20.30~rc.12",
            "true",
        ),
    ] {
        let (resultado, salidas) = lanzar(etiqueta);
        resultado.exito();
        assert_eq!(salidas["tag"], etiqueta);
        assert_eq!(salidas["version"], version, "{etiqueta}");
        assert_eq!(salidas["version_deb"], version_deb, "{etiqueta}");
        assert_eq!(salidas["prerelease"], prerelease, "{etiqueta}");
        // El nombre lleva la versión de la etiqueta (guion), nunca la tilde...
        assert_eq!(
            salidas["nombre_deb"],
            format!("magi-ssh_{version}_aarch64.deb"),
            "{etiqueta}"
        );
        assert!(!salidas["nombre_deb"].contains('~'), "{etiqueta}");
        // ...y es el que le corresponde a la versión del control.
        assert_eq!(
            salidas["nombre_deb"],
            nombre_de_fichero(&salidas["version_deb"])
        );
    }
    for invalida in [
        "android-v0.3.0",
        "v1.6.0-rc.0",
        "v1.6",
        "v01.6.0",
        "v1.6.0-beta.1",
        "1.6.0",
        "v1.6.0-rc.1;touch x",
    ] {
        let (resultado, salidas) = lanzar(invalida);
        resultado.fallo(1, "no es una etiqueta del paquete de Termux");
        assert!(salidas.is_empty(), "{invalida} no debe dejar salidas");
    }

    // Y el script arma el fichero con el mismo nombre que el workflow espera.
    let Some(esc) = Escenario::nuevo() else {
        return;
    };
    for version in ["1.6.0", "1.6.0-rc.1"] {
        esc.armar(version).exito();
        let (_, salidas) = lanzar(&format!("v{version}"));
        assert!(
            esc.salida.join(&salidas["nombre_deb"]).is_file(),
            "el script no dejó {} (el workflow espera ese nombre): {:?}",
            salidas["nombre_deb"],
            esc.debs_en(&esc.salida)
        );
    }
}

#[test]
fn termux_release_falla_si_un_adjunto_de_la_release_no_se_llama_como_se_esperaba() {
    let texto = sin_comentarios(&leer_texto(".github/workflows/termux-release.yml"));
    let paso = paso_de(&texto, "Comprobar los ficheros de la release");
    assert!(
        !paso.contains("::warning"),
        "un nombre de adjunto distinto es un error, no un aviso"
    );
    let cuerpo = cuerpo_de_run(&paso);

    // Un `gh` falso que lista los adjuntos que se le dan.
    let directorio = tempfile::tempdir().expect("directorio temporal");
    let bin = directorio.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let gh = bin.join("gh");
    fs::write(
        &gh,
        "#!/bin/sh\n\
         [ \"$1 $2\" = 'release view' ] || { echo \"gh falso: $*\" >&2; exit 2; }\n\
         cat \"$GH_FALSO_ADJUNTOS\"\n",
    )
    .unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
    let camino = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let adjuntos = directorio.path().join("adjuntos");
    let comprobar = |nombre_deb: &str, publicados: &[&str]| -> Resultado {
        fs::write(&adjuntos, format!("{}\n", publicados.join("\n"))).unwrap();
        ejecutar_paso(
            &cuerpo,
            &camino,
            &[
                ("TAG", "v1.6.0-rc.1"),
                ("NOMBRE_DEB", nombre_deb),
                ("GH_FALSO_ADJUNTOS", adjuntos.to_str().unwrap()),
            ],
        )
    };

    let deb = "magi-ssh_1.6.0-rc.1_aarch64.deb";
    let suma = "magi-ssh_1.6.0-rc.1_aarch64.deb.sha256";
    let tarball = "magi-ssh-x86_64-unknown-linux-gnu.tar.xz";
    comprobar(deb, &[tarball, deb, suma]).exito();

    for (motivo, publicados) in [
        (
            "GitHub cambió la tilde por un punto",
            vec![
                tarball,
                "magi-ssh_1.6.0.rc.1_aarch64.deb",
                "magi-ssh_1.6.0.rc.1_aarch64.deb.sha256",
            ],
        ),
        ("falta el .deb", vec![tarball, suma]),
        ("falta el .sha256", vec![tarball, deb]),
        (
            "solo está el .deb de otra versión",
            vec![
                tarball,
                "magi-ssh_1.6.0_aarch64.deb",
                "magi-ssh_1.6.0_aarch64.deb.sha256",
            ],
        ),
        ("no hay adjuntos", vec![]),
    ] {
        let resultado = comprobar(deb, &publicados);
        assert_eq!(
            resultado.codigo,
            Some(1),
            "{motivo}: el paso debía fallar.\nstdout: {}\nstderr: {}",
            resultado.salida,
            resultado.error
        );
        assert!(
            resultado.salida.contains("::error"),
            "{motivo}: debe dar un error de GitHub Actions: {}",
            resultado.salida
        );
    }
}

#[test]
fn termux_release_fija_el_commit_de_la_etiqueta_una_vez_y_compilar_descarga_ese_commit() {
    let texto = sin_comentarios(&leer_texto(".github/workflows/termux-release.yml"));

    // El commit viaja de verificar a compilar y de compilar a publicar, y
    // compilar lo descarga por SHA: no vuelve a resolver el nombre de la etiqueta,
    // que pudo moverse entre los dos jobs.
    let verificar = trozo_de_job(&texto, "verificar");
    let compilar = trozo_de_job(&texto, "compilar");
    let publicar = trozo_de_job(&texto, "publicar");
    assert!(
        verificar.contains("commit: ${{ steps.commit.outputs.commit }}"),
        "verificar debe exportar el commit de la etiqueta"
    );
    assert!(
        compilar.contains("commit: ${{ steps.commit_compilado.outputs.commit }}"),
        "compilar debe exportar el commit que compiló"
    );
    assert!(
        publicar.contains("COMMIT_COMPILADO: ${{ needs.compilar.outputs.commit }}"),
        "publicar debe recibir el commit que compiló compilar"
    );
    assert!(
        compilar.contains("ref: ${{ needs.verificar.outputs.commit }}"),
        "compilar debe descargar el commit que fijó verificar"
    );
    assert!(
        !compilar.contains("refs/tags/"),
        "compilar no vuelve a resolver el nombre de la etiqueta"
    );

    let Some(repo) = RepoGit::nuevo() else {
        return;
    };
    repo.escribir("Cargo.toml", "uno\n");
    let a = repo.confirmar("A");
    repo.git(&["tag", "-a", "v1.6.0", "-m", "MAGI 1.6.0"]);
    repo.escribir("Cargo.toml", "dos\n");
    let b = repo.confirmar("B");
    let objeto_de_la_etiqueta = repo.git(&["rev-parse", "refs/tags/v1.6.0"]);
    assert_ne!(
        objeto_de_la_etiqueta, a,
        "una etiqueta anotada tiene su propio objeto: el paso debe dar el commit"
    );
    let directorio = tempfile::tempdir().expect("directorio temporal");
    let salida = directorio.path().join("salidas_del_paso");

    // El paso que fija el commit, ejecutado de verdad.
    let cuerpo = cuerpo_de_run(&paso_de(&texto, "Fijar el commit de la etiqueta"));
    let fijar = |tag: &str, evento: &str, sha: &str| {
        let _ = fs::remove_file(&salida);
        let resultado = repo.paso(
            &cuerpo,
            &[
                ("TAG", tag),
                ("EVENTO", evento),
                ("GITHUB_SHA", sha),
                ("GITHUB_OUTPUT", salida.to_str().unwrap()),
            ],
        );
        (resultado, salidas_del_paso(&salida))
    };

    // Un push de la etiqueta: lo descargado, la etiqueta y GITHUB_SHA coinciden.
    repo.git(&["checkout", "-q", "--detach", &a]);
    let (resultado, salidas) = fijar("v1.6.0", "push", &a);
    resultado.exito();
    assert_eq!(
        salidas["commit"], a,
        "el commit, no el objeto de la etiqueta"
    );
    // GITHUB_SHA es el commit; si GitHub diera el objeto de una etiqueta anotada,
    // tampoco debe fallar una publicación legítima.
    let (resultado, salidas) = fijar("v1.6.0", "push", &objeto_de_la_etiqueta);
    resultado.exito();
    assert_eq!(salidas["commit"], a);

    // Con workflow_dispatch GITHUB_SHA es la punta de la rama, no la etiqueta: no
    // se compara, pero el commit que sale sigue siendo el de la etiqueta.
    let (resultado, salidas) = fijar("v1.6.0", "workflow_dispatch", &b);
    resultado.exito();
    assert_eq!(salidas["commit"], a);

    // La etiqueta se borra y se recrea en otro commit y la ejecución de la
    // etiqueta vieja arranca ahora: descarga el commit nuevo, pero GITHUB_SHA es
    // el viejo. Tiene que fallar, sin dejar el commit como salida.
    repo.git(&["tag", "-f", "-a", "v1.6.0", "-m", "MAGI 1.6.0", &b]);
    repo.git(&["checkout", "-q", "--detach", &b]);
    let (resultado, salidas) = fijar("v1.6.0", "push", &a);
    resultado.fallo(1, "se movió o se volvió a crear");
    assert!(
        salidas.is_empty(),
        "no debe fijar ningún commit: {salidas:?}"
    );
    // La ejecución que sí corresponde a la etiqueta nueva sigue valiendo.
    let (resultado, salidas) = fijar("v1.6.0", "push", &b);
    resultado.exito();
    assert_eq!(salidas["commit"], b);

    // Lo descargado no es el commit de la etiqueta.
    repo.git(&["checkout", "-q", "--detach", &a]);
    let (resultado, salidas) = fijar("v1.6.0", "push", &b);
    resultado.fallo(1, "no es el commit de la etiqueta");
    assert!(salidas.is_empty(), "{salidas:?}");

    // Una etiqueta que no existe no deja pasar nada.
    let (resultado, salidas) = fijar("v9.9.9", "push", &a);
    assert_ne!(resultado.codigo, Some(0), "una etiqueta inexistente");
    assert!(salidas.is_empty(), "{salidas:?}");

    // El paso de compilar, que comprueba lo que descargó y lo anota para publicar.
    let cuerpo = cuerpo_de_run(&paso_de(&texto, "Anotar el commit que se compila"));
    let anotar = |esperado: &str| {
        let _ = fs::remove_file(&salida);
        let resultado = repo.paso(
            &cuerpo,
            &[
                ("COMMIT_ESPERADO", esperado),
                ("TAG", "v1.6.0"),
                ("GITHUB_OUTPUT", salida.to_str().unwrap()),
            ],
        );
        (resultado, salidas_del_paso(&salida))
    };
    let (resultado, salidas) = anotar(&a);
    resultado.exito();
    assert_eq!(salidas["commit"], a);
    assert!(
        resultado.salida.contains(&a),
        "el log debe decir qué commit se compila: {}",
        resultado.salida
    );
    let (resultado, salidas) = anotar(&b);
    resultado.fallo(1, "«verificar» fijó");
    assert!(salidas.is_empty(), "{salidas:?}");
}

/// Un `gh` falso para el paso que sube el `.deb`: anota cada llamada, contesta
/// `gh api` con el SHA que se le da (o falla) y deja subir (o falla).
fn gh_falso_de_publicar(directorio: &Path) -> PathBuf {
    let bin = directorio.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let gh = bin.join("gh");
    fs::write(
        &gh,
        "#!/bin/sh\n\
         echo \"$*\" >> \"$GH_FALSO_REGISTRO\"\n\
         case \"$1\" in\n\
           api)\n\
             if [ -n \"$GH_FALSO_API_FALLA\" ]; then echo \"$GH_FALSO_API_FALLA\" >&2; exit 1; fi\n\
             printf '%s\\n' \"$GH_FALSO_SHA\" ;;\n\
           release)\n\
             [ \"$2\" = upload ] || { echo \"gh falso: $*\" >&2; exit 2; }\n\
             [ -z \"$GH_FALSO_SUBIDA_FALLA\" ] || exit 1 ;;\n\
           *) echo \"gh falso: $*\" >&2; exit 2 ;;\n\
         esac\n",
    )
    .unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

#[test]
fn termux_release_no_sube_el_deb_si_la_etiqueta_ya_no_apunta_al_commit_compilado() {
    let texto = sin_comentarios(&leer_texto(".github/workflows/termux-release.yml"));
    let paso = paso_de(&texto, "Subir el .deb y su .sha256 a la release");
    // La comprobación va dentro del paso que sube y antes de subir: no hay
    // hueco entre comprobar y subir en el que otro paso pueda colarse.
    let cuerpo = cuerpo_de_run(&paso);
    let comprobacion = cuerpo.find("gh api").expect("el paso consulta la etiqueta");
    let subida = cuerpo.find("gh release upload").expect("el paso sube");
    assert!(
        comprobacion < subida,
        "primero se comprueba y luego se sube"
    );

    let directorio = tempfile::tempdir().expect("directorio temporal");
    let bin = gh_falso_de_publicar(directorio.path());
    let camino = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let registro = directorio.path().join("registro.txt");
    let a = "a".repeat(40);
    let b = "b".repeat(40);
    let deb = "magi-ssh_1.6.0_aarch64.deb";
    let consulta = "api repos/4d3-org/magi/commits/v1.6.0 --jq .sha";
    let subida = format!("release upload v1.6.0 {deb} {deb}.sha256");
    let lanzar = |compilado: &str, extra: &[(&str, &str)]| -> (Resultado, String) {
        let _ = fs::remove_file(&registro);
        let mut entorno = vec![
            ("TAG", "v1.6.0"),
            ("NOMBRE_DEB", deb),
            ("GH_REPO", "4d3-org/magi"),
            ("COMMIT_COMPILADO", compilado),
            ("GH_FALSO_REGISTRO", registro.to_str().unwrap()),
            ("GH_FALSO_SHA", a.as_str()),
        ];
        entorno.extend_from_slice(extra);
        let resultado = ejecutar_paso(&cuerpo, &camino, &entorno);
        (resultado, fs::read_to_string(&registro).unwrap_or_default())
    };

    // La etiqueta sigue donde se compiló: se consulta y se sube.
    let (resultado, llamadas) = lanzar(&a, &[]);
    resultado.exito();
    assert_eq!(
        llamadas.lines().collect::<Vec<_>>(),
        [consulta, subida.as_str()],
        "primero la consulta de la etiqueta y luego la subida"
    );

    // La etiqueta se borró y se recreó en otro commit mientras se esperaba: el
    // .deb es de otro commit y no se sube, con un mensaje que explica qué pasó.
    let (resultado, llamadas) = lanzar(&b, &[]);
    resultado.fallo(1, "la etiqueta v1.6.0 apunta ahora a");
    assert!(
        resultado.error.contains(&b) && resultado.error.contains(&a),
        "el error debe nombrar los dos commits: {}",
        resultado.error
    );
    assert!(
        resultado.error.contains("se borró y se volvió a crear")
            && resultado.error.contains("workflow_dispatch"),
        "el error debe explicar qué pasó y cómo seguir: {}",
        resultado.error
    );
    assert_eq!(llamadas.lines().collect::<Vec<_>>(), [consulta]);
    assert!(!llamadas.contains("upload"), "no debe subir nada");

    // Si no se puede leer la etiqueta, tampoco se sube: ante la duda, no.
    let (resultado, llamadas) = lanzar(&a, &[("GH_FALSO_API_FALLA", "HTTP 502: Bad Gateway")]);
    resultado.fallo(1, "no se pudo leer a qué commit apunta la etiqueta v1.6.0");
    assert!(resultado.error.contains("HTTP 502"), "{}", resultado.error);
    assert!(!llamadas.contains("upload"), "{llamadas}");
    // Ni con una respuesta que no es un commit.
    let (resultado, llamadas) = lanzar(&a, &[("GH_FALSO_SHA", "no es un sha")]);
    resultado.fallo(1, "GitHub no devolvió un commit");
    assert!(!llamadas.contains("upload"), "{llamadas}");

    // Si compilar no dejó el commit, no se llega ni a consultar.
    for vacio in ["", "no-es-un-sha", "ABCDEF"] {
        let (resultado, llamadas) = lanzar(vacio, &[]);
        resultado.fallo(1, "el job compilar no dejó el commit");
        assert_eq!(llamadas, "", "sin commit compilado no se llama a gh");
    }

    // Y un fallo de la subida sigue siendo un fallo, con su explicación.
    let (resultado, llamadas) = lanzar(&a, &[("GH_FALSO_SUBIDA_FALLA", "1")]);
    resultado.fallo(1, "no se pudo subir a la release v1.6.0");
    assert!(llamadas.contains("upload"), "{llamadas}");
}

#[test]
fn termux_release_usa_solo_github_token_y_nunca_toca_latest() {
    let texto = sin_comentarios(&leer_texto(".github/workflows/termux-release.yml"));
    assert!(
        !texto.contains("secrets: inherit"),
        "nunca secrets: inherit"
    );
    for (indice, _) in texto.match_indices("secrets.") {
        assert!(
            texto[indice..].starts_with("secrets.GITHUB_TOKEN"),
            "solo GITHUB_TOKEN: {}",
            &texto[indice..indice + 30]
        );
    }
    assert!(
        texto.contains("\npermissions:\n  contents: read\n"),
        "contents: read por defecto"
    );
    // La release la crea dist: este workflow solo sube, sin sustituir ni tocar «Latest».
    for ausente in [
        "gh release create",
        "gh release edit",
        "--latest",
        "--clobber",
    ] {
        assert!(
            !texto.contains(ausente),
            "termux-release.yml no debe llevar «{ausente}»"
        );
    }
    for presente in [
        "gh release upload",
        "actions/attest-build-provenance@v4",
        "scripts/paquete-termux.sh --comprobar-binario",
        "scripts/paquete-termux.sh \"$VERSION\"",
        "cargo ndk -t arm64-v8a --platform 24 build --release -p magi-ssh --locked",
        "release not found",
        "workflow_dispatch",
    ] {
        assert!(
            texto.contains(presente),
            "termux-release.yml debe llevar «{presente}»"
        );
    }
}

#[test]
fn las_herramientas_del_workflow_van_con_las_mismas_versiones_y_sumas_que_las_de_android() {
    let valor = |texto: &str, clave: &str| -> String {
        texto
            .lines()
            .find_map(|l| {
                l.trim()
                    .strip_prefix(&format!("{clave}: "))
                    .map(|v| v.trim_matches('\'').to_string())
            })
            .unwrap_or_else(|| panic!("falta {clave}"))
    };
    let android = leer_texto(".github/workflows/android-release.yml");
    for fichero in [
        ".github/workflows/termux-release.yml",
        ".github/workflows/ci.yml",
    ] {
        let texto = leer_texto(fichero);
        for clave in [
            "NDK_VERSION",
            "CARGO_NDK_VERSION",
            "CARGO_NDK_X86_64_LINUX_MUSL_SHA256",
        ] {
            assert_eq!(
                valor(&texto, clave),
                valor(&android, clave),
                "{clave} en {fichero}"
            );
        }
        assert!(
            texto.contains("sha256sum -c -"),
            "{fichero}: cargo-ndk se descarga a fichero y se comprueba su SHA-256"
        );
    }
}

/// Los patrones del `case` con que el job `cambios-termux` decide si una PR toca
/// el paquete: uno por alternativa, tal cual (`src/*`, `Cargo.toml`…).
fn rutas_del_filtro(texto: &str) -> Vec<String> {
    let desde = texto
        .find("\n  cambios-termux:\n")
        .expect("falta el job cambios-termux");
    let job = &texto[desde..];
    let inicio = "case \"$fichero\" in";
    let caso = job.find(inicio).expect("falta el case del filtro") + inicio.len();
    let resto = &job[caso..];
    // Los patrones acaban en el primer «)», que ninguno lleva.
    let fin = resto
        .find(')')
        .expect("el case no cierra su lista de patrones");
    resto[..fin]
        .replace("\\\n", " ")
        .split('|')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

#[test]
fn el_filtro_de_rutas_de_ci_cubre_todo_lo_que_cambia_el_binario_o_el_contenido_del_deb() {
    let texto = leer_texto(".github/workflows/ci.yml");
    let rutas = rutas_del_filtro(&texto);
    // Las del checklist y, además (desviación del checklist): lo que interviene al
    // compilar el binario (build.rs, vendor), lo que lleva embebido (visor/dist,
    // assets), lo que va al .deb tal cual (README y licencias) y el propio
    // workflow de publicación.
    for ruta in [
        "src/*",
        "crates/magi-nucleo/*",
        "Cargo.toml",
        "Cargo.lock",
        "packaging/termux/*",
        "scripts/paquete-termux.sh",
        "tests/termux_paquete.rs",
        ".github/workflows/ci.yml",
        "build.rs",
        "vendor/*",
        "visor/dist/*",
        "assets/*",
        "README.md",
        "LICENSE-*",
        ".github/workflows/termux-release.yml",
    ] {
        assert!(
            rutas.iter().any(|r| r == ruta),
            "el filtro de rutas de cambios-termux no lleva «{ruta}»: {rutas:?}"
        );
    }
    // Cada ruta que entra en el .deb tiene que quedar cubierta por algún patrón.
    for (_, fichero) in FICHEROS {
        let origen = match fichero {
            "bin/magi" => continue,
            f if f.ends_with("magi.1.gz") => "assets/man/magi.1",
            f if f.contains("completion") || f.contains("site-functions") => "assets/completions/x",
            f if f.ends_with("LICENSE-MIT") => "LICENSE-MIT",
            f if f.ends_with("LICENSE-APACHE") => "LICENSE-APACHE",
            f if f.ends_with("README.md") => "README.md",
            otro => panic!("fichero del .deb sin origen conocido en la prueba: {otro}"),
        };
        let cubierta = rutas.iter().any(|patron| match patron.strip_suffix('*') {
            Some(prefijo) => origen.starts_with(prefijo),
            None => origen == patron,
        });
        assert!(
            cubierta,
            "{origen} entra en el .deb y el filtro de cambios-termux no lo cubre"
        );
    }
    // La guía lo cuenta igual.
    let guia = leer_texto("docs/publicar-termux.md");
    for mencion in [
        "`build.rs`",
        "`vendor/`",
        "`visor/dist/`",
        "`assets/`",
        "`termux-release.yml`",
    ] {
        assert!(
            guia.contains(mencion),
            "docs/publicar-termux.md debe listar {mencion} entre lo que dispara el job termux"
        );
    }
}

#[test]
fn el_job_termux_de_ci_corre_solo_en_pr_que_tocan_el_paquete_y_exige_el_deb() {
    let texto = leer_texto(".github/workflows/ci.yml");
    assert!(texto.contains("\n  termux:\n"), "falta el job termux");
    assert!(
        texto.contains("needs: cambios-termux"),
        "el job termux depende del filtro"
    );
    assert!(
        texto.contains("if: needs.cambios-termux.outputs.termux == 'true'"),
        "el job termux solo corre si el filtro lo dice"
    );
    assert!(
        texto.contains("if: github.event_name == 'pull_request'"),
        "el filtro es de las PR"
    );
    for presente in [
        "cargo ndk -t arm64-v8a --platform 24 build --release -p magi-ssh --locked",
        "cargo ndk -t x86_64 --platform 24 build --release -p magi-ssh --locked",
        "scripts/paquete-termux.sh",
        "MAGI_EXIGIR_DEB: '1'",
        "MAGI_DEB: ${{ steps.deb.outputs.deb }}",
        "--test termux_paquete",
        "target/x86_64-linux-android/release/magi",
    ] {
        assert!(
            texto.contains(presente),
            "el job termux de ci.yml debe llevar «{presente}»"
        );
    }
    // El job termux no publica nada.
    let desde = texto.find("\n  termux:\n").unwrap();
    let job = &texto[desde..];
    for ausente in ["gh release", "contents: write", "secrets."] {
        assert!(
            !job.contains(ausente),
            "el job termux no debe llevar «{ausente}»"
        );
    }
}

#[test]
fn el_filtro_de_ci_ve_los_ficheros_vigilados_que_una_pr_mueve_fuera_de_sus_rutas() {
    let texto = leer_texto(".github/workflows/ci.yml");
    let paso = paso_de(&sin_comentarios(&texto), "Mirar qué ficheros cambia la PR");
    let cuerpo = cuerpo_de_run(&paso);
    assert!(
        cuerpo.contains("git diff --name-only --no-renames HEAD^1 HEAD"),
        "la lista de ficheros de la PR no debe pasar por la detección de renombrados"
    );

    let Some(repo) = RepoGit::nuevo() else {
        return;
    };
    // Fichero grande y sin nada que lo distinga de otro: git lo empareja como
    // renombrado (con la detección activa) y lista solo la ruta nueva.
    let contenido = "pub fn hola() {}\n".repeat(40);
    repo.escribir("src/vigilado.rs", &contenido);
    repo.escribir("docs/nota.md", "nota\n");
    let base = repo.confirmar("base");
    let directorio = tempfile::tempdir().expect("directorio temporal");
    let salida = directorio.path().join("salidas_del_paso");
    let filtro = |repo: &RepoGit| -> String {
        let _ = fs::remove_file(&salida);
        repo.paso(&cuerpo, &[("GITHUB_OUTPUT", salida.to_str().unwrap())])
            .exito();
        salidas_del_paso(&salida)["termux"].clone()
    };
    // Como en una PR de actions/checkout: HEAD es la fusión de la base (primer
    // padre) con la cabeza de la PR (segundo).
    let fusionar = |rama: &str| {
        repo.git(&["checkout", "-q", "main"]);
        repo.git(&["merge", "-q", "--no-ff", "-m", "fusión", rama]);
    };

    // Una PR que solo toca documentación no activa el job...
    repo.git(&["checkout", "-q", "-b", "documentacion"]);
    repo.escribir("docs/nota.md", "otra nota\n");
    repo.confirmar("documentación");
    fusionar("documentacion");
    assert_eq!(filtro(&repo), "false", "una PR de documentación");

    // ...y una que mueve un fichero vigilado fuera de las rutas del filtro, sí:
    // sin --no-renames, git solo lista la ruta nueva y la vieja pasa inadvertida.
    repo.git(&["checkout", "-q", "-b", "mover"]);
    repo.git(&["mv", "src/vigilado.rs", "docs/vigilado.rs"]);
    repo.confirmar("mover");
    fusionar("mover");
    assert_eq!(
        repo.git(&["diff", "--name-only", "HEAD^1", "HEAD"]),
        "docs/vigilado.rs",
        "la premisa de la prueba: con la detección de renombrados solo sale la ruta nueva"
    );
    assert_eq!(filtro(&repo), "true", "una PR que mueve src/vigilado.rs");

    // Sin fusión (HEAD no tiene segundo padre) se asume que toca Termux.
    repo.git(&["checkout", "-q", "--detach", &base]);
    assert_eq!(filtro(&repo), "true", "HEAD no es una fusión");
}

#[test]
fn la_receta_de_tur_compila_el_paquete_del_tag_e_instala_lo_mismo_que_el_deb() {
    let receta = leer_texto("packaging/termux/build.sh");
    for presente in [
        "termux_setup_rust",
        "TERMUX_PKG_EXCLUDED_ARCHES=\"arm, i686\"",
        "TERMUX_PKG_SUGGESTS=\"termux-api, openssh\"",
        "TERMUX_PKG_HOMEPAGE=https://github.com/4d3-org/magi",
        "https://github.com/4d3-org/magi/archive/refs/tags/v${TERMUX_PKG_VERSION}.tar.gz",
        "TERMUX_PKG_LICENSE_FILE=\"LICENSE-APACHE, LICENSE-MIT\"",
        "-p magi-ssh",
        "--locked",
        "share/man/man1/magi.1",
        "share/bash-completion/completions/magi",
        "share/zsh/site-functions/_magi",
        "share/fish/vendor_completions.d/magi.fish",
        "${TERMUX_PREFIX}/bin/magi",
    ] {
        assert!(
            receta.contains(presente),
            "la receta de TUR debe llevar «{presente}»"
        );
    }
    // La suma es un marcador (64 ceros) o una suma de verdad (64 hexadecimales):
    // nunca otra cosa.
    let suma = receta
        .lines()
        .find_map(|l| l.strip_prefix("TERMUX_PKG_SHA256="))
        .expect("TERMUX_PKG_SHA256");
    assert!(
        suma.len() == 64 && suma.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
        "TERMUX_PKG_SHA256 debe ser una suma SHA-256 o su marcador de ceros: «{suma}»"
    );
    let version = receta
        .lines()
        .find_map(|l| l.strip_prefix("TERMUX_PKG_VERSION="))
        .expect("TERMUX_PKG_VERSION")
        .trim_matches('"');
    assert!(
        version_de_debian_valida(version) && !version.contains('~'),
        "versión estable en la receta: {version}"
    );
    assert!(
        !receta
            .lines()
            .any(|l| l.trim_start().starts_with("TERMUX_PKG_AUTO_UPDATE")),
        "no se activa la actualización automática"
    );
}

/// Un nombre de usuario que puede aparecer en un documento público: el de la
/// documentación (CLAUDE.md) o un marcador (`$USER`, `<usuario>`, `{usuario}`,
/// `~`, `*`…), nunca un nombre real.
fn usuario_de_documentacion(segmento: &str) -> bool {
    segmento.is_empty()
        || segmento == "usuario"
        || segmento.starts_with(['$', '<', '{', '%', '~', '*', '…', '.'])
}

/// ¿Es una dirección de correo de documentación o de un servicio (no una
/// persona)? Dominios de documentación, el `noreply` de GitHub y los nombres de
/// extensiones de OpenSSH (`posix-rename@openssh.com`).
fn correo_de_documentacion(local: &str, dominio: &str) -> bool {
    local == "usuario"
        || dominio == "ejemplo.org"
        || dominio.ends_with(".ejemplo.org")
        || matches!(dominio, "example.org" | "example.com" | "example.net")
        || dominio.ends_with(".noreply.github.com")
        || dominio == "openssh.com"
        || dominio.starts_with("192.0.2.")
}

/// Datos personales que no deben llegar a un fichero del repositorio, buscados
/// por patrón (no por un nombre concreto, que sería ponerlo en el repositorio):
/// una ruta `/home/<x>` o `/Users/<x>` que no sea la de documentación o un
/// marcador, y una dirección de correo que no sea de documentación.
fn datos_personales(texto: &str) -> Vec<String> {
    let mut halladas = Vec::new();
    // Rutas de usuario: solo si empiezan una ruta (no `…/files/home/…`).
    for prefijo in ["/home/", "/Users/"] {
        for (indice, _) in texto.match_indices(prefijo) {
            let sigue_otra_ruta = texto[..indice].chars().next_back().is_some_and(|c| {
                c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '~' | '}' | '/')
            });
            if sigue_otra_ruta {
                continue;
            }
            let segmento: String = texto[indice + prefijo.len()..]
                .chars()
                .take_while(|c| {
                    !c.is_whitespace()
                        && !matches!(
                            c,
                            '/' | '"' | '\'' | '`' | ')' | ']' | ';' | ',' | ':' | '>'
                        )
                })
                .collect();
            if !usuario_de_documentacion(&segmento) {
                halladas.push(format!("{prefijo}{segmento}"));
            }
        }
    }
    // Correos: `local@dominio.tld`.
    let es_local = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-');
    let es_dominio = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-');
    for (indice, _) in texto.match_indices('@') {
        let local: String = texto[..indice]
            .chars()
            .rev()
            .take_while(|c| es_local(*c))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let dominio: String = texto[indice + 1..]
            .chars()
            .take_while(|c| es_dominio(*c))
            .collect();
        let dominio = dominio.trim_end_matches('.');
        if local.is_empty() || !dominio.contains('.') || dominio.starts_with('.') {
            continue;
        }
        if !correo_de_documentacion(&local, dominio) {
            halladas.push(format!("{local}@{dominio}"));
        }
    }
    halladas
}

#[test]
fn el_detector_de_datos_personales_busca_por_patron_y_no_confunde_la_documentacion() {
    // Lo que debe detectar: cualquier usuario real, el que sea.
    for malo in [
        "/home/otro",
        "/home/otro/.ssh/config",
        "ruta: /home/otro/proyectos",
        "`/home/otro`",
        "(/home/otro)",
        "/Users/otro",
        "/Users/otro/Library/Application Support",
        "cd /Users/otro; ls",
        "\"/home/otra.persona/x\"",
        "escribe a otro@dominio-real.es para pedirlo",
        "Maintainer: otro.nombre@gmail.com",
    ] {
        assert!(
            !datos_personales(malo).is_empty(),
            "debía detectar datos personales en «{malo}»"
        );
    }
    // Lo que vale: la documentación de CLAUDE.md, los marcadores y las rutas de
    // Termux, que no son de una persona.
    for bueno in [
        "/home/usuario",
        "/home/usuario/.ssh/config",
        "/Users/usuario/Library",
        "$HOME/.ssh y ${HOME}/.config",
        "/home/$USER/proyectos",
        "/home/<usuario>/x y /Users/<usuario>/y",
        "/home/{usuario}/x",
        "/data/data/com.termux/files/home/.termux/termux.properties",
        "${PREFIX}/home/x y ~/home/x",
        "scp usuario@192.0.2.10:fichero .",
        "usuario@ejemplo.org y soporte@ejemplo.org",
        "4d3-org@users.noreply.github.com",
        "posix-rename@openssh.com",
        "wg-quick@*.service y \"${#faltan[@]}\" y TERMUX_PKG_MAINTAINER=\"@4d3-org\"",
        "https://github.com/4d3-org/magi/archive/refs/tags/v1.6.0.tar.gz",
    ] {
        assert_eq!(
            datos_personales(bueno),
            Vec::<String>::new(),
            "«{bueno}» es documentación y no debía detectarse"
        );
    }
}

#[test]
fn la_guia_y_publicar_md_cuentan_lo_que_hacen_de_verdad_los_workflows() {
    // La guía de Termux: la etiqueta de una candidata, el nombre de su fichero y el
    // riesgo de las acciones por etiqueta.
    let guia = leer_texto("docs/publicar-termux.md");
    // Sin los saltos de línea ni las sangrías de las listas.
    let plana = guia.split_whitespace().collect::<Vec<_>>().join(" ");
    for presente in [
        "version = \"X.Y.Z-rc.N\"",
        "idéntica a la `version` de `Cargo.toml`",
        "no sigue la convención de Debian",
        "Riesgo conocido y mejora pendiente",
        "fijar las acciones por SHA en todos los workflows",
        "un job propio que solo use `actions/download-artifact` y `actions/attest-build-provenance`",
    ] {
        assert!(
            plana.contains(presente),
            "docs/publicar-termux.md debe decir «{presente}»"
        );
    }
    assert!(
        !guia.contains("~rc.1_aarch64") && !guia.contains("~rc.N_aarch64"),
        "el fichero de una candidata no lleva tilde en su nombre"
    );

    // docs/publicar.md: los permisos de cada job que escribe, tal como están en
    // los workflows (no «solo en release.yml»).
    let publicar = leer_texto("docs/publicar.md");
    let permisos = publicar
        .split("\nPermisos: ")
        .nth(1)
        .expect("falta el párrafo «Permisos» de docs/publicar.md")
        .split("\n`release.yml` **sí se mantiene")
        .next()
        .unwrap();
    assert!(
        !permisos.contains("solo en los jobs de `release.yml`"),
        "el párrafo de permisos ya no es cierto: hay más workflows que escriben"
    );
    for fichero in ["release.yml", "android-release.yml", "termux-release.yml"] {
        assert!(
            permisos.contains(&format!("| `{fichero}` |")),
            "los permisos de docs/publicar.md no hablan de {fichero}"
        );
        let texto = sin_comentarios(&leer_texto(&format!(".github/workflows/{fichero}")));
        for job in nombres_de_jobs(&texto) {
            let propios = permisos_de(&trozo_de_job(&texto, &job), 4);
            for (permiso, valor) in &propios {
                if valor == "write" {
                    assert!(
                        permisos.contains(&format!("`{job}`")),
                        "{fichero}: el job {job} tiene {permiso}: write y docs/publicar.md no lo lista"
                    );
                }
            }
        }
    }
}

/// El texto de un documento sin saltos de línea ni sangrías: para buscar frases
/// que el formato parte a 80 columnas.
fn aplanado(texto: &str) -> String {
    texto.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// La sección `## <numero> …` de un documento, hasta la siguiente (la tabla de
/// canales de antes del primer apartado, con `""`).
fn seccion_de(texto: &str, numero: &str) -> String {
    if numero.is_empty() {
        return texto.split("\n## ").next().unwrap().to_string();
    }
    let cabecera = format!("\n## {numero} ");
    let desde = texto
        .find(&cabecera)
        .unwrap_or_else(|| panic!("falta la sección {numero}"));
    let resto = &texto[desde + 1..];
    resto.split("\n## ").next().unwrap().to_string()
}

#[test]
fn las_guias_de_publicacion_cuentan_como_se_recupera_una_publicacion_y_donde_sale_el_deb() {
    // docs/publicar-termux.md: lo que hay que saber para rescatar una publicación.
    let guia = leer_texto("docs/publicar-termux.md");
    let probar = seccion_de(&guia, "5.");
    let repetir = aplanado(&seccion_de(&guia, "8."));
    let fallos = aplanado(&seccion_de(&guia, "9."));
    let probar = aplanado(&probar);
    for (donde, texto, presente) in [
        // F3: la primera publicación es una candidata y se arma antes con dpkg-deb real.
        (
            "§5",
            &probar,
            "La primera publicación de Termux tiene que ser siempre una candidata `-rc.N`",
        ),
        ("§5", &probar, "Ejecuta el script con `dpkg-deb` real"),
        // F3: Re-run failed jobs frente a un nuevo dispatch.
        ("§8", &repetir, "«Re-run failed jobs»"),
        ("§8", &repetir, "mismo workflow, la misma etiqueta y el mismo commit"),
        ("§8", &repetir, "empieza una ejecución nueva: compila y atesta otra vez"),
        // F3: de dónde sale lo que se compila.
        (
            "§8",
            &repetir,
            "El script y todo lo que se compila salen siempre del árbol de la etiqueta",
        ),
        ("§8", &repetir, "`--ref main` solo cambia el YAML del workflow"),
        // F2: la atestación de un dispatch.
        (
            "§8",
            &repetir,
            "lleva el commit de la rama desde la que se lanzó",
        ),
        // F3: el armado con dpkg-deb real en la etiqueta estable.
        ("§9", &fallos, "Falla el armado del .deb (script o estructura)"),
        ("§9", &fallos, "solo arregla fallos del YAML"),
        (
            "§9",
            &fallos,
            "**sin `.deb`**, y solo se arregla con un PATCH nuevo",
        ),
        ("§9", &fallos, "la primera publicación es siempre una candidata `-rc.N`"),
        // F1: la etiqueta recreada con una ejecución suya en marcha.
        ("§9", &fallos, "el .deb se compiló desde el commit"),
        (
            "§9",
            &fallos,
            "No se recrea una etiqueta con una ejecución de «Release Termux» suya en curso o en cola: cancélala antes",
        ),
        ("§9", &fallos, "mejor **«Re-run failed jobs»** del workflow que falló"),
    ] {
        assert!(
            texto.contains(presente),
            "docs/publicar-termux.md, {donde}, debe decir «{presente}»"
        );
    }

    // docs/publicar.md: el .deb en los canales, el flujo, §7 y las comprobaciones.
    let publicar = leer_texto("docs/publicar.md");
    let canales = aplanado(&seccion_de(&publicar, ""));
    let flujo = aplanado(&seccion_de(&publicar, "5."));
    let canal_caido = aplanado(&seccion_de(&publicar, "7."));
    let comprobaciones = aplanado(&seccion_de(&publicar, "9."));
    for (donde, texto, presente) in [
        ("la tabla de canales", &canales, "`magi-ssh_X.Y.Z_aarch64.deb`"),
        ("la tabla de canales", &canales, "`termux-release.yml`"),
        ("§5", &flujo, "Vigila también Release Termux; ver [`publicar-termux.md`]"),
        (
            "§7",
            &canal_caido,
            "No recrees un tag con una ejecución de «Release Termux» suya en curso o en cola: cancélala antes",
        ),
        ("§7", &canal_caido, "es mejor **Re-run failed jobs** del workflow que falló"),
        ("§9", &comprobaciones, "el `.deb` y su `.sha256` para Termux"),
    ] {
        assert!(
            texto.contains(presente),
            "docs/publicar.md, {donde}, debe decir «{presente}»"
        );
    }
}

#[test]
fn la_documentacion_del_paquete_cubre_lo_que_pide_el_checklist_y_no_lleva_datos_personales() {
    let guia = leer_texto("docs/publicar-termux.md");
    for presente in [
        "workflow_dispatch",
        "sha256sum -c",
        "apt install",
        "magi --version",
        "TERMUX_PKG_SHA256",
        "sha256sum",
        "-rc.N",
        "Nunca crea la release",
        "abre la PR a TUR",
    ] {
        assert!(
            guia.contains(presente),
            "docs/publicar-termux.md debe hablar de «{presente}»"
        );
    }
    let readme = leer_texto("README.md");
    let seccion = readme
        .split("\n## MAGI en Termux\n")
        .nth(1)
        .expect("falta la sección «MAGI en Termux» del README")
        .split("\n## ")
        .next()
        .unwrap();
    // Los párrafos del README se parten a 80 columnas: se compara sin saltos.
    let plana = seccion.replace('\n', " ");
    for presente in [
        "apt install ./magi-ssh_",
        "sha256sum -c",
        "pkg install tur-repo",
        "pkg install magi-ssh",
        "termux-api",
        "OSC 52",
        "termux-setup-storage",
        "magi termux teclas --instalar",
        "Volumen arriba",
        "frase maestra",
        "no se recupera",
        "Android 12 o posterior",
        "magi diagnostico",
    ] {
        assert!(
            plana.contains(presente),
            "la sección del README debe decir «{presente}»"
        );
    }
    // Nada con el usuario del desarrollador (CLAUDE.md): rutas con $HOME o de
    // documentación. Se busca por patrón (cualquier /home/<x> o /Users/<x> que no
    // sea de documentación y cualquier correo que no lo sea), sin escribir en el
    // repositorio ningún nombre real.
    let publicable = [
        ("docs/publicar-termux.md", guia),
        ("README.md", seccion.to_string()),
        (
            "packaging/termux/build.sh",
            leer_texto("packaging/termux/build.sh"),
        ),
        (
            "scripts/paquete-termux.sh",
            leer_texto("scripts/paquete-termux.sh"),
        ),
        (
            ".github/workflows/termux-release.yml",
            leer_texto(".github/workflows/termux-release.yml"),
        ),
    ];
    for (nombre, texto) in publicable {
        let halladas = datos_personales(&texto);
        assert!(
            halladas.is_empty(),
            "{nombre} lleva datos personales: {halladas:?}"
        );
    }
}
