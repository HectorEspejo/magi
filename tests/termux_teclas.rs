//! La fila de teclas de Termux (Fase 17, Sprint 3, §4.8, §5.4, §5.5, §6.4 y
//! §7.7): la fila propuesta, la fusión de `termux.properties`, la instalación
//! con copia y escritura atómica, `termux-reload-settings`, `magi termux
//! teclas [--instalar]` con una consola de mentira y el diálogo FILA DE TECLAS
//! DE TERMUX con `App::de_prueba` (paleta, primer arranque, ayuda y REGISTRO).
//!
//! Sin Termux, sin red y sin tocar el HOME real: todo en directorios
//! temporales con los dobles de las órdenes `termux-*` de
//! `tests/fijos/termux/`. `es_termux()` se decide una vez por proceso, por eso
//! esto vive en su propio binario y todas las pruebas empiezan por
//! `como_termux()`. Solo vale en compilaciones de depuración (donde
//! `MAGI_PLATAFORMA=termux` activa algo). Las instantáneas del diálogo están en
//! `tests/redimensionado/vistas_q.rs`.
#![cfg(debug_assertions)]

#[path = "fijos/termux/mod.rs"]
mod dobles;

use std::collections::VecDeque;
use std::ffi::OsString;
use std::io::{Read as _, Write as _};
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Output, Stdio};
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use magi::app::termux_teclas::{
    tomar_marca_del_primer_arranque, DialogoTeclasTermux, EventoTeclas, ETIQUETA_PALETA,
    MENSAJE_PRIMER_ARRANQUE,
};
use magi::app::{App, Dialogo, Evento};
use magi::config::{Config, Rutas};
use magi::llavero_cli::{Consola, ErrorEntrada};
use magi::modelo::{DatosHost, Origen};
use magi::protocolo::{MensajeServidor, VERSION_PROTOCOLO};
use magi::tema::Tema;
use magi::termux::cli::orden_teclas;
use magi::termux::estado::{self, Marca};
use magi::termux::teclas::{
    self, ErrorTeclas, EstadoFila, COMENTARIO_FILA, ORDEN_RECARGAR, REINICIA_TERMUX,
};
use magi::termux::{es_termux, ApiProceso, EntornoTermux, EstadoApi, FalloOrden, Ordenes};
use magi_nucleo::llavero_fichero::{LlaveroFichero, NOMBRE_FICHERO};
use zeroize::Zeroizing;

use dobles::{Dobles, Modo};

/// La fila por defecto del informe (§5.4), con el prefijo `Ctrl+]`.
const FILA: &str = "extra-keys = [['ESC','TAB','CTRL','ALT',{macro: 'CTRL ]', display: '^]'},'LEFT','DOWN','UP','RIGHT'],['F1','F2','F3','F4','F5','F6','F7','F8','F9']]";
const PREFIJO: &str = "Ctrl+]";
const COPIA: &str = "termux.properties.magi-2026-11-17";
const FRASE: &str = "una frase maestra larga";

/// Pone `MAGI_PLATAFORMA=termux` una sola vez, antes de la primera pregunta.
fn como_termux() {
    static UNA_VEZ: Once = Once::new();
    UNA_VEZ.call_once(|| std::env::set_var("MAGI_PLATAFORMA", "termux"));
    assert!(
        es_termux(),
        "es_termux() ya se decidió sin MAGI_PLATAFORMA=termux en este proceso"
    );
}

fn fecha() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 17).expect("fecha")
}

fn modo(ruta: &Path) -> u32 {
    std::fs::metadata(ruta)
        .expect("metadatos")
        .permissions()
        .mode()
        & 0o777
}

/// Cuántas entradas hay en un directorio (para ver que no sobra ni falta
/// ningún fichero).
fn entradas(directorio: &Path) -> Vec<String> {
    let mut nombres: Vec<String> = std::fs::read_dir(directorio)
        .expect("directorio")
        .map(|entrada| {
            entrada
                .expect("entrada")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    nombres.sort();
    nombres
}

/// Un directorio con permisos de solo lectura mientras vive; si el usuario es
/// root, la prueba que lo usa no demuestra nada y se omite.
struct SoloLectura(PathBuf);

impl SoloLectura {
    fn de(directorio: &Path) -> Option<Self> {
        if nix::unistd::Uid::effective().is_root() {
            eprintln!("aviso: se omite: con root no hay directorios que no se puedan escribir");
            return None;
        }
        std::fs::set_permissions(directorio, std::fs::Permissions::from_mode(0o500))
            .expect("permisos");
        Some(Self(directorio.to_path_buf()))
    }
}

impl Drop for SoloLectura {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o700));
    }
}

// ------------------------------------------------------------------ el banco

/// Un Termux de mentira: un HOME, los dobles de las órdenes y unas rutas de
/// MAGI aisladas.
struct Banco {
    _raiz: tempfile::TempDir,
    /// El directorio de la prueba (`XDG_*`, `$PREFIX` y los dobles cuelgan de
    /// él).
    base: PathBuf,
    dobles: Dobles,
    home: PathBuf,
    rutas: Rutas,
    entorno: EntornoTermux,
}

impl Banco {
    fn nuevo() -> Self {
        como_termux();
        let raiz = tempfile::Builder::new()
            .prefix("magitk")
            .tempdir()
            .expect("directorio temporal");
        let base = raiz.path().to_path_buf();
        let dobles = Dobles::nuevos(&base.join("termux"));
        let home = base.join("home");
        let entorno = dobles.entorno(&home);
        // Las mismas rutas que sacaría `magi` de los `XDG_*` de
        // `entorno_del_binario`: las pruebas del binario real leen el mismo
        // REGISTRO.
        let rutas = Rutas {
            datos: base.join("xd").join("magi"),
            config: base.join("xc").join("magi"),
            estado: base.join("xe").join("magi"),
            hogar: home.clone(),
            runtime: base.join("r").join("magi"),
        };
        std::fs::create_dir_all(&rutas.datos).expect("datos");
        Self {
            _raiz: raiz,
            base,
            dobles,
            home,
            rutas,
            entorno,
        }
    }

    /// `~/.termux/termux.properties`.
    fn propiedades(&self) -> PathBuf {
        self.home.join(".termux").join("termux.properties")
    }

    /// `~/.config/termux/termux.properties`.
    fn propiedades_de_config(&self) -> PathBuf {
        self.home
            .join(".config")
            .join("termux")
            .join("termux.properties")
    }

    /// Escribe `~/.termux/termux.properties` con ese contenido.
    fn con_propiedades(&self, contenido: impl AsRef<[u8]>) -> PathBuf {
        let ruta = self.propiedades();
        std::fs::create_dir_all(ruta.parent().expect("padre")).expect("directorio");
        std::fs::write(&ruta, contenido).expect("escribir");
        ruta
    }

    fn ordenes(&self, plazo: Duration) -> (Ordenes, Arc<ApiProceso>) {
        let api = Arc::new(ApiProceso::default());
        (Ordenes::nuevas(&self.entorno, plazo, Arc::clone(&api)), api)
    }

    /// Los tipos de REGISTRO anotados y su detalle.
    fn registro(&self) -> Vec<(String, String)> {
        let almacen = magi::almacen::abrir(&self.rutas.base_datos()).expect("almacén");
        almacen
            .registro_para_exportar(None)
            .expect("registro")
            .into_iter()
            .map(|entrada| (entrada.tipo, entrada.detalle))
            .collect()
    }

    fn instalaciones(&self) -> Vec<String> {
        self.registro()
            .into_iter()
            .filter(|(tipo, _)| tipo == "termux_teclas_instaladas")
            .map(|(_, detalle)| detalle)
            .collect()
    }
}

// ------------------------------------------------------------------ la fila

#[test]
fn la_fila_propuesta_es_la_del_informe_y_sigue_al_prefijo() {
    como_termux();
    assert_eq!(teclas::fila(PREFIJO), FILA);
    let con_a = teclas::fila("Ctrl+a");
    assert!(
        con_a.contains("{macro: 'CTRL a', display: '^A'}"),
        "{con_a}"
    );
    assert_eq!(
        con_a.replace(
            "{macro: 'CTRL a', display: '^A'}",
            "{macro: 'CTRL ]', display: '^]'}"
        ),
        FILA
    );
    // Un prefijo que no se puede expresar deja la fila sin macro y lo dice.
    let sin_macro = teclas::fila("Ctrl+'");
    assert!(!sin_macro.contains("macro"), "{sin_macro}");
    assert!(teclas::macro_del_prefijo("Ctrl+'").is_err());
}

#[test]
fn el_prefijo_configurado_va_en_la_fila_instalada() {
    let banco = Banco::nuevo();
    let ruta = banco.propiedades();
    teclas::escribir_fila(&ruta, "Ctrl+a", fecha()).expect("instalar");
    let texto = std::fs::read_to_string(&ruta).unwrap();
    assert!(
        texto.contains("{macro: 'CTRL a', display: '^A'}"),
        "{texto}"
    );
    assert_eq!(
        teclas::estado_fila(&banco.entorno, "Ctrl+a"),
        EstadoFila::LaDeMagi
    );
    // Con el prefijo por defecto esa fila ya no es la de MAGI.
    assert_eq!(
        teclas::estado_fila(&banco.entorno, PREFIJO),
        EstadoFila::Otra
    );
}

// ------------------------------------------------------------------ la fusión (AC)

/// El AC del checklist: comentarios, otras claves y una `extra-keys` partida
/// en tres líneas con `\`; solo cambia esa línea lógica y el resto queda byte
/// a byte, con CRLF.
#[test]
fn solo_cambia_la_linea_logica_extra_keys_y_el_resto_queda_byte_a_byte() {
    let banco = Banco::nuevo();
    let antes = "# Ajustes de Termux\r\nbell-character = ignore\r\n\r\n# extra-keys = [['X']]\r\n   # otra = 1\r\nextra-keys = [['ESC','TAB'], \\\r\n    ['F1','F2'], \\\r\n    ['CTRL','ALT']]\r\nextra-keys-style = default\r\nfullscreen = false\r\nsin-salto = 1";
    let despues = format!("# Ajustes de Termux\r\nbell-character = ignore\r\n\r\n# extra-keys = [['X']]\r\n   # otra = 1\r\n{FILA}\r\nextra-keys-style = default\r\nfullscreen = false\r\nsin-salto = 1");
    let ruta = banco.con_propiedades(antes);
    let escritura = teclas::escribir_fila(&ruta, PREFIJO, fecha()).expect("instalar");
    assert!(!escritura.ya_estaba);
    assert_eq!(std::fs::read_to_string(&ruta).unwrap(), despues);
    // La copia es el original, tal cual.
    assert_eq!(
        std::fs::read_to_string(ruta.with_file_name(COPIA)).unwrap(),
        antes
    );
}

#[test]
fn con_lf_y_con_la_continuacion_en_la_ultima_linea_del_fichero() {
    let banco = Banco::nuevo();
    let ruta = banco.con_propiedades("a = 1\nextra-keys = [[\\\n  'A'\\\n]]\n");
    teclas::escribir_fila(&ruta, PREFIJO, fecha()).expect("instalar");
    assert_eq!(
        std::fs::read_to_string(&ruta).unwrap(),
        format!("a = 1\n{FILA}\n")
    );
}

#[test]
fn una_extra_keys_dentro_de_un_comentario_no_se_toca_y_la_fila_se_anade_al_final() {
    let banco = Banco::nuevo();
    let antes = "# extra-keys = [['X']]\n! extra-keys = [['Y']]\nfullscreen = true";
    let ruta = banco.con_propiedades(antes);
    assert_eq!(
        teclas::estado_fila(&banco.entorno, PREFIJO),
        EstadoFila::SinExtraKeys
    );
    teclas::escribir_fila(&ruta, PREFIJO, fecha()).expect("instalar");
    assert_eq!(
        std::fs::read_to_string(&ruta).unwrap(),
        format!("{antes}\n{COMENTARIO_FILA}\n{FILA}\n")
    );
}

#[test]
fn con_varias_extra_keys_manda_la_ultima() {
    let banco = Banco::nuevo();
    let antes = "extra-keys = [['A']]\nx = 1\nextra-keys = [['B']]\ny = 2\n";
    let ruta = banco.con_propiedades(antes);
    teclas::escribir_fila(&ruta, PREFIJO, fecha()).expect("instalar");
    assert_eq!(
        std::fs::read_to_string(&ruta).unwrap(),
        format!("extra-keys = [['A']]\nx = 1\n{FILA}\ny = 2\n")
    );
}

#[test]
fn una_clave_que_solo_empieza_por_extra_keys_no_es_la_fila() {
    let banco = Banco::nuevo();
    let antes = "extra-keys-style = default\n";
    let ruta = banco.con_propiedades(antes);
    teclas::escribir_fila(&ruta, PREFIJO, fecha()).expect("instalar");
    assert_eq!(
        std::fs::read_to_string(&ruta).unwrap(),
        format!("{antes}{COMENTARIO_FILA}\n{FILA}\n")
    );
}

// ------------------------------------------------------------------ la fusión según Java

/// Las líneas lógicas de `java.util.Properties.load` (el `LineReader` de
/// OpenJDK, sobre caracteres): saltos `\n`, `\r\n` y `\r`, comentarios solo al
/// empezar una línea lógica, continuación con `\` y fin del fichero. Es la
/// referencia contra la que se comprueba que MAGI no cambia el valor de una
/// línea que no es suya (Termux lee su `termux.properties` con ese lector).
fn java_lineas_logicas(texto: &str) -> Vec<String> {
    let caracteres: Vec<char> = texto.chars().collect();
    let mut lineas = Vec::new();
    let mut posicion = 0;
    let mut salta_lf = false;
    loop {
        // Una llamada a `readLine()`.
        let mut linea = String::new();
        let mut salta_blancos = true;
        let mut es_comentario = false;
        let mut es_nueva = true;
        let mut empieza_anadida = false;
        let mut barra_previa = false;
        loop {
            if posicion >= caracteres.len() {
                if linea.is_empty() || es_comentario {
                    return lineas;
                }
                if barra_previa {
                    linea.pop();
                }
                lineas.push(linea);
                return lineas;
            }
            let c = caracteres[posicion];
            posicion += 1;
            if salta_lf {
                salta_lf = false;
                if c == '\n' {
                    continue;
                }
            }
            if salta_blancos {
                if c == ' ' || c == '\t' || c == '\u{c}' {
                    continue;
                }
                if !empieza_anadida && (c == '\r' || c == '\n') {
                    continue;
                }
                salta_blancos = false;
                empieza_anadida = false;
            }
            if es_nueva {
                es_nueva = false;
                if c == '#' || c == '!' {
                    es_comentario = true;
                    continue;
                }
            }
            if c != '\n' && c != '\r' {
                linea.push(c);
                barra_previa = c == '\\' && !barra_previa;
                continue;
            }
            // Fin de línea física.
            if es_comentario || linea.is_empty() {
                es_comentario = false;
                es_nueva = true;
                salta_blancos = true;
                linea.clear();
                continue;
            }
            if posicion >= caracteres.len() {
                if barra_previa {
                    linea.pop();
                }
                lineas.push(linea);
                return lineas;
            }
            if barra_previa {
                linea.pop();
                salta_blancos = true;
                empieza_anadida = true;
                barra_previa = false;
                if c == '\r' {
                    salta_lf = true;
                }
            } else {
                lineas.push(linea);
                break;
            }
        }
    }
}

/// Clave y valor de una línea lógica, como `Properties.load0` (con los
/// escapes más simples de `loadConvert`).
fn java_clave_y_valor(logica: &str) -> (String, String) {
    let c: Vec<char> = logica.chars().collect();
    let es_blanco = |c: char| c == ' ' || c == '\t' || c == '\u{c}';
    let mut fin_clave = 0;
    let mut valor = c.len();
    let mut con_separador = false;
    let mut barra_previa = false;
    while fin_clave < c.len() {
        let actual = c[fin_clave];
        if (actual == '=' || actual == ':') && !barra_previa {
            valor = fin_clave + 1;
            con_separador = true;
            break;
        } else if es_blanco(actual) && !barra_previa {
            valor = fin_clave + 1;
            break;
        }
        barra_previa = actual == '\\' && !barra_previa;
        fin_clave += 1;
    }
    while valor < c.len() {
        let actual = c[valor];
        if !es_blanco(actual) {
            if !con_separador && (actual == '=' || actual == ':') {
                con_separador = true;
            } else {
                break;
            }
        }
        valor += 1;
    }
    let convertir = |trozo: &[char]| {
        let mut salida = String::new();
        let mut escapa = false;
        for &caracter in trozo {
            if escapa {
                salida.push(match caracter {
                    't' => '\t',
                    'n' => '\n',
                    'r' => '\r',
                    'f' => '\u{c}',
                    otro => otro,
                });
                escapa = false;
            } else if caracter == '\\' {
                escapa = true;
            } else {
                salida.push(caracter);
            }
        }
        salida
    };
    (convertir(&c[..fin_clave]), convertir(&c[valor..]))
}

/// Lo que ve Termux: clave → valor, con la última repetida mandando. Una
/// línea que queda vacía (una `\` sola al final del fichero) la lee Java como
/// la propiedad `"" = ""`, que no es nada: no se cuenta.
fn java_propiedades(texto: &str) -> std::collections::BTreeMap<String, String> {
    java_lineas_logicas(texto)
        .iter()
        .map(|logica| java_clave_y_valor(logica))
        .filter(|(clave, valor)| !(clave.is_empty() && valor.is_empty()))
        .collect()
}

/// El valor que Termux lee de la fila de MAGI.
fn valor_de_la_fila() -> String {
    java_propiedades(&format!("{FILA}\n"))
        .remove("extra-keys")
        .expect("la fila tiene su clave")
}

/// El lector de referencia entiende lo que se espera de él (si no, las
/// pruebas que lo usan no demostrarían nada).
#[test]
fn el_lector_de_referencia_sigue_a_java_properties() {
    let mapa = java_propiedades("a = 1\n# c\nb:2\n  c   3 \\\n   4\nd = x\\\n\n# e\n! f\ng\n");
    assert_eq!(mapa["a"], "1");
    assert_eq!(mapa["b"], "2");
    assert_eq!(mapa["c"], "3 4");
    // Una línea en blanco termina la continuación.
    assert_eq!(mapa["d"], "x");
    assert_eq!(mapa["g"], "");
    assert_eq!(mapa.len(), 5);
    // Tras una continuación, un `#` es del valor, no un comentario.
    let mapa = java_propiedades("bell = vibrate \\\n# fila de MAGI\nextra-keys = X\n");
    assert_eq!(mapa["bell"], "vibrate # fila de MAGI");
    assert_eq!(mapa["extra-keys"], "X");
}

/// K2: un `termux.properties` que acaba en una continuación (`bell-character =
/// vibrate \`) no puede tragarse el comentario de MAGI: en Java una línea tras
/// una continuación no es un comentario aunque empiece por `#`, y el valor de
/// la última clave del usuario cambiaría. Con LF, con CRLF y sin salto final.
#[test]
fn si_el_fichero_acaba_en_continuacion_el_valor_del_usuario_no_cambia() {
    for (nombre, antes) in [
        ("LF", "a = 1\nbell-character = vibrate \\\n"),
        ("CRLF", "a = 1\r\nbell-character = vibrate \\\r\n"),
        ("CR", "a = 1\rbell-character = vibrate \\\r"),
        ("sin salto", "a = 1\nbell-character = vibrate \\"),
    ] {
        let banco = Banco::nuevo();
        let ruta = banco.con_propiedades(antes);
        let escritura = teclas::escribir_fila(&ruta, PREFIJO, fecha()).expect("instalar");
        assert!(!escritura.ya_estaba, "{nombre}");
        let despues = std::fs::read_to_string(&ruta).unwrap();
        let antes_java = java_propiedades(antes);
        let despues_java = java_propiedades(&despues);
        assert_eq!(
            despues_java["bell-character"], antes_java["bell-character"],
            "{nombre}: MAGI cambió el valor de una línea que no es suya:\n{despues:?}"
        );
        assert_eq!(despues_java["a"], "1", "{nombre}");
        assert_eq!(despues_java["extra-keys"], valor_de_la_fila(), "{nombre}");
        // Del original solo se añade: lo de antes sigue byte a byte y la
        // fila queda con su comentario.
        assert!(despues.starts_with(antes), "{nombre}: {despues:?}");
        assert!(
            despues.contains(&format!("{COMENTARIO_FILA}\n{FILA}\n")),
            "{nombre}: {despues:?}"
        );
        // Y lo que se ha escrito se reconoce como la fila de MAGI.
        assert_eq!(
            teclas::estado_fila(&banco.entorno, PREFIJO),
            EstadoFila::LaDeMagi,
            "{nombre}"
        );
    }
}

/// Un generador congruencial para repetir los mismos ficheros en cada
/// ejecución (sin dependencias).
struct Azar(u64);

impl Azar {
    fn siguiente(&mut self, tope: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as usize) % tope
    }
}

/// Para cualquier mezcla de comentarios, continuaciones, líneas en blanco y
/// saltos de línea (incluidos los finales abiertos), lo que Termux lee tras
/// instalar la fila es lo mismo que antes salvo `extra-keys`, que pasa a ser la
/// de MAGI.
#[test]
fn instalar_la_fila_solo_cambia_extra_keys_para_el_lector_de_java() {
    const LINEAS: [&str; 17] = [
        "a = 1",
        "b: valor",
        "bell-character = vibrate \\",
        "clave = dos barras \\\\",
        "clave = tres \\\\\\",
        "   # comentario sangrado",
        "! otro comentario",
        "# comentario con barra \\",
        "",
        "   ",
        "extra-keys = [['A']]",
        "extra-keys = [[ \\",
        "    'B']]",
        "extra-keys-style = default",
        "c = d\\",
        "\\",
        "# extra-keys = [['X']]",
    ];
    const SALTOS: [&str; 4] = ["\n", "\r\n", "\r", ""];
    let mut azar = Azar(0x4d41_4749);
    let esperado = valor_de_la_fila();
    for caso in 0..4000 {
        let cuantas = azar.siguiente(9);
        let mut texto = String::new();
        for indice in 0..cuantas {
            texto.push_str(LINEAS[azar.siguiente(LINEAS.len())]);
            // El último puede no llevar salto; los demás sí.
            let ultimo = indice + 1 == cuantas;
            texto.push_str(if ultimo {
                SALTOS[azar.siguiente(SALTOS.len())]
            } else {
                SALTOS[azar.siguiente(3)]
            });
        }
        let nuevo = teclas::fusionar(&texto, &teclas::fila(PREFIJO));
        let mut previsto = java_propiedades(&texto);
        previsto.insert("extra-keys".to_string(), esperado.clone());
        assert_eq!(
            java_propiedades(&nuevo),
            previsto,
            "caso {caso}: MAGI cambió algo más que extra-keys\nantes:   {texto:?}\ndespués: {nuevo:?}"
        );
        // Y lo que MAGI lee de lo que acaba de escribir es la fila de MAGI.
        assert!(
            teclas::es_la_de_magi(&nuevo, PREFIJO),
            "caso {caso}: {nuevo:?}"
        );
    }
}

// ------------------------------------------------------------------ el fichero

#[test]
fn sin_fichero_se_crea_termux_properties_con_directorio_700_y_fichero_600() {
    let banco = Banco::nuevo();
    assert!(!banco.home.join(".termux").exists());
    let ruta = teclas::ruta_properties(&banco.entorno).expect("ruta");
    assert_eq!(ruta, banco.propiedades());
    assert_eq!(
        teclas::estado_fila(&banco.entorno, PREFIJO),
        EstadoFila::SinFichero
    );
    let escritura = teclas::escribir_fila(&ruta, PREFIJO, fecha()).expect("instalar");
    assert_eq!(escritura.copia, None);
    assert_eq!(escritura.detalle_de_registro(), "no existía");
    assert_eq!(
        std::fs::read_to_string(&ruta).unwrap(),
        format!("{COMENTARIO_FILA}\n{FILA}\n")
    );
    assert_eq!(modo(&ruta), 0o600);
    assert_eq!(modo(&banco.home.join(".termux")), 0o700);
    assert_eq!(entradas(&banco.home.join(".termux")), ["termux.properties"]);
    assert_eq!(
        teclas::estado_fila(&banco.entorno, PREFIJO),
        EstadoFila::LaDeMagi
    );
}

#[test]
fn si_el_que_existe_es_el_de_config_se_usa_ese() {
    let banco = Banco::nuevo();
    let secundaria = banco.propiedades_de_config();
    std::fs::create_dir_all(secundaria.parent().unwrap()).unwrap();
    std::fs::write(&secundaria, "a = 1\n").unwrap();
    let ruta = teclas::ruta_properties(&banco.entorno).expect("ruta");
    assert_eq!(ruta, secundaria);
    teclas::escribir_fila(&ruta, PREFIJO, fecha()).expect("instalar");
    assert_eq!(
        std::fs::read_to_string(&secundaria).unwrap(),
        format!("a = 1\n{COMENTARIO_FILA}\n{FILA}\n")
    );
    // La copia queda junto a él y no se crea `~/.termux/`.
    assert!(secundaria.with_file_name(COPIA).is_file());
    assert!(!banco.home.join(".termux").exists());
    // Con los dos, manda el de ~/.termux (el que lee Termux).
    let principal = banco.con_propiedades("b = 2\n");
    assert_eq!(teclas::ruta_properties(&banco.entorno), Some(principal));
}

#[test]
fn si_ya_es_la_de_magi_no_hay_copia_ni_escritura() {
    let banco = Banco::nuevo();
    // Con otro espaciado y el resto del usuario: ni un byte cambia.
    let texto = format!("# mío\r\nx = 1\r\n{}\r\n", FILA.replace(", ", ",  "));
    let ruta = banco.con_propiedades(&texto);
    let modificado = std::fs::metadata(&ruta).unwrap().modified().unwrap();
    let escritura = teclas::escribir_fila(&ruta, PREFIJO, fecha()).expect("instalar");
    assert!(escritura.ya_estaba);
    assert_eq!(escritura.copia, None);
    assert_eq!(std::fs::read_to_string(&ruta).unwrap(), texto);
    assert_eq!(
        std::fs::metadata(&ruta).unwrap().modified().unwrap(),
        modificado
    );
    assert_eq!(entradas(&banco.home.join(".termux")), ["termux.properties"]);
}

#[test]
fn la_copia_lleva_la_fecha_y_sufijos_y_conserva_los_permisos() {
    let banco = Banco::nuevo();
    let ruta = banco.con_propiedades("a = 1\n");
    std::fs::set_permissions(&ruta, std::fs::Permissions::from_mode(0o640)).unwrap();
    let carpeta = banco.home.join(".termux");

    let primera = teclas::escribir_fila(&ruta, PREFIJO, fecha()).expect("primera");
    assert_eq!(primera.copia.as_deref(), Some(COPIA));
    assert_eq!(primera.detalle_de_registro(), COPIA);
    assert_eq!(
        std::fs::read_to_string(carpeta.join(COPIA)).unwrap(),
        "a = 1\n"
    );
    assert_eq!(modo(&carpeta.join(COPIA)), 0o640, "permisos del original");
    assert_eq!(modo(&ruta), 0o640, "la escritura conserva los permisos");

    // Otro cambio el mismo día: la copia nueva lleva sufijo y no pisa la anterior.
    std::fs::write(&ruta, "b = 2\n").unwrap();
    let segunda = teclas::escribir_fila(&ruta, PREFIJO, fecha()).expect("segunda");
    assert_eq!(segunda.copia.as_deref(), Some(&*format!("{COPIA}-2")));
    std::fs::write(&ruta, "c = 3\n").unwrap();
    let tercera = teclas::escribir_fila(&ruta, PREFIJO, fecha()).expect("tercera");
    assert_eq!(tercera.copia.as_deref(), Some(&*format!("{COPIA}-3")));
    assert_eq!(
        std::fs::read_to_string(carpeta.join(COPIA)).unwrap(),
        "a = 1\n"
    );
    assert_eq!(
        std::fs::read_to_string(carpeta.join(format!("{COPIA}-2"))).unwrap(),
        "b = 2\n"
    );
    // Otro día, otra copia sin sufijo.
    std::fs::write(&ruta, "d = 4\n").unwrap();
    let otro_dia = NaiveDate::from_ymd_opt(2026, 11, 18).unwrap();
    let cuarta = teclas::escribir_fila(&ruta, PREFIJO, otro_dia).expect("otro día");
    assert_eq!(
        cuarta.copia.as_deref(),
        Some("termux.properties.magi-2026-11-18")
    );
}

#[test]
fn si_la_copia_falla_no_se_escribe_nada() {
    let banco = Banco::nuevo();
    let ruta = banco.con_propiedades("a = 1\n");
    let carpeta = banco.home.join(".termux");
    let Some(_guarda) = SoloLectura::de(&carpeta) else {
        return;
    };
    let error = teclas::escribir_fila(&ruta, PREFIJO, fecha()).unwrap_err();
    assert!(matches!(error, ErrorTeclas::Copia(_)), "{error}");
    assert!(error.to_string().contains("no se ha escrito nada"));
    assert_eq!(std::fs::read_to_string(&ruta).unwrap(), "a = 1\n");
    assert_eq!(entradas(&carpeta), ["termux.properties"]);
}

#[test]
fn un_fichero_que_no_se_puede_leer_no_se_toca() {
    let banco = Banco::nuevo();
    let carpeta = banco.home.join(".termux");
    // Más de 256 KiB.
    let grande = vec![b'#'; teclas::TOPE_PROPERTIES + 1];
    let ruta = banco.con_propiedades(&grande);
    let error = teclas::escribir_fila(&ruta, PREFIJO, fecha()).unwrap_err();
    assert!(matches!(error, ErrorTeclas::Ilegible(_)), "{error}");
    assert!(error.to_string().contains("edítalo a mano"), "{error}");
    assert_eq!(std::fs::read(&ruta).unwrap(), grande);
    assert_eq!(entradas(&carpeta), ["termux.properties"]);
    assert!(matches!(
        teclas::estado_fila(&banco.entorno, PREFIJO),
        EstadoFila::Ilegible(_)
    ));
    // No UTF-8.
    let binario = [b'a', b'=', 0xff, 0xfe, b'\n'];
    std::fs::write(&ruta, binario).unwrap();
    let error = teclas::escribir_fila(&ruta, PREFIJO, fecha()).unwrap_err();
    assert!(matches!(error, ErrorTeclas::Ilegible(_)), "{error}");
    assert_eq!(std::fs::read(&ruta).unwrap(), binario);
    assert_eq!(entradas(&carpeta), ["termux.properties"]);
    // Justo en el tope, sí se lee y se instala.
    let justo = format!("{}\n", "#".repeat(teclas::TOPE_PROPERTIES - 1));
    assert_eq!(justo.len(), teclas::TOPE_PROPERTIES);
    std::fs::write(&ruta, &justo).unwrap();
    teclas::escribir_fila(&ruta, PREFIJO, fecha()).expect("en el tope");
}

#[test]
fn un_enlace_no_se_sustituye_por_un_fichero_normal() {
    let banco = Banco::nuevo();
    let destino = banco.home.join("dotfiles.properties");
    std::fs::create_dir_all(&banco.home).unwrap();
    std::fs::write(&destino, "a = 1\n").unwrap();
    let carpeta = banco.home.join(".termux");
    std::fs::create_dir_all(&carpeta).unwrap();
    let ruta = carpeta.join("termux.properties");
    std::os::unix::fs::symlink(&destino, &ruta).unwrap();
    teclas::escribir_fila(&ruta, PREFIJO, fecha()).expect("instalar");
    assert!(std::fs::symlink_metadata(&ruta)
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(std::fs::read_to_string(&destino).unwrap().contains(FILA));
}

// ------------------------------------------------------------------ termux-reload-settings

#[test]
fn termux_reload_settings_se_llama_con_el_plazo_y_sin_argumentos() {
    let banco = Banco::nuevo();
    let (ordenes, api) = banco.ordenes(Duration::from_secs(5));
    teclas::recargar(&ordenes).expect("recargar");
    let llamadas = banco.dobles.llamadas(ORDEN_RECARGAR);
    assert_eq!(llamadas.len(), 1);
    assert!(llamadas[0].args.is_empty(), "{:?}", llamadas[0].args);
    // No es de Termux:API: no la toca.
    assert_eq!(api.estado(), EstadoApi::SinComprobar);
}

#[test]
fn si_la_recarga_falla_o_no_responde_el_resultado_dice_que_hay_que_reiniciar() {
    let banco = Banco::nuevo();
    // Falla con un código.
    banco.dobles.programar(ORDEN_RECARGAR, Modo::Fallar);
    let (ordenes, api) = banco.ordenes(Duration::from_secs(5));
    let fallo = teclas::recargar(&ordenes).unwrap_err();
    assert_eq!(teclas::texto_de_recarga(&Err(fallo)), REINICIA_TERMUX);
    // No responde: el plazo la corta y no se espera de más.
    banco.dobles.programar(ORDEN_RECARGAR, Modo::Colgar);
    let (ordenes, _) = banco.ordenes(Duration::from_millis(500));
    let inicio = Instant::now();
    let fallo = teclas::recargar(&ordenes).unwrap_err();
    assert!(
        inicio.elapsed() < Duration::from_secs(10),
        "esperó {:?}",
        inicio.elapsed()
    );
    assert_eq!(teclas::texto_de_recarga(&Err(fallo)), REINICIA_TERMUX);
    // Ni falta de la orden ni plazos vencidos apagan Termux:API.
    assert_eq!(api.estado(), EstadoApi::SinComprobar);
    banco.dobles.quitar(ORDEN_RECARGAR);
    let (ordenes, api) = banco.ordenes(Duration::from_secs(5));
    let fallo = teclas::recargar(&ordenes).unwrap_err();
    assert_eq!(
        teclas::motivo_de_recarga(&fallo),
        "falta termux-reload-settings"
    );
    assert_eq!(api.estado(), EstadoApi::SinComprobar);
}

// ------------------------------------------------------------------ la CLI

struct ConsolaFalsa {
    interactiva: bool,
    respuestas: VecDeque<Result<String, ErrorEntrada>>,
    dicho: Vec<String>,
    avisado: Vec<String>,
    preguntas: Vec<String>,
}

impl ConsolaFalsa {
    fn con(interactiva: bool, respuestas: Vec<Result<String, ErrorEntrada>>) -> Self {
        Self {
            interactiva,
            respuestas: respuestas.into(),
            dicho: Vec::new(),
            avisado: Vec::new(),
            preguntas: Vec::new(),
        }
    }

    fn responde(respuesta: &str) -> Self {
        Self::con(true, vec![Ok(format!("{respuesta}\n"))])
    }

    fn todo(&self) -> String {
        format!("{}\n{}", self.dicho.join("\n"), self.avisado.join("\n"))
    }
}

impl Consola for ConsolaFalsa {
    fn es_interactiva(&self) -> bool {
        self.interactiva
    }

    fn leer_oculto(&mut self, _pregunta: &str) -> Result<Zeroizing<String>, ErrorEntrada> {
        panic!("la fila de teclas no pide nada oculto");
    }

    fn leer_linea(&mut self, pregunta: &str) -> Result<String, ErrorEntrada> {
        self.preguntas.push(pregunta.to_string());
        self.respuestas
            .pop_front()
            .unwrap_or(Err(ErrorEntrada::Cancelada))
    }

    fn decir(&mut self, texto: &str) {
        self.dicho.push(texto.to_string());
    }

    fn avisar(&mut self, texto: &str) {
        self.avisado.push(texto.to_string());
    }
}

fn cli(banco: &Banco, instalar: bool, consola: &mut ConsolaFalsa) -> i32 {
    let (ordenes, _) = banco.ordenes(Duration::from_secs(5));
    orden_teclas(
        instalar,
        &banco.entorno,
        PREFIJO,
        &ordenes,
        &banco.rutas,
        consola,
        fecha(),
    )
}

#[test]
fn sin_instalar_enseña_la_fila_propuesta_y_la_actual_sin_escribir() {
    let banco = Banco::nuevo();
    let antes = "a = 1\nextra-keys = [['A','B']]\n";
    let ruta = banco.con_propiedades(antes);
    let mut consola = ConsolaFalsa::con(false, Vec::new());
    assert_eq!(cli(&banco, false, &mut consola), 0);
    let salida = consola.todo();
    for esperado in [
        FILA,
        "~/.termux/termux.properties",
        "extra-keys = [['A','B']]",
        "magi termux teclas --instalar",
    ] {
        assert!(salida.contains(esperado), "falta «{esperado}»:\n{salida}");
    }
    assert!(consola.preguntas.is_empty(), "sin --instalar no pregunta");
    assert_eq!(std::fs::read_to_string(&ruta).unwrap(), antes);
    assert_eq!(entradas(&banco.home.join(".termux")), ["termux.properties"]);
    assert!(banco.instalaciones().is_empty());
    assert!(banco.dobles.llamadas(ORDEN_RECARGAR).is_empty());

    // Sin fichero y sin línea: lo dice.
    let banco = Banco::nuevo();
    let mut consola = ConsolaFalsa::con(false, Vec::new());
    assert_eq!(cli(&banco, false, &mut consola), 0);
    assert!(consola.todo().contains("el fichero no existe"));
    assert!(!banco.home.join(".termux").exists());
    let banco = Banco::nuevo();
    banco.con_propiedades("a = 1\n");
    let mut consola = ConsolaFalsa::con(false, Vec::new());
    assert_eq!(cli(&banco, false, &mut consola), 0);
    assert!(consola.todo().contains("no hay línea extra-keys"));
}

#[test]
fn con_instalar_pregunta_y_con_s_copia_escribe_anota_y_recarga() {
    let banco = Banco::nuevo();
    let ruta = banco.con_propiedades("a = 1\nextra-keys = [['A']]\nz = 9\n");
    let mut consola = ConsolaFalsa::responde("s");
    assert_eq!(cli(&banco, true, &mut consola), 0, "{}", consola.todo());
    assert_eq!(consola.preguntas, ["¿instalar? [s/N] "]);
    assert_eq!(
        std::fs::read_to_string(&ruta).unwrap(),
        format!("a = 1\n{FILA}\nz = 9\n")
    );
    let copia = banco.home.join(".termux").join(COPIA);
    assert_eq!(
        std::fs::read_to_string(&copia).unwrap(),
        "a = 1\nextra-keys = [['A']]\nz = 9\n"
    );
    // REGISTRO con el efecto ya producido y el nombre de la copia.
    assert_eq!(banco.instalaciones(), [COPIA]);
    // Recarga una vez y lo dice.
    assert_eq!(banco.dobles.llamadas(ORDEN_RECARGAR).len(), 1);
    let salida = consola.todo();
    assert!(salida.contains(COPIA), "{salida}");
    assert!(salida.contains("ya se ve"), "{salida}");
}

#[test]
fn con_instalar_sin_fichero_anota_no_existia() {
    let banco = Banco::nuevo();
    let mut consola = ConsolaFalsa::responde("sí");
    assert_eq!(cli(&banco, true, &mut consola), 0, "{}", consola.todo());
    assert_eq!(banco.instalaciones(), ["no existía"]);
    assert!(banco.propiedades().is_file());
    assert!(consola.todo().contains("no hay copia"));
}

#[test]
fn cualquier_respuesta_que_no_sea_si_no_instala_nada() {
    for respuesta in ["n", "N", "", "no", "ss", "claro", "\n"] {
        let banco = Banco::nuevo();
        let ruta = banco.con_propiedades("a = 1\n");
        let mut consola = ConsolaFalsa::responde(respuesta.trim_end_matches('\n'));
        assert_eq!(cli(&banco, true, &mut consola), 1, "«{respuesta}»");
        assert_eq!(std::fs::read_to_string(&ruta).unwrap(), "a = 1\n");
        assert_eq!(entradas(&banco.home.join(".termux")), ["termux.properties"]);
        assert!(banco.instalaciones().is_empty());
        assert!(banco.dobles.llamadas(ORDEN_RECARGAR).is_empty());
    }
    // Ctrl+D o el fin de la entrada: tampoco.
    let banco = Banco::nuevo();
    let mut consola = ConsolaFalsa::con(true, vec![Err(ErrorEntrada::Cancelada)]);
    assert_eq!(cli(&banco, true, &mut consola), 1);
    assert!(!banco.home.join(".termux").exists());
}

#[test]
fn sin_terminal_da_error_con_instruccion_y_no_escribe() {
    let banco = Banco::nuevo();
    let ruta = banco.con_propiedades("a = 1\n");
    let mut consola = ConsolaFalsa::con(false, vec![Ok("s\n".to_string())]);
    assert_eq!(cli(&banco, true, &mut consola), 1);
    let avisos = consola.avisado.join("\n");
    assert!(avisos.contains("hace falta un terminal"), "{avisos}");
    assert!(avisos.contains("sesión de Termux"), "{avisos}");
    assert!(consola.preguntas.is_empty());
    assert_eq!(std::fs::read_to_string(&ruta).unwrap(), "a = 1\n");
    assert_eq!(entradas(&banco.home.join(".termux")), ["termux.properties"]);
    assert!(banco.instalaciones().is_empty());
}

#[test]
fn si_ya_es_la_de_magi_dice_ya_esta_sin_preguntar_ni_copiar() {
    let banco = Banco::nuevo();
    let texto = format!("{FILA}\n");
    let ruta = banco.con_propiedades(&texto);
    // Sin terminal también: no hay nada que preguntar ni escribir.
    let mut consola = ConsolaFalsa::con(false, Vec::new());
    assert_eq!(cli(&banco, true, &mut consola), 0);
    assert!(consola.todo().contains("Ya está"), "{}", consola.todo());
    assert!(consola.preguntas.is_empty());
    assert_eq!(std::fs::read_to_string(&ruta).unwrap(), texto);
    assert_eq!(entradas(&banco.home.join(".termux")), ["termux.properties"]);
    assert!(banco.instalaciones().is_empty());
    assert!(banco.dobles.llamadas(ORDEN_RECARGAR).is_empty());
}

#[test]
fn un_fichero_ilegible_sale_con_1_y_dice_que_se_edite_a_mano() {
    let banco = Banco::nuevo();
    let ruta = banco.con_propiedades([0xff_u8, 0xfe]);
    for instalar in [false, true] {
        let mut consola = ConsolaFalsa::responde("s");
        assert_eq!(cli(&banco, instalar, &mut consola), 1);
        let avisos = consola.avisado.join("\n");
        assert!(avisos.contains("edítalo a mano"), "{avisos}");
        assert!(consola.preguntas.is_empty());
    }
    assert_eq!(std::fs::read(&ruta).unwrap(), [0xff, 0xfe]);
    assert_eq!(entradas(&banco.home.join(".termux")), ["termux.properties"]);
    assert!(banco.dobles.llamadas(ORDEN_RECARGAR).is_empty());
}

#[test]
fn si_la_copia_falla_la_cli_sale_con_1_sin_escribir_ni_anotar() {
    let banco = Banco::nuevo();
    let ruta = banco.con_propiedades("a = 1\n");
    let Some(_guarda) = SoloLectura::de(&banco.home.join(".termux")) else {
        return;
    };
    let mut consola = ConsolaFalsa::responde("s");
    assert_eq!(cli(&banco, true, &mut consola), 1);
    assert!(consola.todo().contains("no se ha escrito nada"));
    assert_eq!(std::fs::read_to_string(&ruta).unwrap(), "a = 1\n");
    assert!(
        banco.instalaciones().is_empty(),
        "REGISTRO solo si hay efecto"
    );
    assert!(banco.dobles.llamadas(ORDEN_RECARGAR).is_empty());
}

/// El `termux.properties` de `banco` pasa a ser un enlace a unos dotfiles: la
/// copia cabe junto al enlace (en `~/.termux/`) y la escritura va al destino,
/// que se puede dejar sin permiso para que falle justo después de la copia.
/// Devuelve el directorio de los dotfiles.
fn con_el_destino_en_otro_directorio(banco: &Banco, contenido: &str) -> PathBuf {
    let dotfiles = banco.home.join("dotfiles");
    std::fs::create_dir_all(&dotfiles).expect("dotfiles");
    std::fs::write(dotfiles.join("termux.properties"), contenido).expect("fichero");
    let carpeta = banco.home.join(".termux");
    std::fs::create_dir_all(&carpeta).expect("~/.termux");
    std::os::unix::fs::symlink(
        dotfiles.join("termux.properties"),
        carpeta.join("termux.properties"),
    )
    .expect("enlace");
    dotfiles
}

/// K4: si la copia ya se hizo y la escritura falla, el mensaje de la CLI dice
/// que la copia ya está hecha (y cuál es), no solo que no se pudo escribir.
#[test]
fn si_la_escritura_falla_tras_la_copia_la_cli_dice_cual_es_la_copia() {
    let banco = Banco::nuevo();
    let dotfiles = con_el_destino_en_otro_directorio(&banco, "a = 1\n");
    let Some(_guarda) = SoloLectura::de(&dotfiles) else {
        return;
    };
    let mut consola = ConsolaFalsa::responde("s");
    assert_eq!(cli(&banco, true, &mut consola), 1);
    let avisos = consola.avisado.join("\n");
    assert!(
        avisos.contains("no se pudo escribir termux.properties (permiso denegado)")
            && avisos.contains(&format!("la copia {COPIA} ya está hecha")),
        "{avisos}"
    );
    assert!(banco.home.join(".termux").join(COPIA).is_file());
    assert!(banco.instalaciones().is_empty(), "REGISTRO solo con efecto");
    assert!(banco.dobles.llamadas(ORDEN_RECARGAR).is_empty());
}

#[test]
fn si_termux_no_recarga_la_cli_dice_que_reinicie_y_el_fichero_ya_esta_escrito() {
    let banco = Banco::nuevo();
    banco.dobles.programar(ORDEN_RECARGAR, Modo::Fallar);
    let mut consola = ConsolaFalsa::responde("s");
    assert_eq!(cli(&banco, true, &mut consola), 0, "{}", consola.todo());
    assert!(
        consola.avisado.join("\n").contains(REINICIA_TERMUX),
        "{}",
        consola.todo()
    );
    assert!(std::fs::read_to_string(banco.propiedades())
        .unwrap()
        .contains(FILA));
    assert_eq!(banco.instalaciones(), ["no existía"]);
}

// ------------------------------------------------------------------ el binario real

impl Banco {
    /// El entorno aislado del binario `magi`: nada del HOME ni de los `XDG_*`
    /// reales, los dobles de Termux al principio del `PATH` y, con `termux`,
    /// `MAGI_PLATAFORMA=termux`.
    fn entorno_del_binario(&self, termux: bool) -> Vec<(&'static str, OsString)> {
        let mut path = OsString::from(self.dobles.bin());
        path.push(":");
        path.push(std::env::var_os("PATH").unwrap_or_default());
        let mut entorno: Vec<(&'static str, OsString)> = vec![
            ("PATH", path),
            ("HOME", self.home.clone().into()),
            ("XDG_DATA_HOME", self.base.join("xd").into()),
            ("XDG_CONFIG_HOME", self.base.join("xc").into()),
            ("XDG_STATE_HOME", self.base.join("xe").into()),
            ("XDG_RUNTIME_DIR", self.base.join("r").into()),
            ("PREFIX", self.dobles.prefix().into()),
            ("TMPDIR", self.dobles.prefix().join("tmp").into()),
            ("TZ", "UTC".into()),
            ("LC_ALL", "C.UTF-8".into()),
        ];
        if termux {
            entorno.push(("MAGI_PLATAFORMA", "termux".into()));
        }
        entorno
    }

    /// Ejecuta `magi` sin terminal (entrada nula, salidas capturadas).
    fn ejecutar_binario(&self, termux: bool, argumentos: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_magi"))
            .args(argumentos)
            .env_clear()
            .envs(self.entorno_del_binario(termux))
            .stdin(Stdio::null())
            .output()
            .expect("ejecutar magi")
    }
}

fn texto_de(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn el_binario_enseña_la_fila_sin_escribir_y_sin_terminal_no_instala() {
    let banco = Banco::nuevo();
    let salida = banco.ejecutar_binario(true, &["termux", "teclas"]);
    assert_eq!(
        salida.status.code(),
        Some(0),
        "{}",
        texto_de(&salida.stderr)
    );
    let dicho = texto_de(&salida.stdout);
    for esperado in [
        FILA,
        "Fichero: ~/.termux/termux.properties",
        "el fichero no existe",
    ] {
        assert!(dicho.contains(esperado), "falta «{esperado}»:\n{dicho}");
    }
    assert!(
        !banco.home.join(".termux").exists(),
        "sin --instalar no se escribe"
    );

    // Con --instalar y sin terminal: error con instrucción y nada escrito.
    let salida = banco.ejecutar_binario(true, &["termux", "teclas", "--instalar"]);
    assert_eq!(salida.status.code(), Some(1));
    let avisos = texto_de(&salida.stderr);
    assert!(avisos.contains("hace falta un terminal"), "{avisos}");
    assert!(!banco.home.join(".termux").exists());
    assert!(banco.instalaciones().is_empty());
    assert!(banco.dobles.llamadas(ORDEN_RECARGAR).is_empty());
}

#[test]
fn el_binario_con_la_fila_de_magi_ya_puesta_dice_ya_esta_sin_terminal() {
    let banco = Banco::nuevo();
    let texto = format!("# mío\n{FILA}\n");
    let ruta = banco.con_propiedades(&texto);
    let salida = banco.ejecutar_binario(true, &["termux", "teclas", "--instalar"]);
    assert_eq!(
        salida.status.code(),
        Some(0),
        "{}",
        texto_de(&salida.stderr)
    );
    assert!(texto_de(&salida.stdout).contains("Ya está"));
    assert_eq!(std::fs::read_to_string(&ruta).unwrap(), texto);
    assert_eq!(entradas(&banco.home.join(".termux")), ["termux.properties"]);
    assert!(banco.dobles.llamadas(ORDEN_RECARGAR).is_empty());
}

#[test]
fn fuera_de_termux_la_cli_dice_solo_en_termux_y_sale_con_1() {
    let banco = Banco::nuevo();
    for argumentos in [
        &["termux", "teclas"][..],
        &["termux", "teclas", "--instalar"],
    ] {
        let salida = banco.ejecutar_binario(false, argumentos);
        assert_eq!(salida.status.code(), Some(1), "{argumentos:?}");
        assert!(
            texto_de(&salida.stderr).contains("solo en Termux"),
            "{}",
            texto_de(&salida.stderr)
        );
    }
    assert!(!banco.home.join(".termux").exists());
}

/// ¿Está el `script` de util-linux, que da un pseudoterminal al binario? Si
/// falta, con `CI` puesta es un fallo y sin ella la prueba se omite con un
/// aviso (solo se ve con `--nocapture`).
fn hay_script(prueba: &str) -> bool {
    let util_linux = Command::new("script")
        .arg("--version")
        .output()
        .ok()
        .is_some_and(|salida| String::from_utf8_lossy(&salida.stdout).contains("util-linux"));
    if util_linux {
        return true;
    }
    assert!(
        std::env::var_os("CI").is_none(),
        "termux_teclas::{prueba}: en CI hace falta el `script` de util-linux"
    );
    eprintln!("termux_teclas::{prueba}: se omite, falta `script` de util-linux");
    false
}

/// Un programa corriendo dentro de un pseudoterminal creado por `script`.
struct SesionPty {
    hijo: Child,
    entrada: Option<ChildStdin>,
    salida: Arc<Mutex<Vec<u8>>>,
    lector: Option<std::thread::JoinHandle<()>>,
}

impl SesionPty {
    /// Lanza `magi` con esos argumentos dentro de un pseudoterminal de 24×80,
    /// con el entorno del banco.
    fn magi(banco: &Banco, argumentos: &[&str]) -> Self {
        let mut orden = format!("stty rows 24 cols 80; '{}'", env!("CARGO_BIN_EXE_magi"));
        for argumento in argumentos {
            orden.push_str(&format!(" '{argumento}'"));
        }
        let mut hijo = Command::new("script")
            .env_clear()
            .envs(banco.entorno_del_binario(true))
            .args(["-qefc", &orden, "/dev/null"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("lanzar script");
        let entrada = hijo.stdin.take();
        let mut lectura = hijo.stdout.take().expect("salida de script");
        let salida = Arc::new(Mutex::new(Vec::new()));
        let copia = salida.clone();
        let lector = std::thread::spawn(move || {
            let mut bloque = [0u8; 4096];
            while let Ok(leidos) = lectura.read(&mut bloque) {
                if leidos == 0 {
                    break;
                }
                copia
                    .lock()
                    .expect("salida del pty")
                    .extend_from_slice(&bloque[..leidos]);
            }
        });
        Self {
            hijo,
            entrada,
            salida,
            lector: Some(lector),
        }
    }

    fn visto(&self) -> String {
        texto_de(&self.salida.lock().expect("salida del pty"))
    }

    /// Espera a que lo escrito en el terminal contenga `fragmento`.
    fn esperar(&self, fragmento: &str, plazo: Duration) {
        let limite = Instant::now() + plazo;
        while !self.visto().contains(fragmento) {
            assert!(
                Instant::now() < limite,
                "no apareció «{fragmento}» en el terminal; se vio: {:?}",
                self.visto()
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// Escribe lo que escribiría una persona (con el terminal en modo normal,
    /// Intro es `\n`).
    fn escribir(&mut self, bytes: &[u8]) {
        let entrada = self.entrada.as_mut().expect("entrada abierta");
        entrada.write_all(bytes).expect("escribir en el terminal");
        entrada.flush().expect("vaciar la entrada");
    }

    /// Espera a que acabe y devuelve su código (`None` si pasa el plazo).
    fn esperar_salida(&mut self, plazo: Duration) -> Option<i32> {
        let limite = Instant::now() + plazo;
        while Instant::now() < limite {
            if let Some(estado) = self.hijo.try_wait().expect("esperar a script") {
                if let Some(lector) = self.lector.take() {
                    let _ = lector.join();
                }
                return estado.code();
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        None
    }
}

impl Drop for SesionPty {
    fn drop(&mut self) {
        let _ = self.hijo.kill();
        let _ = self.hijo.wait();
    }
}

const PLAZO_TERMINAL: Duration = Duration::from_secs(30);

#[test]
fn con_un_terminal_de_verdad_s_instala_con_copia_registro_y_recarga() {
    if !hay_script("con_un_terminal_de_verdad_s_instala") {
        return;
    }
    let banco = Banco::nuevo();
    let antes = "a = 1\nextra-keys = [['A']]\n";
    let ruta = banco.con_propiedades(antes);
    let mut sesion = SesionPty::magi(&banco, &["termux", "teclas", "--instalar"]);
    sesion.esperar("¿instalar? [s/N]", PLAZO_TERMINAL);
    // Antes de preguntar enseñó lo que va a tocar.
    let visto = sesion.visto();
    for esperado in ["~/.termux/termux.properties", "una copia", "magi-"] {
        assert!(visto.contains(esperado), "falta «{esperado}»: {visto:?}");
    }
    assert_eq!(
        std::fs::read_to_string(&ruta).unwrap(),
        antes,
        "aún no se ha escrito"
    );
    sesion.escribir(b"s\n");
    let codigo = sesion.esperar_salida(PLAZO_TERMINAL);
    let visto = sesion.visto();
    assert_eq!(codigo, Some(0), "{visto:?}");
    assert_eq!(
        std::fs::read_to_string(&ruta).unwrap(),
        format!("a = 1\n{FILA}\n")
    );
    let instalaciones = banco.instalaciones();
    assert_eq!(instalaciones.len(), 1, "{:?}", banco.registro());
    let copia = banco.home.join(".termux").join(&instalaciones[0]);
    assert!(instalaciones[0].starts_with("termux.properties.magi-"));
    assert_eq!(std::fs::read_to_string(copia).unwrap(), antes);
    assert_eq!(banco.dobles.llamadas(ORDEN_RECARGAR).len(), 1);
    assert!(visto.contains("Fila de teclas instalada"), "{visto:?}");
}

#[test]
fn con_un_terminal_de_verdad_n_no_instala_nada() {
    if !hay_script("con_un_terminal_de_verdad_n_no_instala") {
        return;
    }
    let banco = Banco::nuevo();
    let ruta = banco.con_propiedades("a = 1\n");
    let mut sesion = SesionPty::magi(&banco, &["termux", "teclas", "--instalar"]);
    sesion.esperar("¿instalar? [s/N]", PLAZO_TERMINAL);
    sesion.escribir(b"\n");
    let codigo = sesion.esperar_salida(PLAZO_TERMINAL);
    assert_eq!(codigo, Some(1), "{:?}", sesion.visto());
    assert!(sesion.visto().contains("Cancelado"));
    assert_eq!(std::fs::read_to_string(&ruta).unwrap(), "a = 1\n");
    assert_eq!(entradas(&banco.home.join(".termux")), ["termux.properties"]);
    assert!(banco.instalaciones().is_empty());
    assert!(banco.dobles.llamadas(ORDEN_RECARGAR).is_empty());
}

// ------------------------------------------------------------------ el diálogo

/// Una App de Termux sobre los dobles, con la `Bienvenida` del servidor ya
/// recibida.
struct Prueba {
    app: App,
    terminal: Terminal<TestBackend>,
    banco: Banco,
    api: Arc<ApiProceso>,
}

impl Prueba {
    fn nueva() -> Self {
        Self::con_tamano(100, 70)
    }

    fn con_tamano(cols: u16, filas: u16) -> Self {
        Self::con_banco(Banco::nuevo(), cols, filas)
    }

    fn con_banco(banco: Banco, cols: u16, filas: u16) -> Self {
        {
            let almacen = magi::almacen::abrir(&banco.rutas.base_datos()).expect("almacén");
            almacen
                .crear_host(
                    &DatosHost {
                        nombre: "web-01".to_string(),
                        direccion: "192.0.2.11".to_string(),
                        ..DatosHost::default()
                    },
                    Origen::Manual,
                )
                .expect("host de prueba");
        }
        let almacen = magi::almacen::abrir(&banco.rutas.base_datos()).expect("almacén");
        let (mut app, _enviados) = App::de_prueba(
            banco.rutas.clone(),
            Config::default(),
            Tema::respaldo(),
            almacen,
        )
        .expect("app");
        assert!(app.termux.activo, "la App debería portarse como Termux");
        let api = Arc::new(ApiProceso::default());
        app.termux.ordenes = Arc::new(Ordenes::nuevas(
            &banco.entorno,
            Duration::from_secs(20),
            Arc::clone(&api),
        ));
        let mut terminal = Terminal::new(TestBackend::new(cols, filas)).expect("terminal");
        app.iniciar_pintado(&mut terminal).expect("pintado");
        let mut prueba = Self {
            app,
            terminal,
            banco,
            api,
        };
        prueba.evento(Evento::Servidor(MensajeServidor::Bienvenida {
            version: VERSION_PROTOCOLO,
            version_app: "1.6.0".to_string(),
            pid: 4242,
            cliente_id: 1,
            clientes: 1,
            sesiones: Vec::new(),
            transferencias: Vec::new(),
            tuneles: Vec::new(),
            ejecuciones: Vec::new(),
            acciones_servicio: Vec::new(),
            tuneles_temporales: Vec::new(),
        }));
        prueba
    }

    fn paso(&mut self) {
        self.app
            .paso(&mut self.terminal, Instant::now())
            .expect("paso");
    }

    fn evento(&mut self, evento: Evento) {
        self.app.procesar(evento);
        self.paso();
    }

    fn tecla(&mut self, codigo: KeyCode) {
        self.tecla_con(codigo, KeyModifiers::NONE);
    }

    fn tecla_con(&mut self, codigo: KeyCode, modificadores: KeyModifiers) {
        self.evento(Evento::Tecla(KeyEvent::new(codigo, modificadores)));
    }

    fn escribir(&mut self, texto: &str) {
        for caracter in texto.chars() {
            self.tecla(KeyCode::Char(caracter));
        }
    }

    fn texto(&self) -> String {
        let buffer = self.terminal.backend().buffer();
        let mut lineas = Vec::new();
        for y in 0..buffer.area.height {
            let mut linea = String::new();
            for x in 0..buffer.area.width {
                linea.push_str(buffer[(x, y)].symbol());
            }
            lineas.push(linea.trim_end().to_string());
        }
        lineas.join("\n")
    }

    /// Lo que dice ahora la barra (texto y si es un error).
    fn mensaje(&self) -> Option<(String, bool)> {
        self.app
            .mensaje
            .as_ref()
            .map(|mensaje| (mensaje.texto.clone(), mensaje.error))
    }

    /// Bombea los eventos que lleguen de las tareas hasta que se cumpla la
    /// condición (máximo 15 s).
    fn esperar(&mut self, que: &str, condicion: impl Fn(&Prueba) -> bool) {
        let limite = Instant::now() + Duration::from_secs(15);
        while !condicion(self) {
            assert!(
                Instant::now() < limite,
                "no se cumplió «{que}» en 15 s; barra: {:?}\n{}",
                self.mensaje(),
                self.texto()
            );
            self.app
                .bombear(&mut self.terminal, Duration::from_millis(30))
                .expect("bombear");
        }
    }

    /// Deja pasar las tareas pendientes un rato (para ver que NO pasa algo).
    fn bombear(&mut self, milisegundos: u64) {
        self.app
            .bombear(&mut self.terminal, Duration::from_millis(milisegundos))
            .expect("bombear");
    }

    fn dialogo_de_teclas(&self) -> Option<&DialogoTeclasTermux> {
        match &self.app.dialogo {
            Some(Dialogo::TeclasTermux(dialogo)) => Some(dialogo),
            _ => None,
        }
    }

    fn hay_dialogo_de_teclas(&self) -> bool {
        self.dialogo_de_teclas().is_some()
    }

    /// Abre el diálogo por la paleta, como haría el usuario, y espera a que
    /// salga (la propuesta se arma en una tarea).
    fn abrir_por_la_paleta(&mut self) {
        self.tecla_con(KeyCode::Char('p'), KeyModifiers::CONTROL);
        assert!(self.app.paleta.is_some());
        self.escribir("fila de teclas");
        self.tecla(KeyCode::Enter);
        self.esperar(
            "diálogo de la fila de teclas",
            Prueba::hay_dialogo_de_teclas,
        );
    }

    fn etiquetas_de_la_paleta(&mut self) -> Vec<String> {
        self.tecla_con(KeyCode::Char('p'), KeyModifiers::CONTROL);
        let etiquetas = self
            .app
            .paleta
            .as_ref()
            .expect("paleta")
            .entradas
            .iter()
            .map(|entrada| entrada.etiqueta.clone())
            .collect();
        self.tecla(KeyCode::Esc);
        etiquetas
    }

    /// Los tipos de REGISTRO anotados y su detalle (leídos de la base de la
    /// propia App).
    fn registro(&self) -> Vec<(String, String)> {
        self.app
            .almacen
            .registro_para_exportar(None)
            .expect("registro")
            .into_iter()
            .map(|entrada| (entrada.tipo, entrada.detalle))
            .collect()
    }

    fn instalaciones(&self) -> Vec<String> {
        self.registro()
            .into_iter()
            .filter(|(tipo, _)| tipo == "termux_teclas_instaladas")
            .map(|(_, detalle)| detalle)
            .collect()
    }

    /// Lo que va diciendo la barra, uno a uno, con los turnos de la cola.
    fn vaciar_barra(&mut self) -> Vec<(String, bool)> {
        let mut vistos = Vec::new();
        for _ in 0..30 {
            if let Some(mensaje) = self.app.mensaje.take() {
                vistos.push((mensaje.texto, mensaje.error));
            }
            self.evento(Evento::Tick);
            if self.app.mensaje.is_none() && self.app.termux.cola_barra.is_empty() {
                break;
            }
        }
        vistos
    }
}

fn en_el_arranque(prueba: &mut Prueba) {
    prueba.app.abrir_lo_del_arranque();
    prueba.paso();
}

#[test]
fn la_paleta_ofrece_instalar_la_fila_solo_en_termux() {
    let mut prueba = Prueba::nueva();
    let etiquetas = prueba.etiquetas_de_la_paleta();
    assert!(
        etiquetas.iter().any(|etiqueta| etiqueta == ETIQUETA_PALETA),
        "{etiquetas:?}"
    );
    assert_eq!(ETIQUETA_PALETA, "Instalar la fila de teclas de Termux");
    // Sin Termux (el mismo proceso, la ventana no se porta como tal): no sale
    // y el resto de entradas es el de siempre.
    let con = prueba.etiquetas_de_la_paleta();
    prueba.app.termux.activo = false;
    let sin = prueba.etiquetas_de_la_paleta();
    assert!(!sin.iter().any(|etiqueta| etiqueta == ETIQUETA_PALETA));
    let esperadas: Vec<String> = con
        .into_iter()
        .filter(|etiqueta| etiqueta != ETIQUETA_PALETA)
        .collect();
    assert_eq!(sin, esperadas);
}

#[test]
fn la_paleta_abre_el_dialogo_con_la_propuesta_y_esc_no_toca_nada() {
    let mut prueba = Prueba::nueva();
    let ruta = prueba
        .banco
        .con_propiedades("a = 1\nextra-keys = [['A']]\n");
    prueba.abrir_por_la_paleta();
    let texto = prueba.texto();
    for esperado in [
        "FILA DE TECLAS DE TERMUX",
        "MAGI propone esta fila:",
        "ESC TAB CTRL ALT ^]",
        "F1  F2  F3",
        "~/.termux/termux.properties",
        "una copia:",
        "termux.properties.magi-",
        "↵ instalar",
        "Esc ahora no",
    ] {
        assert!(texto.contains(esperado), "falta «{esperado}»:\n{texto}");
    }
    let dialogo = prueba.dialogo_de_teclas().expect("diálogo");
    assert_eq!(dialogo.propuesta.estado, EstadoFila::Otra);
    assert_eq!(dialogo.propuesta.fila, FILA);
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.dialogo.is_none());
    prueba.bombear(150);
    assert!(prueba.app.dialogo.is_none());
    assert_eq!(
        std::fs::read_to_string(&ruta).unwrap(),
        "a = 1\nextra-keys = [['A']]\n"
    );
    assert_eq!(
        entradas(&prueba.banco.home.join(".termux")),
        ["termux.properties"]
    );
    assert!(prueba.instalaciones().is_empty());
    assert!(prueba.banco.dobles.llamadas(ORDEN_RECARGAR).is_empty());
}

#[test]
fn intro_instala_copia_escribe_anota_y_recarga() {
    let mut prueba = Prueba::nueva();
    let antes = "# mis ajustes\nextra-keys = [['A']]\nfullscreen = true\n";
    let ruta = prueba.banco.con_propiedades(antes);
    prueba.abrir_por_la_paleta();
    let copia = prueba
        .dialogo_de_teclas()
        .and_then(|dialogo| dialogo.propuesta.copia.clone())
        .expect("copia prevista");
    assert!(
        copia.starts_with("termux.properties.magi-") && copia.len() == COPIA.len(),
        "{copia}"
    );
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("fila instalada", |p| {
        !p.banco.dobles.llamadas(ORDEN_RECARGAR).is_empty()
    });
    prueba.esperar("mensaje final", |p| {
        p.mensaje()
            .is_some_and(|(texto, _)| texto == "fila de teclas instalada: ya se ve en Termux")
    });
    assert!(prueba.app.dialogo.is_none());
    assert_eq!(
        std::fs::read_to_string(&ruta).unwrap(),
        format!("# mis ajustes\n{FILA}\nfullscreen = true\n")
    );
    assert_eq!(
        std::fs::read_to_string(prueba.banco.home.join(".termux").join(&copia)).unwrap(),
        antes
    );
    // REGISTRO: el efecto ya producido, con el nombre de la copia.
    assert_eq!(prueba.instalaciones(), [copia]);
    // Una sola llamada, sin argumentos.
    let llamadas = prueba.banco.dobles.llamadas(ORDEN_RECARGAR);
    assert_eq!(llamadas.len(), 1);
    assert!(llamadas[0].args.is_empty());
    // Termux:API sigue como estaba.
    assert_eq!(prueba.api.estado(), EstadoApi::SinComprobar);
}

#[test]
fn sin_fichero_el_dialogo_lo_dice_y_se_crea_al_instalar() {
    let mut prueba = Prueba::nueva();
    prueba.abrir_por_la_paleta();
    let texto = prueba.texto();
    assert!(texto.contains("No existe todavía"), "{texto}");
    assert!(!texto.contains("una copia:"), "{texto}");
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("fichero creado", |p| p.instalaciones() == ["no existía"]);
    assert!(prueba.app.dialogo.is_none());
    let ruta = prueba.banco.propiedades();
    assert_eq!(
        std::fs::read_to_string(&ruta).unwrap(),
        format!("{COMENTARIO_FILA}\n{FILA}\n")
    );
    assert_eq!(modo(&ruta), 0o600);
    assert_eq!(modo(&prueba.banco.home.join(".termux")), 0o700);
}

#[test]
fn si_la_fila_ya_es_la_de_magi_la_paleta_lo_dice_y_no_abre_nada() {
    let mut prueba = Prueba::nueva();
    let ruta = prueba.banco.con_propiedades(format!("{FILA}\n"));
    let modificado = std::fs::metadata(&ruta).unwrap().modified().unwrap();
    prueba.tecla_con(KeyCode::Char('p'), KeyModifiers::CONTROL);
    prueba.escribir("fila de teclas");
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("mensaje de la paleta", |p| {
        p.mensaje()
            .is_some_and(|(texto, _)| texto.contains("ya es la de MAGI"))
    });
    assert!(prueba.app.dialogo.is_none());
    assert_eq!(
        std::fs::metadata(&ruta).unwrap().modified().unwrap(),
        modificado
    );
    assert_eq!(
        entradas(&prueba.banco.home.join(".termux")),
        ["termux.properties"]
    );
}

#[test]
fn un_fichero_ilegible_no_abre_el_dialogo_y_dice_que_se_edite_a_mano() {
    let mut prueba = Prueba::nueva();
    let ruta = prueba.banco.con_propiedades([0xff_u8, 0xfe, 0x00]);
    prueba.tecla_con(KeyCode::Char('p'), KeyModifiers::CONTROL);
    prueba.escribir("fila de teclas");
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("mensaje de error", |p| {
        p.mensaje().is_some_and(|(texto, error)| {
            error && texto.contains("no se puede leer") && texto.contains("edítalo a mano")
        })
    });
    assert!(prueba.app.dialogo.is_none());
    assert_eq!(std::fs::read(&ruta).unwrap(), [0xff, 0xfe, 0x00]);
}

#[test]
fn si_la_instalacion_falla_el_dialogo_sigue_con_el_motivo_y_se_puede_reintentar() {
    let mut prueba = Prueba::nueva();
    let ruta = prueba.banco.con_propiedades("a = 1\n");
    let carpeta = prueba.banco.home.join(".termux");
    prueba.abrir_por_la_paleta();
    let Some(guarda) = SoloLectura::de(&carpeta) else {
        return;
    };
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("error de la copia", |p| {
        p.dialogo_de_teclas()
            .is_some_and(|dialogo| dialogo.trabajando.is_none() && dialogo.error.is_some())
    });
    let error = prueba
        .dialogo_de_teclas()
        .and_then(|dialogo| dialogo.error.clone())
        .expect("error");
    assert!(error.contains("no se ha escrito nada"), "{error}");
    assert!(prueba.texto().contains("no se ha escrito nada"));
    assert_eq!(std::fs::read_to_string(&ruta).unwrap(), "a = 1\n");
    assert!(
        prueba.instalaciones().is_empty(),
        "REGISTRO solo si hay efecto"
    );
    assert!(prueba.banco.dobles.llamadas(ORDEN_RECARGAR).is_empty());
    // Se arregla el permiso y `↵` otra vez instala.
    drop(guarda);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("instalada", |p| p.instalaciones().len() == 1);
    assert!(prueba.app.dialogo.is_none());
    assert!(std::fs::read_to_string(&ruta).unwrap().contains(FILA));
}

/// El diálogo ha terminado un intento fallido: sin instalación en vuelo y con
/// su error a la vista.
fn intento_fallido_terminado(prueba: &Prueba) -> bool {
    prueba
        .dialogo_de_teclas()
        .is_some_and(|dialogo| dialogo.trabajando.is_none() && dialogo.error.is_some())
}

/// K4: la copia ya se hizo y la escritura falla. El diálogo dice que la copia
/// ya está hecha (y cuál es) y cada reintento con `↵` no apila otra (`-2`,
/// `-3`…): reutiliza la de antes mientras siga intacta, y el éxito final lo
/// dice.
#[test]
fn si_la_escritura_falla_tras_la_copia_el_dialogo_lo_dice_y_el_reintento_no_hace_otra() {
    let mut prueba = Prueba::nueva();
    let dotfiles = con_el_destino_en_otro_directorio(&prueba.banco, "a = 1\n");
    let carpeta = prueba.banco.home.join(".termux");
    // Termux tarda 1 s en recargar: da tiempo a ver el mensaje del éxito.
    prueba
        .banco
        .dobles
        .programar(ORDEN_RECARGAR, Modo::Lento(10));
    prueba.abrir_por_la_paleta();
    let copia = prueba
        .dialogo_de_teclas()
        .and_then(|dialogo| dialogo.propuesta.copia.clone())
        .expect("copia prevista");
    let copias = |carpeta: &Path| -> Vec<String> {
        entradas(carpeta)
            .into_iter()
            .filter(|nombre| nombre.contains(".magi-"))
            .collect()
    };
    let Some(guarda) = SoloLectura::de(&dotfiles) else {
        return;
    };

    // Primer intento: la copia se hace y la escritura falla.
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("error de la escritura", intento_fallido_terminado);
    let error = prueba
        .dialogo_de_teclas()
        .and_then(|dialogo| dialogo.error.clone())
        .expect("error");
    assert!(
        error.contains("no se pudo escribir termux.properties (permiso denegado)")
            && error.contains(&format!("la copia {copia} ya está hecha")),
        "{error}"
    );
    assert_eq!(
        prueba
            .dialogo_de_teclas()
            .and_then(|dialogo| dialogo.copia_hecha.clone()),
        Some(copia.clone())
    );
    let pantalla = prueba.texto();
    assert!(
        pantalla.contains(&copia) && pantalla.contains("hecha"),
        "el diálogo no dice cuál es la copia:\n{pantalla}"
    );
    assert_eq!(copias(&carpeta), std::slice::from_ref(&copia));
    assert!(
        prueba.instalaciones().is_empty(),
        "REGISTRO solo si hay efecto"
    );
    // Una instalación que falla no deja ninguna recarga que esperar.
    assert_eq!(prueba.app.teclas_termux.ultima_instalacion, None);

    // Dos reintentos que siguen fallando: ni una copia más, y el mismo aviso.
    for _ in 0..2 {
        prueba.tecla(KeyCode::Enter);
        prueba.esperar("otro error de la escritura", intento_fallido_terminado);
        assert_eq!(
            copias(&carpeta),
            std::slice::from_ref(&copia),
            "apiló otra copia"
        );
        assert_eq!(
            prueba
                .dialogo_de_teclas()
                .and_then(|dialogo| dialogo.error.clone()),
            Some(error.clone())
        );
    }

    // Se arregla el permiso: el reintento escribe con la copia de antes, la
    // anota y lo dice.
    drop(guarda);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("instalada", |p| p.instalaciones().len() == 1);
    assert!(prueba.app.dialogo.is_none());
    assert_eq!(copias(&carpeta), std::slice::from_ref(&copia));
    assert_eq!(prueba.instalaciones(), std::slice::from_ref(&copia));
    assert_eq!(
        std::fs::read_to_string(carpeta.join(&copia)).unwrap(),
        "a = 1\n"
    );
    assert!(std::fs::read_to_string(dotfiles.join("termux.properties"))
        .unwrap()
        .contains(FILA));
    assert!(
        prueba.mensaje().is_some_and(|(texto, error)| {
            !error && texto.contains(&format!("con la copia {copia} de antes"))
        }),
        "el éxito no dice que reutilizó la copia: {:?}",
        prueba.mensaje()
    );
    prueba.esperar("fin de la recarga", |p| {
        p.mensaje()
            .is_some_and(|(texto, _)| texto == "fila de teclas instalada: ya se ve en Termux")
    });
}

/// K4: si el fichero cambió entre el intento fallido y el reintento (la copia
/// ya no es idéntica), el reintento hace su copia: nunca se reutiliza una copia
/// que no vale como respaldo de lo que se va a sobrescribir.
#[test]
fn si_el_fichero_cambia_entre_intentos_el_reintento_hace_una_copia_nueva() {
    let mut prueba = Prueba::nueva();
    let dotfiles = con_el_destino_en_otro_directorio(&prueba.banco, "a = 1\n");
    let carpeta = prueba.banco.home.join(".termux");
    prueba.abrir_por_la_paleta();
    let copia = prueba
        .dialogo_de_teclas()
        .and_then(|dialogo| dialogo.propuesta.copia.clone())
        .expect("copia prevista");
    let Some(guarda) = SoloLectura::de(&dotfiles) else {
        return;
    };
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("error de la escritura", intento_fallido_terminado);
    drop(guarda);
    // El usuario edita su fichero mientras el diálogo sigue abierto.
    std::fs::write(dotfiles.join("termux.properties"), "a = 2\n").unwrap();
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("instalada", |p| p.instalaciones().len() == 1);
    let otra = format!("{copia}-2");
    assert_eq!(prueba.instalaciones(), std::slice::from_ref(&otra));
    assert_eq!(
        std::fs::read_to_string(carpeta.join(&copia)).unwrap(),
        "a = 1\n"
    );
    assert_eq!(
        std::fs::read_to_string(carpeta.join(&otra)).unwrap(),
        "a = 2\n"
    );
}

#[test]
fn mientras_instala_el_dialogo_no_atiende_teclas_y_un_resultado_viejo_se_descarta() {
    let mut prueba = Prueba::nueva();
    prueba.abrir_por_la_paleta();
    // Una instalación en vuelo (la de id 7): ni Esc ni ↵ hacen nada.
    if let Some(Dialogo::TeclasTermux(dialogo)) = prueba.app.dialogo.as_mut() {
        dialogo.trabajando = Some(7);
    }
    prueba.tecla(KeyCode::Esc);
    prueba.tecla(KeyCode::Enter);
    assert!(prueba.hay_dialogo_de_teclas());
    assert!(prueba.texto().contains("instalando…"));
    // El resultado de otra instalación (id 3) no cierra este diálogo ni lo
    // desbloquea, y su error no se pierde.
    prueba.evento(Evento::Termux(magi::app::termux::EventoTermux::Teclas(
        EventoTeclas::Escrita {
            id: 3,
            resultado: Err(ErrorTeclas::Escritura("permiso denegado".to_string())),
        },
    )));
    let dialogo = prueba.dialogo_de_teclas().expect("diálogo");
    assert_eq!(dialogo.trabajando, Some(7));
    assert_eq!(dialogo.error, None);
    assert!(prueba
        .mensaje()
        .is_some_and(|(texto, error)| error && texto.contains("no se pudo escribir")));
    // El de la suya sí lo desbloquea con el motivo.
    prueba.evento(Evento::Termux(magi::app::termux::EventoTermux::Teclas(
        EventoTeclas::Escrita {
            id: 7,
            resultado: Err(ErrorTeclas::Escritura("permiso denegado".to_string())),
        },
    )));
    let dialogo = prueba.dialogo_de_teclas().expect("diálogo");
    assert_eq!(dialogo.trabajando, None);
    assert!(dialogo
        .error
        .as_deref()
        .is_some_and(|e| e.contains("permiso denegado")));
    assert!(prueba.instalaciones().is_empty());
}

#[test]
fn si_termux_no_recarga_la_barra_dice_que_reinicie_y_la_instalacion_ya_esta_anotada() {
    let mut prueba = Prueba::nueva();
    prueba.banco.dobles.programar(ORDEN_RECARGAR, Modo::Fallar);
    prueba.abrir_por_la_paleta();
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("mensaje de la recarga", |p| {
        p.mensaje()
            .is_some_and(|(texto, _)| texto.contains(REINICIA_TERMUX))
    });
    assert_eq!(prueba.instalaciones(), ["no existía"]);
    assert!(std::fs::read_to_string(prueba.banco.propiedades())
        .unwrap()
        .contains(FILA));
}

/// K5: la recarga de una instalación que ya no es la última de la ventana se
/// descarta: no pisa lo que dice la barra ni da por hecho nada de lo que hay
/// ahora. (Antes `fila_recargada` ignoraba el id y atendía cualquier resultado.)
#[test]
fn el_resultado_de_la_recarga_de_otra_instalacion_se_descarta() {
    let mut prueba = Prueba::nueva();
    // Termux tarda 1,5 s en recargar: da tiempo a que llegue una recarga vieja.
    prueba
        .banco
        .dobles
        .programar(ORDEN_RECARGAR, Modo::Lento(15));
    prueba.abrir_por_la_paleta();
    assert_eq!(prueba.app.teclas_termux.ultima_instalacion, None);
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("instalada", |p| p.instalaciones().len() == 1);
    let id = prueba
        .app
        .teclas_termux
        .ultima_instalacion
        .expect("la instalación espera su recarga");
    assert!(prueba
        .mensaje()
        .is_some_and(|(texto, _)| texto.contains("recargando Termux")));

    // Una recarga de otra instalación (de otra, anterior, que responde tarde y
    // mal): ni cambia la barra ni quita la espera de la buena.
    for otro in [id + 1000, id.wrapping_sub(1), 0] {
        prueba.evento(Evento::Termux(magi::app::termux::EventoTermux::Teclas(
            EventoTeclas::Recargada {
                id: otro,
                resultado: Err(FalloOrden::SinRespuesta),
            },
        )));
        assert!(
            prueba
                .mensaje()
                .is_some_and(|(texto, _)| texto.contains("recargando Termux")),
            "la recarga de la instalación {otro} cambió la barra: {:?}",
            prueba.mensaje()
        );
        assert_eq!(prueba.app.teclas_termux.ultima_instalacion, Some(id));
    }

    // La suya sí se atiende, y después ya no se espera ninguna.
    prueba.esperar("fin de la recarga", |p| {
        p.mensaje()
            .is_some_and(|(texto, _)| texto == "fila de teclas instalada: ya se ve en Termux")
    });
    assert_eq!(prueba.app.teclas_termux.ultima_instalacion, None);
    // Una repetida o tardía de la misma instalación tampoco dice nada.
    prueba.app.mensaje = None;
    prueba.evento(Evento::Termux(magi::app::termux::EventoTermux::Teclas(
        EventoTeclas::Recargada {
            id,
            resultado: Err(FalloOrden::SinRespuesta),
        },
    )));
    assert_eq!(prueba.mensaje(), None);
}

/// K5: una instalación que no escribe nada (la fila ya era la de MAGI) no deja
/// una recarga esperando.
#[test]
fn si_la_fila_ya_estaba_no_queda_ninguna_recarga_que_esperar() {
    let mut prueba = Prueba::nueva();
    prueba.banco.con_propiedades("a = 1\n");
    prueba.abrir_por_la_paleta();
    // Mientras el diálogo está abierto, la fila pasa a ser la de MAGI (otra
    // ventana la instaló): `↵` no escribe nada.
    prueba.banco.con_propiedades(format!("{FILA}\n"));
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("sin cambios", |p| {
        p.mensaje()
            .is_some_and(|(texto, _)| texto.contains("no se ha tocado nada"))
    });
    assert_eq!(prueba.app.teclas_termux.ultima_instalacion, None);
    assert!(prueba.banco.dobles.llamadas(ORDEN_RECARGAR).is_empty());
}

#[test]
fn la_instalacion_no_bloquea_la_interfaz_aunque_termux_tarde_en_recargar() {
    let mut prueba = Prueba::nueva();
    // Termux tarda ~3 s en recargar.
    prueba
        .banco
        .dobles
        .programar(ORDEN_RECARGAR, Modo::Lento(30));
    prueba.abrir_por_la_paleta();
    let inicio = Instant::now();
    prueba.tecla(KeyCode::Enter);
    // El bucle sigue vivo: el diálogo se cierra y se anota sin esperar a la
    // recarga, y la barra dice que se está recargando.
    prueba.esperar("fichero instalado", |p| p.instalaciones().len() == 1);
    assert!(prueba.app.dialogo.is_none());
    assert!(
        inicio.elapsed() < Duration::from_millis(2500),
        "tardó {:?}: esperó a la recarga",
        inicio.elapsed()
    );
    assert!(prueba
        .mensaje()
        .is_some_and(|(texto, _)| texto.contains("recargando Termux")));
    prueba.esperar("fin de la recarga", |p| {
        p.mensaje()
            .is_some_and(|(texto, _)| texto == "fila de teclas instalada: ya se ve en Termux")
    });
}

// ------------------------------------------------------------------ el primer arranque

#[test]
fn en_el_primer_arranque_se_ofrece_la_fila_una_sola_vez() {
    let mut prueba = Prueba::nueva();
    prueba.banco.con_propiedades("a = 1\n");
    assert!(!estado::hay_marca(
        &prueba.banco.rutas,
        Marca::PrimerArranque
    ));
    en_el_arranque(&mut prueba);
    prueba.esperar("diálogo del primer arranque", Prueba::hay_dialogo_de_teclas);
    // La marca se puso al ofrecerlo.
    assert!(estado::hay_marca(
        &prueba.banco.rutas,
        Marca::PrimerArranque
    ));
    // La barra remite a magi diagnostico.
    let dicho = prueba.vaciar_barra();
    assert!(
        dicho
            .iter()
            .any(|(texto, error)| texto == MENSAJE_PRIMER_ARRANQUE && !error),
        "{dicho:?}"
    );
    assert_eq!(
        MENSAJE_PRIMER_ARRANQUE,
        "MAGI en Termux: mira magi diagnostico"
    );
    // Esc: ahora no, y nada se escribe.
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.dialogo.is_none());
    assert_eq!(
        std::fs::read_to_string(prueba.banco.propiedades()).unwrap(),
        "a = 1\n"
    );
    assert!(prueba.instalaciones().is_empty());
}

/// K5: la marca del primer arranque se crea de forma exclusiva: de varias
/// ventanas que la piden a la vez, solo una la ve nacer.
#[test]
fn la_marca_del_primer_arranque_la_pone_una_sola_ventana() {
    let banco = Banco::nuevo();
    let hilos = 16;
    let puerta = Arc::new(std::sync::Barrier::new(hilos));
    let resultados: Vec<bool> = (0..hilos)
        .map(|_| {
            let rutas = banco.rutas.clone();
            let puerta = Arc::clone(&puerta);
            std::thread::spawn(move || {
                puerta.wait();
                tomar_marca_del_primer_arranque(&rutas).expect("marca")
            })
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|hilo| hilo.join().expect("hilo"))
        .collect();
    assert_eq!(
        resultados.iter().filter(|primera| **primera).count(),
        1,
        "{resultados:?}"
    );
    // Es la marca de siempre (la ven `estado::hay_marca` y el diagnóstico), con
    // 600, y una vez puesta nadie más la toma.
    assert!(estado::hay_marca(&banco.rutas, Marca::PrimerArranque));
    let marca = estado::directorio(&banco.rutas).join("primer-arranque");
    assert_eq!(modo(&marca), 0o600);
    assert!(!tomar_marca_del_primer_arranque(&banco.rutas).unwrap());
}

/// K5: una marca puesta por `estado::poner_marca` (de una versión anterior o de
/// otra ventana) cuenta: no se vuelve a ofrecer.
#[test]
fn una_marca_ya_puesta_por_el_estado_cuenta_como_ya_ofrecida() {
    let banco = Banco::nuevo();
    estado::poner_marca(&banco.rutas, Marca::PrimerArranque).expect("marca");
    assert!(!tomar_marca_del_primer_arranque(&banco.rutas).unwrap());
    // Y al revés: la que pone esta función es la que lee `hay_marca`.
    let otro = Banco::nuevo();
    assert!(tomar_marca_del_primer_arranque(&otro.rutas).unwrap());
    assert!(estado::hay_marca(&otro.rutas, Marca::PrimerArranque));
}

/// K5: si no se puede dejar la marca (el directorio de estado no se puede
/// crear), se dice con un error y no hay pánico.
#[test]
fn si_no_se_puede_dejar_la_marca_da_error() {
    let banco = Banco::nuevo();
    // Un fichero donde debería haber un directorio.
    let ocupado = banco.base.join("estado-ocupado");
    std::fs::write(&ocupado, "").unwrap();
    let mut rutas = banco.rutas.clone();
    rutas.estado = ocupado.join("magi");
    assert!(tomar_marca_del_primer_arranque(&rutas).is_err());
}

/// K5: dos ventanas de MAGI que arrancan a la vez con la misma marca sin poner
/// no ofrecen el diálogo las dos. Antes comprobaban la marca y la ponían por
/// separado (con un `fsync` entre medias) y las dos la veían sin poner.
#[test]
fn dos_ventanas_lanzadas_a_la_vez_en_el_primer_arranque_no_ofrecen_la_fila_las_dos() {
    let mut primera = Prueba::nueva();
    primera.banco.con_propiedades("a = 1\n");
    let almacen = magi::almacen::abrir(&primera.banco.rutas.base_datos()).expect("almacén");
    let (mut segunda, _) = App::de_prueba(
        primera.banco.rutas.clone(),
        Config::default(),
        Tema::respaldo(),
        almacen,
    )
    .expect("app");
    assert!(segunda.termux.activo);
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
    segunda.iniciar_pintado(&mut terminal).expect("pintado");

    // Las dos arrancan a la vez, antes de que ninguna haya terminado.
    primera.app.abrir_lo_del_arranque();
    segunda.abrir_lo_del_arranque();
    let ofrece = |app: &App| usize::from(matches!(app.dialogo, Some(Dialogo::TeclasTermux(_))));
    let limite = Instant::now() + Duration::from_secs(15);
    let mut turnos = 0;
    // Hasta que alguna ofrezca, y un rato más por si la otra también lo hiciera.
    while turnos < 15 || ofrece(&primera.app) + ofrece(&segunda) == 0 {
        assert!(Instant::now() < limite, "ninguna ventana ofreció la fila");
        primera
            .app
            .bombear(&mut primera.terminal, Duration::from_millis(30))
            .expect("bombear");
        segunda
            .bombear(&mut terminal, Duration::from_millis(30))
            .expect("bombear");
        turnos += 1;
    }
    assert_eq!(
        ofrece(&primera.app) + ofrece(&segunda),
        1,
        "las dos ventanas ofrecieron la fila de teclas"
    );
}

#[test]
fn otra_ventana_con_la_marca_ya_puesta_no_ofrece_nada() {
    let banco = Banco::nuevo();
    estado::poner_marca(&banco.rutas, Marca::PrimerArranque).expect("marca");
    banco.con_propiedades("a = 1\n");
    let mut prueba = Prueba::con_banco(banco, 100, 70);
    en_el_arranque(&mut prueba);
    prueba.bombear(300);
    assert!(prueba.app.dialogo.is_none());
    // Ni siquiera la barra: no es el primer arranque.
    let dicho = prueba.vaciar_barra();
    assert!(
        !dicho
            .iter()
            .any(|(texto, _)| texto == MENSAJE_PRIMER_ARRANQUE),
        "{dicho:?}"
    );
}

#[test]
fn el_segundo_arranque_con_la_misma_marca_no_vuelve_a_ofrecerla() {
    let mut primera = Prueba::nueva();
    primera.banco.con_propiedades("a = 1\n");
    en_el_arranque(&mut primera);
    primera.esperar("diálogo", Prueba::hay_dialogo_de_teclas);
    primera.tecla(KeyCode::Esc);
    // Otra ventana de MAGI con los mismos datos de estado.
    let almacen = magi::almacen::abrir(&primera.banco.rutas.base_datos()).expect("almacén");
    let (mut segunda, _) = App::de_prueba(
        primera.banco.rutas.clone(),
        Config::default(),
        Tema::respaldo(),
        almacen,
    )
    .expect("app");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
    segunda.iniciar_pintado(&mut terminal).expect("pintado");
    segunda.abrir_lo_del_arranque();
    segunda
        .bombear(&mut terminal, Duration::from_millis(300))
        .expect("bombear");
    assert!(segunda.dialogo.is_none());
}

#[test]
fn si_la_fila_ya_es_la_de_magi_no_se_ofrece_pero_la_barra_remite_al_diagnostico() {
    let mut prueba = Prueba::nueva();
    prueba.banco.con_propiedades(format!("{FILA}\n"));
    en_el_arranque(&mut prueba);
    prueba.bombear(400);
    assert!(prueba.app.dialogo.is_none());
    assert!(estado::hay_marca(
        &prueba.banco.rutas,
        Marca::PrimerArranque
    ));
    let dicho = prueba.vaciar_barra();
    assert!(
        dicho
            .iter()
            .any(|(texto, _)| texto == MENSAJE_PRIMER_ARRANQUE),
        "{dicho:?}"
    );
}

#[test]
fn en_el_primer_arranque_un_fichero_ilegible_se_dice_por_la_barra_sin_dialogo() {
    let mut prueba = Prueba::nueva();
    prueba.banco.con_propiedades([0xff_u8, 0xfe]);
    en_el_arranque(&mut prueba);
    prueba.bombear(400);
    assert!(prueba.app.dialogo.is_none());
    let dicho = prueba.vaciar_barra();
    assert!(
        dicho
            .iter()
            .any(|(texto, error)| *error && texto.contains("edítalo a mano")),
        "{dicho:?}"
    );
}

#[test]
fn fuera_de_termux_el_arranque_no_ofrece_nada_ni_pone_la_marca() {
    let mut prueba = Prueba::nueva();
    prueba.app.termux.activo = false;
    en_el_arranque(&mut prueba);
    prueba.bombear(300);
    assert!(prueba.app.dialogo.is_none());
    assert!(!estado::hay_marca(
        &prueba.banco.rutas,
        Marca::PrimerArranque
    ));
}

#[test]
fn con_llavero_primero_la_frase_y_la_fila_queda_debajo_sin_pisarla() {
    let mut prueba = Prueba::nueva();
    prueba.banco.con_propiedades("a = 1\n");
    // Un llavero de fichero cerrado: al arrancar pide la frase.
    let fichero = Arc::new(LlaveroFichero::con_ajustes(
        prueba.banco.rutas.datos.join(NOMBRE_FICHERO),
        6,
        Arc::new(Instant::now),
    ));
    fichero.crear(FRASE, FRASE).expect("crear");
    fichero.bloquear();
    prueba.app.llavero = fichero.clone();
    en_el_arranque(&mut prueba);
    prueba.esperar("fila de teclas debajo", |p| p.app.dialogos_apilados() == 1);
    // La frase está a la vista y lo que se teclee es para ella.
    assert!(
        matches!(prueba.app.dialogo, Some(Dialogo::Llavero(_))),
        "el diálogo a la vista debería ser el del llavero"
    );
    assert!(prueba.texto().contains("LLAVERO DE MAGI"));
    assert!(prueba.instalaciones().is_empty());
    // Esc sigue sin llavero y entonces sale la fila de teclas.
    prueba.tecla(KeyCode::Esc);
    assert!(
        prueba.hay_dialogo_de_teclas(),
        "tras el llavero debería salir la fila de teclas:\n{}",
        prueba.texto()
    );
    assert_eq!(prueba.app.dialogos_apilados(), 0);
}

// ------------------------------------------------------------------ ayuda y REGISTRO

#[test]
fn la_ayuda_en_termux_recuerda_volumen_arriba_y_fuera_de_termux_es_la_de_siempre() {
    let mut prueba = Prueba::nueva();
    prueba.tecla(KeyCode::Char('?'));
    let texto = prueba.texto();
    assert!(
        texto.contains("Volumen arriba") && texto.contains("+ 1…9 (F1…F9) y + 0 (F10)"),
        "{texto}"
    );
    prueba.tecla(KeyCode::Esc);

    prueba.app.termux.activo = false;
    prueba.tecla(KeyCode::Char('?'));
    let texto = prueba.texto();
    assert!(!texto.contains("Volumen arriba"), "{texto}");
}

#[test]
fn la_vista_registro_enseña_entero_el_tipo_termux_teclas_instaladas() {
    let mut prueba = Prueba::nueva();
    prueba.banco.con_propiedades("a = 1\n");
    prueba.abrir_por_la_paleta();
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("instalada", |p| p.instalaciones().len() == 1);
    prueba.tecla(KeyCode::F(7));
    let texto = prueba.texto();
    // Una vez en la fila de la tabla y otra en el panel de detalle de la
    // seleccionada: con la columna del tipo más corta, la de la tabla saldría
    // cortada con «…».
    assert_eq!(
        texto.matches("termux_teclas_instaladas").count(),
        2,
        "el tipo no se ve entero en la tabla:\n{texto}"
    );
    assert!(texto.contains("termux.properties.magi-"), "{texto}");
}

#[test]
fn el_registro_de_la_instalacion_no_lleva_la_fila_del_usuario_ni_el_contenido() {
    let mut prueba = Prueba::nueva();
    prueba
        .banco
        .con_propiedades("secreto = clave-que-no-debe-salir\nextra-keys = [['MIA']]\n");
    prueba.abrir_por_la_paleta();
    prueba.tecla(KeyCode::Enter);
    prueba.esperar("instalada", |p| p.instalaciones().len() == 1);
    for (tipo, detalle) in prueba.registro() {
        for texto in [&tipo, &detalle] {
            assert!(!texto.contains("clave-que-no-debe-salir"), "{texto}");
            assert!(!texto.contains("MIA"), "{texto}");
        }
    }
}
