//! Pruebas de caja de `terminal_tmux.sh` (Fase 15, ampliadas en la Fase 16):
//! se ejecuta de verdad con `sh` y el tmux falso compartido de
//! `tests/fijos/terminal/tmux` en el `PATH` (nunca el tmux del sistema), y se
//! comprueba la salida exacta que lee el núcleo. Sin red.
//!
//! Las pruebas con tmux de verdad están en `tmux_real.rs`.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use magi_nucleo::sesiones_persistentes::lectura::{
    leer_comprobar, leer_crear, leer_estado, leer_renombrar, leer_terminar as leer_terminar_nuevo,
    ResultadoCrear, ResultadoRenombrar, TerminfoTmux,
};
use magi_nucleo::sesiones_persistentes::{ServidorTmux, VersionTmux};
use magi_nucleo::terminal::tmux::{
    carpeta_valida, comando_valido, equipo_valido, etiqueta_equipo, fin_de_enganchar,
    leer_terminar, linea_de_exec, nombre_valido, version_de_texto, FinEnganchar, OrdenTmux,
    ResultadoTerminar, ARRANQUE, SCRIPT, SCRIPT_ENGANCHE,
};

/// Salida cruda de una ejecución del script.
struct Salida {
    stdout: String,
    stderr: String,
    codigo: i32,
}

/// El tmux falso compartido con las pruebas del escritorio; su contrato está
/// en `tests/fijos/terminal/README.md`.
const FAKE_TMUX: &str = include_str!("../../../tests/fijos/terminal/tmux");

/// Shell de acceso falso: copia sus argumentos al fichero de estado.
const FAKE_SHELL: &str = r#"#!/bin/sh
printf '%s\n' "$*" > "$MAGI_SHELL_ESTADO"
"#;

/// `infocmp` falso: dice que tiene los terminales que liste
/// `MAGI_TERMINFO_TIENE` (separados por espacios).
const FAKE_INFOCMP: &str = r#"#!/bin/sh
for terminal in ${MAGI_TERMINFO_TIENE:-}; do
    [ "$terminal" = "$1" ] && exit 0
done
exit 1
"#;

/// Separador de los campos de `MAGI_SES` y de la bitácora de argumentos.
const US: char = '\u{1f}';

/// Temporal con el tmux falso (o sin él) y un shell de acceso falso.
struct Entorno {
    _temporal: tempfile::TempDir,
    bin: PathBuf,
    con_tmux: bool,
    estado: PathBuf,
    estado_magi: PathBuf,
    llamadas: PathBuf,
    argumentos: PathBuf,
    entorno_tmux: PathBuf,
    shell_estado: PathBuf,
    home: PathBuf,
}

impl Entorno {
    fn nuevo(tmux: Option<&str>) -> Self {
        let temporal = tempfile::tempdir().expect("temporal de la prueba");
        // El tmux falso compartido se escribe una sola vez: reescribirlo en
        // cada prueba lo deja abierto para escritura mientras otra prueba lo
        // ejecuta y el núcleo devuelve ETXTBSY ("text file busy").
        let bin = if tmux == Some(FAKE_TMUX) {
            binarios_compartidos().clone()
        } else {
            let bin = temporal.path().join("bin");
            fs::create_dir_all(&bin).expect("directorio de binarios falsos");
            if let Some(guion) = tmux {
                instalar(&bin.join("tmux"), guion);
            }
            // Sin tmux, el PATH del hijo es solo este directorio, y el arranque
            // del script necesita `cat` (todo host lo tiene): se le enlaza.
            for cat in ["/usr/bin/cat", "/bin/cat"] {
                if Path::new(cat).exists() {
                    std::os::unix::fs::symlink(cat, bin.join("cat")).expect("enlace a cat");
                    break;
                }
            }
            instalar(&bin.join("shell"), FAKE_SHELL);
            instalar(&bin.join("infocmp"), FAKE_INFOCMP);
            bin
        };
        let estado = temporal.path().join("estado");
        let estado_magi = temporal.path().join("estado_magi");
        let llamadas = temporal.path().join("llamadas");
        let argumentos = temporal.path().join("argumentos");
        let entorno_tmux = temporal.path().join("entorno_tmux");
        let shell_estado = temporal.path().join("shell.txt");
        let home = temporal.path().join("home");
        fs::create_dir_all(&home).expect("home de la prueba");
        Self {
            _temporal: temporal,
            bin,
            con_tmux: tmux.is_some(),
            estado,
            estado_magi,
            llamadas,
            argumentos,
            entorno_tmux,
            shell_estado,
            home,
        }
    }

    /// Siembra el fichero de estado del tmux falso habitual con estas sesiones
    /// (nombre, adheridos, ventanas, creada), una por línea y con tabuladores.
    fn sembrar_sesiones(&self, sesiones: &[(&str, u32, u16, &str)]) {
        let mut contenido = String::new();
        for (nombre, adheridos, ventanas, creada) in sesiones {
            contenido.push_str(&format!("{nombre}\t{adheridos}\t{ventanas}\t{creada}\n"));
        }
        fs::write(&self.estado, contenido).expect("escribir el estado del tmux falso");
    }

    /// Siembra el estado de un servidor con líneas tal cual (campos separados
    /// por tabuladores; hasta nueve, §README del tmux falso).
    fn sembrar_lineas(&self, fichero: &Path, lineas: &[String]) {
        let mut contenido = lineas.join("\n");
        contenido.push('\n');
        fs::write(fichero, contenido).expect("escribir el estado del tmux falso");
    }

    /// Siembra el servidor propio (`-L magi`) con sesiones sencillas.
    fn sembrar_magi(&self, nombres: &[&str]) {
        let lineas: Vec<String> = nombres
            .iter()
            .map(|nombre| format!("{nombre}\t0\t1\t2026-11-09 19:00:00"))
            .collect();
        self.sembrar_lineas(&self.estado_magi, &lineas);
    }

    fn estado(&self) -> String {
        fs::read_to_string(&self.estado).unwrap_or_default()
    }

    fn estado_de_magi(&self) -> String {
        fs::read_to_string(&self.estado_magi).unwrap_or_default()
    }

    fn tiene_sesion(&self, nombre: &str) -> bool {
        self.estado()
            .lines()
            .any(|linea| linea.split('\t').next() == Some(nombre))
    }

    fn magi_tiene(&self, nombre: &str) -> bool {
        self.estado_de_magi()
            .lines()
            .any(|linea| linea.split('\t').next() == Some(nombre))
    }

    /// La bitácora de llamadas a tmux sin el `-u` de delante, que el script
    /// pone siempre y comprueba `todo_tmux_va_en_modo_utf8`.
    fn llamadas(&self) -> String {
        let mut texto: String = self
            .llamadas_crudas()
            .lines()
            .map(|linea| linea.strip_prefix("-u ").unwrap_or(linea))
            .collect::<Vec<&str>>()
            .join("\n");
        if !texto.is_empty() {
            texto.push('\n');
        }
        texto
    }

    fn llamadas_crudas(&self) -> String {
        fs::read_to_string(&self.llamadas).unwrap_or_default()
    }

    /// Cada invocación a tmux con sus argumentos sueltos (la bitácora de
    /// argumentos del tmux falso).
    fn invocaciones(&self) -> Vec<Vec<String>> {
        self.invocaciones_crudas()
            .into_iter()
            .map(|mut partes| {
                // Sin el `-u` de delante (lo comprueba `todo_tmux_va_en_modo_utf8`).
                if partes.first().map(String::as_str) == Some("-u") {
                    partes.remove(0);
                }
                partes
            })
            .collect()
    }

    fn invocaciones_crudas(&self) -> Vec<Vec<String>> {
        fs::read_to_string(&self.argumentos)
            .unwrap_or_default()
            .lines()
            .map(|linea| {
                let mut partes: Vec<String> = linea.split(US).map(str::to_string).collect();
                partes.pop();
                partes
            })
            .collect()
    }

    fn shell_recibido(&self) -> Option<String> {
        fs::read_to_string(&self.shell_estado).ok()
    }

    fn ejecutar(&self, argumentos: &[&str]) -> Salida {
        self.ejecutar_con(argumentos, &[])
    }

    /// Ejecuta una suborden de `terminal_tmux.sh` como lo hace MAGI en el host:
    /// con el arranque mínimo en la línea de órdenes y el script ENTERO por la
    /// entrada estándar (D357), con variables de entorno adicionales (versión
    /// del tmux falso, fallos, `HOME`…).
    fn ejecutar_con(&self, argumentos: &[&str], entorno: &[(&str, &str)]) -> Salida {
        self.ejecutar_con_entrada(argumentos, entorno, SCRIPT)
    }

    /// Como [`Self::ejecutar_con`], con el texto que llega por la entrada
    /// estándar (para las pruebas del arranque con un script cortado o vacío).
    fn ejecutar_con_entrada(
        &self,
        argumentos: &[&str],
        entorno: &[(&str, &str)],
        entrada: &'static str,
    ) -> Salida {
        let mut hijo = self
            .preparar(argumentos, entorno)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("ejecutar terminal_tmux.sh");
        let mut pipe = hijo.stdin.take().expect("entrada del script");
        // Se escribe en otro hilo: el hijo no lee más que cuando `cat` llega al
        // final, y no hay que depender del tamaño del tubo.
        let escritor = std::thread::spawn(move || {
            let _ = pipe.write_all(entrada.as_bytes());
        });
        let salida = hijo.wait_with_output().expect("esperar al script");
        let _ = escritor.join();
        Salida {
            stdout: String::from_utf8_lossy(&salida.stdout).to_string(),
            stderr: String::from_utf8_lossy(&salida.stderr).to_string(),
            codigo: salida.status.code().unwrap_or(-1),
        }
    }

    /// El comando de una suborden de `terminal_tmux.sh` con el entorno de la
    /// prueba, sin lanzarlo (la línea de órdenes es el arranque mínimo: el
    /// script va por la entrada estándar).
    fn preparar(&self, argumentos: &[&str], entorno: &[(&str, &str)]) -> Command {
        // /bin/sh absoluto: el PATH del hijo es solo el de los falsos, para
        // que el script no vea el tmux real de la máquina.
        let mut comando = Command::new("/bin/sh");
        comando.arg("-c").arg(ARRANQUE).arg("magi-tmux");
        for argumento in argumentos {
            comando.arg(argumento);
        }
        self.entorno_de(&mut comando, entorno);
        comando
    }

    /// Ejecuta el enganche mínimo (`terminal_enganche.sh`) con la entrada
    /// cerrada, como un `exec` de ssh sin nadie que escriba: `<servidor>
    /// <nombre> <soltar> <equipo> <guardar>`.
    fn enganche(&self, argumentos: &[&str]) -> Salida {
        self.enganche_con(argumentos, &[])
    }

    fn enganche_con(&self, argumentos: &[&str], entorno: &[(&str, &str)]) -> Salida {
        let salida = self
            .preparar_enganche(argumentos, entorno)
            .output()
            .expect("ejecutar terminal_enganche.sh");
        Salida {
            stdout: String::from_utf8_lossy(&salida.stdout).to_string(),
            stderr: String::from_utf8_lossy(&salida.stderr).to_string(),
            codigo: salida.status.code().unwrap_or(-1),
        }
    }

    /// El comando del enganche mínimo con el entorno de la prueba, sin
    /// lanzarlo (para las que necesitan la entrada abierta o el hijo vivo).
    fn preparar_enganche(&self, argumentos: &[&str], entorno: &[(&str, &str)]) -> Command {
        let mut comando = Command::new("/bin/sh");
        comando.arg("-c").arg(SCRIPT_ENGANCHE).arg("magi-enganche");
        for argumento in argumentos {
            comando.arg(argumento);
        }
        self.entorno_de(&mut comando, entorno);
        comando
    }

    /// Engancharse como lo hace el servidor: primero `preparar` por la entrada
    /// estándar y, si sale bien, el enganche mínimo. Devuelve la salida de la
    /// primera mitad si falla y la del enganche si no.
    fn enganchar(&self, servidor: &str, nombre: &str, soltar: &str, equipo: &str) -> Salida {
        let preparada = self.ejecutar(&["preparar", servidor, nombre]);
        if preparada.codigo != 0 {
            return preparada;
        }
        self.enganche(&[servidor, nombre, soltar, equipo, "0"])
    }

    /// Pone al comando el entorno de la prueba.
    fn entorno_de(&self, comando: &mut Command, entorno: &[(&str, &str)]) {
        // Con tmux falso, el PATH lleva además los directorios del sistema
        // para que el falso encuentre awk/sed/mkdir; el falso va primero, así
        // que nunca se usa un tmux real. Sin tmux, el PATH no ve ninguno.
        let path = if self.con_tmux {
            format!("{}:/usr/bin:/bin", self.bin.display())
        } else {
            self.bin.display().to_string()
        };
        comando
            .env_clear()
            .env("PATH", path)
            .env("HOME", &self.home)
            .env("SHELL", self.bin.join("shell"))
            .env("MAGI_TMUX_ESTADO", &self.estado)
            .env("MAGI_TMUX_ESTADO_MAGI", &self.estado_magi)
            .env("MAGI_TMUX_LLAMADAS", &self.llamadas)
            .env("MAGI_TMUX_ARGUMENTOS", &self.argumentos)
            .env("MAGI_TMUX_ENTORNO", &self.entorno_tmux)
            .env("MAGI_SHELL_ESTADO", &self.shell_estado)
            .env("MAGI_TERMINFO_TIENE", "tmux-256color screen-256color");
        for (clave, valor) in entorno {
            comando.env(clave, valor);
        }
    }

    /// Ejecuta la línea de comando que recibiría el shell de acceso del host
    /// en un `exec` de ssh (`sh -c '<script o arranque>' magi-tmux …`, ya
    /// escapada), con el script por la entrada estándar si la orden lo lleva.
    fn ejecutar_linea(&self, linea: &str, entrada: Option<&'static str>) -> Salida {
        let mut comando = Command::new("/bin/sh");
        comando.arg("-c").arg(linea);
        self.entorno_de(&mut comando, &[]);
        let mut hijo = comando
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("ejecutar la línea de exec");
        let mut pipe = hijo.stdin.take().expect("entrada del exec");
        let escritor = std::thread::spawn(move || {
            if let Some(entrada) = entrada {
                let _ = pipe.write_all(entrada.as_bytes());
            }
            // Al soltar la tubería, el hijo ve el fin de la entrada.
        });
        let salida = hijo.wait_with_output().expect("esperar a la línea de exec");
        let _ = escritor.join();
        Salida {
            stdout: String::from_utf8_lossy(&salida.stdout).to_string(),
            stderr: String::from_utf8_lossy(&salida.stderr).to_string(),
            codigo: salida.status.code().unwrap_or(-1),
        }
    }
}

/// Binarios falsos compartidos por todas las pruebas: se escriben una sola
/// vez, al principio, y nunca se vuelven a abrir para escritura.
///
/// Viven en el directorio de temporales de cargo (`target/tmp`), con un nombre
/// fijo, y no en `/tmp` con el pid: allí nadie los borraría y cada ejecución
/// dejaría una carpeta más. Si ya están y son iguales, no se tocan; si no, se
/// escriben con otro nombre y se renombran encima (el rename es atómico y un
/// proceso de otra ejecución nunca ve un binario a medio escribir ni abierto
/// para escritura: ETXTBSY).
fn binarios_compartidos() -> &'static PathBuf {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("magi-falsos-tmux");
        fs::create_dir_all(&dir).expect("directorio de binarios falsos compartidos");
        instalar_si_cambia(&dir.join("tmux"), FAKE_TMUX);
        instalar_si_cambia(&dir.join("shell"), FAKE_SHELL);
        instalar_si_cambia(&dir.join("infocmp"), FAKE_INFOCMP);
        dir
    })
}

fn instalar_si_cambia(destino: &Path, contenido: &str) {
    if fs::read_to_string(destino).is_ok_and(|actual| actual == contenido) {
        return;
    }
    let temporal = destino.with_extension(format!("{}.tmp", std::process::id()));
    instalar(&temporal, contenido);
    fs::rename(&temporal, destino).expect("colocar el binario falso");
}

fn instalar(destino: &Path, contenido: &str) {
    fs::write(destino, contenido).expect("escribir el binario falso");
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(destino, fs::Permissions::from_mode(0o755))
        .expect("permisos del binario falso");
}

#[test]
fn un_tmux_que_falla_al_pedir_la_version_cuenta_como_ausente() {
    let entorno = Entorno::nuevo(Some(
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$MAGI_TMUX_LLAMADAS\"\nexit 1\n",
    ));
    let salida = entorno.ejecutar(&["estado"]);
    assert_eq!(salida.codigo, 0);
    assert_eq!(salida.stdout, "MAGI_TMUX=no\n");
    assert_eq!(entorno.llamadas().trim_end(), "-V");
}

#[test]
fn un_tmux_anterior_a_1_8_se_trata_como_sin_tmux() {
    let viejo = "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$MAGI_TMUX_LLAMADAS\"\ncase \"$1\" in\n    -V) printf '%s\\n' 'tmux 1.7' ;;\nesac\n";
    let entorno = Entorno::nuevo(Some(viejo));
    let salida = entorno.ejecutar(&["estado"]);
    assert_eq!(salida.stdout, "MAGI_TMUX=no\n");
    // Prepararse con un tmux viejo no engancha a nada (ni abre un shell): el
    // enganche ni se lanza.
    let enganchar = entorno.enganchar("habitual", "portatil-1", "0", "portatil");
    assert_eq!(enganchar.codigo, 76, "{}", enganchar.stderr);
    assert!(!entorno.llamadas().contains("attach-session"));
    assert!(entorno.shell_recibido().is_none());
}

#[test]
fn los_nombres_invalidos_se_rechazan_sin_tocar_tmux() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    let largo = "a".repeat(65);
    let invalidos = [
        "",
        "mal nombre",
        "a;b",
        "acentuado-é",
        "a/b",
        "a|b",
        largo.as_str(),
    ];
    for nombre in invalidos {
        let salida = entorno.ejecutar(&["comprobar", "magi", nombre]);
        assert_eq!(salida.codigo, 64, "nombre rechazado: {nombre:?}");
        assert!(salida.stdout.is_empty(), "nombre rechazado: {nombre:?}");
        assert!(
            salida.stderr.contains("uso:"),
            "nombre rechazado: {nombre:?}"
        );
    }
    assert!(entorno.llamadas().is_empty());
}

#[test]
fn los_argumentos_de_mas_o_de_menos_y_los_subordenes_desconocidos_son_uso() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    // Las subórdenes de la Fase 15 (`abrir`, `listar` y las formas de un solo
    // argumento de `comprobar` y `terminar`) ya no existen.
    let casos: [&[&str]; 9] = [
        &[],
        &["comprobar"],
        &["comprobar", "portatil-1"],
        &["terminar", "portatil-1"],
        &["listar"],
        &["abrir", "portatil-1"],
        &["comprobar", "magi", "portatil-1", "de-mas"],
        &["estado", "de-mas"],
        &["pepe", "portatil-1"],
    ];
    for argumentos in casos {
        let salida = entorno.ejecutar(argumentos);
        assert_eq!(salida.codigo, 64, "argumentos: {argumentos:?}");
        assert!(salida.stderr.contains("uso:"), "argumentos: {argumentos:?}");
    }
    assert!(entorno.llamadas().is_empty());
}

#[test]
fn los_nombres_validos_de_64_caracteres_pasan() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    let nombre = "A_z-09".to_string() + &"b".repeat(58);
    assert_eq!(nombre.len(), 64);
    let salida = entorno.ejecutar(&["comprobar", "magi", &nombre]);
    assert_eq!(salida.codigo, 0);
    assert!(
        salida
            .stdout
            .starts_with("MAGI_TMUX=tmux 3.4\nMAGI_SESION=nueva\n"),
        "{}",
        salida.stdout
    );
    assert!(entorno
        .llamadas()
        .contains("list-sessions -F #{session_name}"));
}

#[test]
fn los_nombres_admitidos_son_letras_numeros_guion_y_guion_bajo() {
    assert!(nombre_valido("magi-portatil-1"));
    assert!(nombre_valido("A_z-09"));
    assert!(nombre_valido(&"a".repeat(64)));
    assert!(!nombre_valido(""));
    assert!(!nombre_valido(&"a".repeat(65)));
    assert!(!nombre_valido("mal nombre"));
    assert!(!nombre_valido("con-ñ"));
    assert!(!nombre_valido("a:b"));
    assert!(!nombre_valido("a\nb"));
}

#[test]
fn la_etiqueta_del_equipo_va_sin_tildes_y_con_guiones() {
    assert_eq!(etiqueta_equipo("Portátil de Héctor"), "portatil-de-hector");
    assert_eq!(etiqueta_equipo("  WEB-01  "), "web-01");
    assert_eq!(etiqueta_equipo("a  b"), "a-b");
    assert_eq!(etiqueta_equipo("Ñandú"), "nandu");
    assert_eq!(etiqueta_equipo(""), "equipo");
    assert_eq!(etiqueta_equipo("!!!"), "equipo");
    assert_eq!(etiqueta_equipo(&"x".repeat(40)).chars().count(), 20);
    // El recorte a 20 no deja un guion al final.
    let recortada = etiqueta_equipo("abcde fghij klmno pqrst");
    assert_eq!(recortada, "abcde-fghij-klmno-pq");
    assert!(!recortada.ends_with('-'));
}

#[test]
fn leer_terminar_solo_da_ok_con_la_marca_exacta() {
    assert_eq!(leer_terminar("MAGI_RESULTADO=ok\n"), ResultadoTerminar::Ok);
    assert_eq!(
        leer_terminar("MAGI_RESULTADO=ok\r\n"),
        ResultadoTerminar::Ok
    );
    assert_eq!(
        leer_terminar("basura\nMAGI_RESULTADO=ok\n"),
        ResultadoTerminar::Ok
    );
    assert_eq!(
        leer_terminar("MAGI_RESULTADO=no_existe\n"),
        ResultadoTerminar::NoExiste
    );
    assert_eq!(leer_terminar(""), ResultadoTerminar::NoExiste);
    assert_eq!(
        leer_terminar("MAGI_RESULTADO=OK\n"),
        ResultadoTerminar::NoExiste
    );
    assert_eq!(
        leer_terminar("MAGI_RESULTADO=ok de mas\n"),
        ResultadoTerminar::NoExiste
    );
}

#[test]
fn el_tmux_falso_engancha_y_devuelve_la_entrada() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    let mut hijo = Command::new(entorno.bin.join("tmux"))
        .args(["new-session", "-A", "-s", "magi-portatil-1"])
        .env("MAGI_TMUX_ESTADO", &entorno.estado)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("arrancar el tmux falso");
    hijo.stdin
        .as_mut()
        .expect("entrada del tmux falso")
        .write_all(b"hola\n")
        .expect("escribir en el PTY falso");
    let salida = hijo.wait_with_output().expect("esperar al tmux falso");
    assert_eq!(
        String::from_utf8_lossy(&salida.stdout),
        "MAGI_TMUX_SESION=magi-portatil-1\nMAGI_TMUX_ENTRADA=hola\n"
    );
    assert_eq!(salida.status.code(), Some(0));
    assert!(entorno.tiene_sesion("magi-portatil-1"));
}

#[test]
fn el_tmux_falso_sustituye_los_campos_del_formato() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_sesiones(&[("magi-portatil-1", 1, 3, "lun 9 nov")]);
    let salida = Command::new(entorno.bin.join("tmux"))
        .args([
            "list-sessions",
            "-F",
            "#{session_name}:#{session_attached}:#{session_windows}:#{session_created_string}",
        ])
        .env("MAGI_TMUX_ESTADO", &entorno.estado)
        .output()
        .expect("ejecutar el tmux falso");
    assert_eq!(salida.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&salida.stdout).trim_end(),
        "magi-portatil-1:1:3:lun 9 nov"
    );
}

#[test]
fn el_tmux_falso_avisa_si_no_hay_sesiones() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    let salida = Command::new(entorno.bin.join("tmux"))
        .args(["list-sessions", "-F", "#{session_name}"])
        .env("MAGI_TMUX_ESTADO", &entorno.estado)
        .output()
        .expect("ejecutar el tmux falso");
    assert_eq!(salida.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&salida.stderr).contains("no server running"));
}

// ======================================================================
// Fase 16: servidor propio, configuración y subórdenes nuevas
// ======================================================================

/// Las líneas de la bitácora que son órdenes al tmux propio.
fn lineas_del_propio(entorno: &Entorno) -> Vec<String> {
    entorno
        .llamadas()
        .lines()
        .filter(|linea| linea.starts_with("-L magi "))
        .map(str::to_string)
        .collect()
}

fn linea_de_estado(campos: &[&str]) -> String {
    format!("MAGI_SES{US}{}", campos.join(&US.to_string()))
}

#[test]
fn comprobar_dice_version_existencia_y_terminfo_por_servidor() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["agente"]);
    entorno.sembrar_sesiones(&[("web", 0, 1, "2026-11-09 19:00:00")]);

    let propio = entorno.ejecutar(&["comprobar", "magi", "agente"]);
    assert_eq!(propio.codigo, 0);
    assert_eq!(
        propio.stdout,
        "MAGI_TMUX=tmux 3.4\nMAGI_SESION=existe\nMAGI_TERM=tmux-256color\n"
    );
    let lectura = leer_comprobar(&propio.stdout);
    assert_eq!(lectura.tmux, Some(VersionTmux::nueva(3, 4)));
    assert!(lectura.existe);
    assert_eq!(lectura.terminfo, Some(TerminfoTmux::Tmux256));

    // Cada servidor tiene las suyas: «agente» no está en el habitual.
    let habitual = entorno.ejecutar(&["comprobar", "habitual", "agente"]);
    assert!(habitual.stdout.contains("MAGI_SESION=nueva\n"));
    let habitual = entorno.ejecutar(&["comprobar", "habitual", "web"]);
    assert!(habitual.stdout.contains("MAGI_SESION=existe\n"));
    let propio = entorno.ejecutar(&["comprobar", "magi", "web"]);
    assert!(propio.stdout.contains("MAGI_SESION=nueva\n"));

    // El propio va siempre con -L magi -f /dev/null; el habitual, sin ellos.
    let llamadas = entorno.llamadas();
    assert!(llamadas.contains("-L magi -f /dev/null list-sessions -F #{session_name}"));
    assert!(llamadas
        .lines()
        .any(|linea| linea == "list-sessions -F #{session_name}"));
}

#[test]
fn comprobar_cae_a_screen_si_el_host_no_tiene_la_terminfo_de_tmux() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    let salida = entorno.ejecutar_con(
        &["comprobar", "magi", "agente"],
        &[("MAGI_TERMINFO_TIENE", "screen-256color")],
    );
    assert!(salida.stdout.ends_with("MAGI_TERM=screen-256color\n"));
    assert_eq!(
        leer_comprobar(&salida.stdout).terminfo,
        Some(TerminfoTmux::Screen256)
    );
}

#[test]
fn comprobar_sin_tmux_dice_que_no() {
    let entorno = Entorno::nuevo(None);
    let salida = entorno.ejecutar(&["comprobar", "magi", "agente"]);
    assert_eq!(salida.codigo, 0);
    assert_eq!(salida.stdout, "MAGI_TMUX=no\nMAGI_SESION=nueva\n");
    assert!(entorno.llamadas().is_empty());
}

#[test]
fn la_existencia_es_por_nombre_exacto_y_no_por_prefijo() {
    // Con magi-a-10 viva y ninguna magi-a-1, magi-a-1 no existe.
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["magi-a-10"]);
    let salida = entorno.ejecutar(&["comprobar", "magi", "magi-a-1"]);
    assert!(salida.stdout.contains("MAGI_SESION=nueva\n"));
    let salida = entorno.ejecutar(&["comprobar", "magi", "magi-a-10"]);
    assert!(salida.stdout.contains("MAGI_SESION=existe\n"));
}

#[test]
fn terminar_magi_a_1_no_mata_a_magi_a_10() {
    // AC (T126, R146): con magi-a-10 viva y ninguna magi-a-1, terminar
    // magi-a-1 da no_existe y magi-a-10 sigue viva.
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["magi-a-10"]);
    let salida = entorno.ejecutar(&["terminar", "magi", "magi-a-1"]);
    assert_eq!(salida.codigo, 0);
    assert_eq!(salida.stdout, "MAGI_RESULTADO=no_existe\n");
    assert_eq!(
        leer_terminar_nuevo(&salida.stdout),
        ResultadoTerminar::NoExiste
    );
    assert!(entorno.magi_tiene("magi-a-10"));
    // La existencia se comprueba con la lista exacta: ni siquiera se intenta.
    assert!(!entorno.llamadas().contains("kill-session"));

    // Lo mismo en el tmux habitual. La forma antigua del script (un solo
    // argumento, que actuaba sobre el habitual) ya no existe.
    entorno.sembrar_sesiones(&[("magi-a-10", 0, 1, "2026-11-09 19:00:00")]);
    let habitual = entorno.ejecutar(&["terminar", "habitual", "magi-a-1"]);
    assert_eq!(habitual.stdout, "MAGI_RESULTADO=no_existe\n");
    let antigua = entorno.ejecutar(&["terminar", "magi-a-1"]);
    assert_eq!(antigua.codigo, 64);
    assert!(antigua.stdout.is_empty());
    assert!(entorno.tiene_sesion("magi-a-10"));
}

#[test]
fn terminar_mata_solo_la_sesion_pedida_con_igual() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["magi-a-1", "magi-a-10"]);
    let salida = entorno.ejecutar(&["terminar", "magi", "magi-a-1"]);
    assert_eq!(salida.stdout, "MAGI_RESULTADO=ok\n");
    assert_eq!(leer_terminar_nuevo(&salida.stdout), ResultadoTerminar::Ok);
    assert!(!entorno.magi_tiene("magi-a-1"));
    assert!(entorno.magi_tiene("magi-a-10"));
    assert!(lineas_del_propio(&entorno)
        .iter()
        .any(|linea| linea.ends_with("kill-session -t =magi-a-1")));
}

#[test]
fn terminar_sin_tmux_o_con_otro_servidor() {
    let sin = Entorno::nuevo(None);
    let salida = sin.ejecutar(&["terminar", "magi", "agente"]);
    assert_eq!(salida.stdout, "MAGI_RESULTADO=no_existe\n");

    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_sesiones(&[("web", 0, 1, "2026-11-09 19:00:00")]);
    // «web» está en el habitual, no en el propio.
    let propio = entorno.ejecutar(&["terminar", "magi", "web"]);
    assert_eq!(propio.stdout, "MAGI_RESULTADO=no_existe\n");
    assert!(entorno.tiene_sesion("web"));
    let habitual = entorno.ejecutar(&["terminar", "habitual", "web"]);
    assert_eq!(habitual.stdout, "MAGI_RESULTADO=ok\n");
    assert!(!entorno.tiene_sesion("web"));
}

// ------------------------------------------------------------------ crear

/// Opciones que `crear` debe pasar según la versión de tmux: (versión,
/// opciones que lleva, opciones que no).
#[test]
fn la_configuracion_depende_de_la_version_de_tmux() {
    let comunes = [
        "set -gq status off",
        "set -gq prefix None",
        "set -gq prefix2 None",
        "set -gq mouse on",
        "set -sgq set-clipboard on",
        "set -sgq focus-events on",
        "set -gq set-titles on",
        "set -gq set-titles-string #T",
        "set -gq history-limit 7000",
        "set -sgq default-terminal tmux-256color",
        "set -sgq escape-time 10",
        "set -gq monitor-bell on",
        "set -gq bell-action any",
    ];
    // (texto de tmux -V, lleva, no lleva)
    let casos: [(&str, &[&str], &[&str]); 9] = [
        (
            "tmux 3.0a",
            &["set -sgaq terminal-overrides ,xterm*:Tc"],
            &[
                "extended-keys",
                "terminal-features",
                "allow-passthrough",
                "window-size",
            ],
        ),
        (
            "tmux 3.1c",
            &[
                "set -sgaq terminal-overrides ,xterm*:Tc",
                "set -gq window-size latest",
            ],
            &["extended-keys", "terminal-features", "allow-passthrough"],
        ),
        (
            "tmux 3.2a",
            &[
                "set -sgq extended-keys on",
                "set -sgq terminal-features xterm*:RGB:clipboard:extkeys:focus:title",
                "set -gq window-size latest",
            ],
            &[
                "extended-keys-format",
                "allow-passthrough",
                "terminal-overrides",
            ],
        ),
        (
            "tmux 3.3a",
            &["set -sgq extended-keys on", "set -gq allow-passthrough on"],
            &["extended-keys-format", "terminal-overrides"],
        ),
        (
            "tmux 3.4",
            &[
                "set -sgq extended-keys on",
                "set -gq allow-passthrough on",
                "set -sgq terminal-features xterm*:RGB:clipboard:extkeys:focus:title",
            ],
            &["extended-keys-format"],
        ),
        (
            "tmux 3.5",
            &[
                "set -sgq extended-keys on",
                "set -sgq extended-keys-format csi-u",
                "set -gq allow-passthrough on",
            ],
            &["terminal-overrides"],
        ),
        (
            "tmux next-3.6",
            &[
                "set -sgq extended-keys-format csi-u",
                "set -gq allow-passthrough on",
            ],
            &["terminal-overrides"],
        ),
        (
            "tmux 3.7c",
            &["set -sgq extended-keys-format csi-u"],
            &["terminal-overrides"],
        ),
        (
            "tmux master",
            &[
                "set -sgq extended-keys-format csi-u",
                "set -gq allow-passthrough on",
            ],
            &["terminal-overrides"],
        ),
    ];
    for (version, lleva, no_lleva) in casos {
        let entorno = Entorno::nuevo(Some(FAKE_TMUX));
        let salida = entorno.ejecutar_con(
            &["crear", "agente", "", "", "7000", "80", "24"],
            &[("MAGI_TMUX_VERSION", version)],
        );
        assert_eq!(salida.codigo, 0, "{version}: {}", salida.stderr);
        assert!(
            salida.stdout.ends_with("MAGI_RESULTADO=creada\n"),
            "{version}: {}",
            salida.stdout
        );
        // La existencia se comprueba antes; la configuración y la creación van
        // juntas, en una sola invocación.
        let propias: Vec<String> = lineas_del_propio(&entorno)
            .into_iter()
            .filter(|linea| linea.contains("start-server"))
            .collect();
        assert_eq!(propias.len(), 1, "{version}: una sola invocación al propio");
        let invocacion = &propias[0];
        for opcion in comunes.iter().chain(lleva.iter()) {
            assert!(
                invocacion.contains(opcion),
                "{version}: falta «{opcion}» en {invocacion}"
            );
        }
        for opcion in no_lleva {
            assert!(
                !invocacion.contains(opcion),
                "{version}: sobra «{opcion}» en {invocacion}"
            );
        }
        assert!(entorno.magi_tiene("agente"), "{version}");
    }
}

#[test]
fn crear_aplica_la_configuracion_antes_de_crear_en_una_sola_invocacion() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    let salida = entorno.ejecutar(&["crear", "agente", "", "", "10000", "120", "40"]);
    assert_eq!(salida.codigo, 0);
    assert_eq!(salida.stdout, "MAGI_TMUX=tmux 3.4\nMAGI_RESULTADO=creada\n");
    assert_eq!(
        leer_crear(&salida.stdout, Some(0)).resultado,
        ResultadoCrear::Creada
    );
    let invocaciones: Vec<Vec<String>> = entorno
        .invocaciones()
        .into_iter()
        .filter(|palabras| palabras.iter().any(|palabra| palabra == "start-server"))
        .collect();
    assert_eq!(invocaciones.len(), 1);
    let palabras = &invocaciones[0];
    assert_eq!(
        &palabras[..5],
        ["-L", "magi", "-f", "/dev/null", "start-server"]
    );
    let posicion = |buscada: &str| palabras.iter().position(|palabra| palabra == buscada);
    let nueva = posicion("new-session").expect("new-session");
    let primer_set = posicion("set").expect("set");
    let limite = palabras
        .windows(2)
        .position(|par| par[0] == "history-limit" && par[1] == "10000")
        .expect("history-limit");
    assert!(posicion("start-server").unwrap() < primer_set);
    assert!(limite < nueva, "la configuración va antes de new-session");
    // Cada comando de la secuencia va separado por «;» suelto.
    assert!(
        palabras[..nueva]
            .iter()
            .filter(|palabra| *palabra == ";")
            .count()
            >= 10
    );
    assert_eq!(palabras[nueva - 1], ";");
    assert_eq!(
        &palabras[nueva..],
        ["new-session", "-d", "-s", "agente", "-x", "120", "-y", "40"]
    );
    // Nunca se carga la configuración del usuario.
    assert!(!entorno.llamadas().contains(".tmux.conf"));
}

#[test]
fn crear_con_comando_usa_el_envoltorio_y_pasa_el_comando_como_dato() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    let comando = "claude --continue; touch /tmp/pwn $(id) 'x' \"y\" `z`";
    let salida = entorno.ejecutar(&["crear", "agente", "", comando, "5000", "80", "24"]);
    assert_eq!(salida.codigo, 0, "{}", salida.stderr);
    let invocaciones = entorno.invocaciones();
    let palabras = invocaciones
        .iter()
        .find(|palabras| palabras.iter().any(|palabra| palabra == "new-session"))
        .expect("new-session");
    let desde = palabras
        .iter()
        .position(|palabra| palabra == "--")
        .expect("«--» antes del comando");
    assert_eq!(palabras[desde + 1], "sh");
    assert_eq!(palabras[desde + 2], "-c");
    let envoltorio = &palabras[desde + 3];
    assert_eq!(palabras[desde + 4], "magi-sesion");
    // El comando va entero como último argumento y no dentro del envoltorio.
    assert_eq!(palabras[desde + 5], comando);
    assert_eq!(palabras.len(), desde + 6);
    assert!(!envoltorio.contains("claude"));
    assert!(!envoltorio.contains("touch"));
    assert!(envoltorio.contains(r#""${SHELL:-/bin/sh}" -l -i -c "$1""#));
    assert!(envoltorio.contains("[MAGI] %s terminó con código %s"));
    assert!(envoltorio.contains(r#"exec "${SHELL:-/bin/sh}" -l"#));
}

#[test]
fn crear_sin_comando_ni_carpeta_no_pasa_ni_envoltorio_ni_directorio() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.ejecutar(&["crear", "agente", "", "", "5000", "80", "24"]);
    let invocaciones = entorno.invocaciones();
    let palabras = invocaciones.last().expect("invocación");
    assert!(!palabras.iter().any(|palabra| palabra == "--"));
    assert!(!palabras.iter().any(|palabra| palabra == "-c"));
    assert!(!palabras.iter().any(|palabra| palabra == "sh"));
}

#[test]
fn crear_cambia_la_tilde_por_home_sin_eval_y_comprueba_la_carpeta() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    fs::create_dir_all(entorno.home.join("proyectos/api")).unwrap();
    let salida = entorno.ejecutar(&["crear", "agente", "~/proyectos/api", "", "5000", "80", "24"]);
    assert_eq!(salida.codigo, 0, "{}", salida.stderr);
    let invocaciones = entorno.invocaciones();
    let palabras = invocaciones.last().unwrap();
    let dir = palabras
        .iter()
        .position(|palabra| palabra == "-c")
        .expect("-c");
    assert_eq!(
        palabras[dir + 1],
        format!("{}/proyectos/api", entorno.home.display())
    );

    // La carpeta absoluta pasa tal cual.
    let absoluta = entorno.home.join("otra");
    fs::create_dir_all(&absoluta).unwrap();
    let absoluta_texto = absoluta.display().to_string();
    let salida = entorno.ejecutar(&["crear", "otra", &absoluta_texto, "", "5000", "80", "24"]);
    assert_eq!(salida.codigo, 0);
    let invocaciones = entorno.invocaciones();
    let palabras = invocaciones.last().unwrap();
    let dir = palabras.iter().position(|palabra| palabra == "-c").unwrap();
    assert_eq!(palabras[dir + 1], absoluta_texto);

    // Con espacios y comillas no hay interpolación.
    let rara = entorno.home.join("con espacio $(x) 'q'");
    fs::create_dir_all(&rara).unwrap();
    let rara_texto = rara.display().to_string();
    let salida = entorno.ejecutar(&["crear", "rara", &rara_texto, "", "5000", "80", "24"]);
    assert_eq!(salida.codigo, 0, "{}", salida.stderr);
    let invocaciones = entorno.invocaciones();
    let palabras = invocaciones.last().unwrap();
    let dir = palabras.iter().position(|palabra| palabra == "-c").unwrap();
    assert_eq!(palabras[dir + 1], rara_texto);
}

/// tmux toma un argumento que acaba en «;» como el fin de la orden y se lo
/// quita («\;» pasa a «;»): con él, `find … -exec … \;` perdía la barra. El
/// tmux falso solo conoce el «;» suelto, así que se comprueba lo que recibe.
#[test]
fn ningun_argumento_de_tmux_acaba_en_punto_y_coma_salvo_el_separador() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    let con_punto_y_coma = entorno.home.join("datos;");
    fs::create_dir_all(&con_punto_y_coma).unwrap();
    for (indice, comando) in [
        "find . -exec ls {} \\;",
        "echo hola;",
        ";",
        "sleep 1 ; echo fin",
    ]
    .into_iter()
    .enumerate()
    {
        let nombre = format!("pc-{indice}");
        let salida = entorno.ejecutar(&["crear", &nombre, "~/datos;", comando, "5000", "80", "24"]);
        assert_eq!(salida.codigo, 0, "{comando}: {}", salida.stderr);
        let invocaciones = entorno.invocaciones();
        let palabras = invocaciones.last().unwrap();
        for palabra in palabras {
            assert!(
                palabra == ";" || !palabra.ends_with(';'),
                "{comando}: tmux vería el fin de la orden en {palabra:?}"
            );
        }
        // El comando llega entero (con un espacio detrás si acababa en «;») y
        // la carpeta sigue siendo la misma, con una barra detrás.
        let dato = palabras
            .iter()
            .position(|palabra| palabra == "magi-sesion")
            .expect("el comando va tras magi-sesion");
        assert!(palabras[dato + 1].starts_with(comando), "{comando}");
        assert_eq!(palabras[dato + 1].trim_end(), comando.trim_end());
        let dir = palabras.iter().position(|palabra| palabra == "-c").unwrap();
        assert_eq!(
            palabras[dir + 1],
            format!("{}/datos;/", entorno.home.display())
        );
    }
}

/// tmux expande formatos en `-c` (`#D`, `##` y `#(orden)`): la carpeta, que es
/// texto del usuario, llega con cada `#` duplicado.
#[test]
fn la_carpeta_con_almohadillas_va_escapada_para_los_formatos_de_tmux() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    let rara = entorno.home.join("#Docs a##b #(x) c#");
    fs::create_dir_all(&rara).unwrap();
    let rara_texto = rara.display().to_string();
    let salida = entorno.ejecutar(&["crear", "rara", &rara_texto, "", "5000", "80", "24"]);
    assert_eq!(salida.codigo, 0, "{}", salida.stderr);
    let invocaciones = entorno.invocaciones();
    let palabras = invocaciones.last().unwrap();
    let dir = palabras.iter().position(|palabra| palabra == "-c").unwrap();
    assert_eq!(palabras[dir + 1], rara_texto.replace('#', "##"));
    // Sin almohadillas, la carpeta pasa igual que llegó.
    let normal = entorno.home.join("normal");
    fs::create_dir_all(&normal).unwrap();
    let normal_texto = normal.display().to_string();
    let salida = entorno.ejecutar(&["crear", "normal", &normal_texto, "", "5000", "80", "24"]);
    assert_eq!(salida.codigo, 0, "{}", salida.stderr);
    let invocaciones = entorno.invocaciones();
    let palabras = invocaciones.last().unwrap();
    let dir = palabras.iter().position(|palabra| palabra == "-c").unwrap();
    assert_eq!(palabras[dir + 1], normal_texto);
}

/// Sin `-u`, tmux cambia por «_» todo lo que no es ASCII imprimible en lo que
/// escribe cuando el entorno del host no dice UTF-8 (LANG sin poner en un
/// canal exec de ssh): el tabulador y el 0x1F de `estado` incluidos.
#[test]
fn todo_tmux_va_en_modo_utf8() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["agente", "otra"]);
    entorno.sembrar_sesiones(&[("web", 0, 1, "2026-11-09 19:00:00")]);
    let llamadas: &[&[&str]] = &[
        &["comprobar", "magi", "agente"],
        &["comprobar", "habitual", "web"],
        &["crear", "nueva", "", "", "5000", "80", "24"],
        &["preparar", "magi", "agente"],
        &["preparar", "habitual", "web"],
        &["estado"],
        &["vista", "magi", "agente", "10"],
        &["historial", "habitual", "web", "10"],
        &["renombrar", "otra", "otra-2"],
        &["terminar", "magi", "agente"],
        &["terminar", "habitual", "web"],
        &["listar"],
    ];
    // El enganche mínimo, con tmux también siempre en modo UTF-8 (antes de que
    // las demás órdenes terminen las sesiones).
    entorno.enganche(&["magi", "agente", "1", "portatil", "0"]);
    entorno.enganche(&["habitual", "web", "0", "portatil", "1"]);
    for argumentos in llamadas {
        entorno.ejecutar(argumentos);
    }
    let invocaciones = entorno.invocaciones_crudas();
    assert!(invocaciones.len() > llamadas.len());
    for palabras in invocaciones {
        if palabras == ["-V"] {
            continue;
        }
        assert_eq!(
            palabras.first().map(String::as_str),
            Some("-u"),
            "tmux sin -u: {palabras:?}"
        );
    }
}

#[test]
fn crear_con_carpeta_inexistente_da_sin_carpeta() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    for carpeta in ["/no/existe/nunca", "~/no-existe"] {
        let salida = entorno.ejecutar(&["crear", "agente", carpeta, "", "5000", "80", "24"]);
        assert_eq!(salida.codigo, 72, "{carpeta}");
        assert!(salida.stdout.ends_with("MAGI_RESULTADO=sin_carpeta\n"));
        assert_eq!(
            leer_crear(&salida.stdout, Some(salida.codigo)).resultado,
            ResultadoCrear::SinCarpeta
        );
    }
    // Un fichero no es una carpeta.
    fs::write(entorno.home.join("fichero"), "x").unwrap();
    let salida = entorno.ejecutar(&["crear", "agente", "~/fichero", "", "5000", "80", "24"]);
    assert_eq!(salida.codigo, 72);
    assert!(!entorno.magi_tiene("agente"));
    assert!(!entorno.llamadas().contains("new-session"));
}

#[test]
fn crear_sobre_una_sesion_viva_da_existe_y_no_la_toca() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["agente"]);
    let antes = entorno.estado_de_magi();
    let salida = entorno.ejecutar(&["crear", "agente", "", "", "5000", "80", "24"]);
    assert_eq!(salida.codigo, 73);
    assert!(salida.stdout.ends_with("MAGI_RESULTADO=existe\n"));
    assert_eq!(
        leer_crear(&salida.stdout, Some(73)).resultado,
        ResultadoCrear::Existe
    );
    assert_eq!(entorno.estado_de_magi(), antes);
    assert!(!entorno.llamadas().contains("new-session"));
    // Con magi-a-1 y la magi-a-10 viva: son nombres distintos.
    entorno.sembrar_magi(&["magi-a-10"]);
    let salida = entorno.ejecutar(&["crear", "magi-a-1", "", "", "5000", "80", "24"]);
    assert_eq!(salida.codigo, 0);
    assert!(entorno.magi_tiene("magi-a-1") && entorno.magi_tiene("magi-a-10"));
}

#[test]
fn crear_con_tmux_antiguo_o_sin_tmux() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    let salida = entorno.ejecutar_con(
        &["crear", "agente", "", "", "5000", "80", "24"],
        &[("MAGI_TMUX_VERSION", "tmux 2.9a")],
    );
    assert_eq!(salida.codigo, 74);
    assert_eq!(
        salida.stdout,
        "MAGI_TMUX=tmux 2.9a\nMAGI_RESULTADO=antiguo\n"
    );
    let lectura = leer_crear(&salida.stdout, Some(74));
    assert_eq!(lectura.resultado, ResultadoCrear::Antiguo);
    assert_eq!(lectura.tmux, Some(VersionTmux::nueva(2, 9)));
    assert!(!entorno.llamadas().contains("new-session"));
    // 3.0 es el mínimo.
    let salida = entorno.ejecutar_con(
        &["crear", "agente", "", "", "5000", "80", "24"],
        &[("MAGI_TMUX_VERSION", "tmux 3.0a")],
    );
    assert_eq!(salida.codigo, 0);

    let sin = Entorno::nuevo(None);
    let salida = sin.ejecutar(&["crear", "agente", "", "", "5000", "80", "24"]);
    assert_eq!(salida.codigo, 76);
    assert_eq!(salida.stdout, "MAGI_TMUX=no\nMAGI_RESULTADO=sin_tmux\n");
    assert_eq!(
        leer_crear(&salida.stdout, Some(76)).resultado,
        ResultadoCrear::SinTmux
    );
}

#[test]
fn crear_que_falla_en_tmux_da_error() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    let salida = entorno.ejecutar_con(
        &["crear", "agente", "", "", "5000", "80", "24"],
        &[("MAGI_TMUX_FALLA", "new-session")],
    );
    assert_eq!(salida.codigo, 1);
    assert!(salida.stdout.ends_with("MAGI_RESULTADO=error\n"));
    assert_eq!(
        leer_crear(&salida.stdout, Some(1)).resultado,
        ResultadoCrear::Error
    );
}

#[test]
fn crear_valida_todos_los_argumentos_sin_tocar_tmux() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    let largo_carpeta = format!("/{}", "a".repeat(1024));
    let largo_comando = "c".repeat(4097);
    let casos: Vec<[&str; 6]> = vec![
        ["mal nombre", "", "", "5000", "80", "24"],
        ["", "", "", "5000", "80", "24"],
        ["a", "relativa/carpeta", "", "5000", "80", "24"],
        ["a", "~otro/carpeta", "", "5000", "80", "24"],
        ["a", "/con\nsalto", "", "5000", "80", "24"],
        ["a", "/con\rsalto", "", "5000", "80", "24"],
        ["a", largo_carpeta.as_str(), "", "5000", "80", "24"],
        ["a", "", largo_comando.as_str(), "5000", "80", "24"],
        ["a", "", "", "mucho", "80", "24"],
        ["a", "", "", "-1", "80", "24"],
        ["a", "", "", "1000001", "80", "24"],
        ["a", "", "", "5000", "1", "24"],
        ["a", "", "", "5000", "1001", "24"],
        ["a", "", "", "5000", "ancho", "24"],
        ["a", "", "", "5000", "80", "0"],
        ["a", "", "", "5000", "80", "2001"],
        ["a", "", "", "5000", "080", "24"],
    ];
    for caso in casos {
        let mut argumentos = vec!["crear"];
        argumentos.extend(caso);
        let salida = entorno.ejecutar(&argumentos);
        assert_eq!(salida.codigo, 64, "caso rechazado: {caso:?}");
        assert!(salida.stderr.contains("uso:"), "caso rechazado: {caso:?}");
        assert!(salida.stdout.is_empty(), "caso rechazado: {caso:?}");
    }
    // Argumentos de más o de menos.
    assert_eq!(
        entorno
            .ejecutar(&["crear", "a", "", "", "5000", "80"])
            .codigo,
        64
    );
    assert_eq!(
        entorno
            .ejecutar(&["crear", "a", "", "", "5000", "80", "24", "extra"])
            .codigo,
        64
    );
    assert!(entorno.llamadas().is_empty());
}

#[test]
fn crear_acepta_los_limites_justos() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    // Carpeta de 1024 bytes que no existe: valida, pero «sin_carpeta».
    let justa = format!("/{}", "a".repeat(1023));
    let salida = entorno.ejecutar(&["crear", "a", &justa, "", "0", "2", "1"]);
    assert_eq!(salida.codigo, 72);
    // Comando de 4096 bytes, columnas y filas en los extremos.
    let comando = "c".repeat(4096);
    let salida = entorno.ejecutar(&["crear", "a", "", &comando, "1000000", "1000", "2000"]);
    assert_eq!(salida.codigo, 0, "{}", salida.stderr);
    let invocaciones = entorno.invocaciones();
    let palabras = invocaciones.last().unwrap();
    assert_eq!(palabras.last().map(String::len), Some(4096));
}

// ------------------------------------------------- preparar y enganchar
//
// Engancharse son dos pasos (D357): `preparar`, por la entrada estándar
// (comprobar que la sesión existe, la configuración de MAGI y la identidad de
// la sesión), y el enganche mínimo con PTY (`terminal_enganche.sh`: la marca
// `@magi_equipo`, el `attach-session` y cómo acabó).

/// La sesión de siempre de estas pruebas con su identidad completa: sin
/// clientes, sin equipo y creada a las 1794845000.
const SESION_CON_IDENTIDAD: &str =
    "agente\t0\t1\t2026-11-09 19:00:00\t\t1794845343\t0\t\t1794845000";

#[test]
fn preparar_aplica_la_configuracion_y_da_la_identidad_sin_engancharse() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_lineas(&entorno.estado_magi, &[SESION_CON_IDENTIDAD.to_string()]);
    let salida = entorno.ejecutar(&["preparar", "magi", "agente"]);
    assert_eq!(salida.codigo, 0, "{}", salida.stderr);
    // El identificador interno y la hora de creación, con los que MAGI sigue la
    // sesión aunque otro equipo la renombre.
    assert_eq!(salida.stdout, "MAGI_ID=$0\nMAGI_CREADA=1794845000\n");
    // La lista exacta (existe), la identidad y la configuración. Ni la marca del
    // equipo ni el enganche: eso es del enganche mínimo.
    let propias = lineas_del_propio(&entorno);
    assert_eq!(propias.len(), 3, "{propias:?}");
    assert!(propias[0].ends_with("list-sessions -F #{session_name}"));
    assert!(
        propias[1].ends_with("list-sessions -F #{session_name}\t#{session_id}\t#{session_created}")
    );
    assert!(propias[2].contains("set -gq status off"));
    assert!(propias[2].contains("set -sgq extended-keys on"));
    // Idempotente y sin start-server ni history-limit.
    assert!(!propias[2].contains("start-server"));
    assert!(!propias[2].contains("history-limit"));
    assert!(!propias[2].contains("terminal-overrides"));
    let llamadas = entorno.llamadas();
    assert!(!llamadas.contains("attach-session"), "{llamadas}");
    assert!(!llamadas.contains("@magi_equipo"), "{llamadas}");
    // La sesión quedó como estaba: sin equipo.
    let linea = entorno
        .estado_de_magi()
        .lines()
        .find(|linea| linea.starts_with("agente\t"))
        .unwrap()
        .to_string();
    assert_eq!(linea.split('\t').nth(4), Some(""));
}

#[test]
fn preparar_sale_con_75_si_la_sesion_no_existia_y_no_toca_nada() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["magi-a-10"]);
    // Ni siquiera «magi-a-1» casa por prefijo con «magi-a-10» (R146).
    let salida = entorno.ejecutar(&["preparar", "magi", "magi-a-1"]);
    assert_eq!(salida.codigo, 75, "{}", salida.stderr);
    assert!(salida.stderr.contains("magi-a-1"));
    assert!(salida.stdout.is_empty());
    // Solo la versión y la lista exacta: ni configuración ni enganche.
    assert_eq!(
        entorno.llamadas().lines().collect::<Vec<&str>>(),
        vec![
            "-V",
            "-L magi -f /dev/null list-sessions -F #{session_name}"
        ]
    );
}

#[test]
fn preparar_exige_tmux_y_3_0_para_el_propio_y_no_toca_la_configuracion_del_habitual() {
    let sin = Entorno::nuevo(None);
    let salida = sin.ejecutar(&["preparar", "magi", "agente"]);
    assert_eq!(salida.codigo, 76);
    assert!(salida.stderr.contains("instala tmux en el host"));

    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["agente"]);
    entorno.sembrar_sesiones(&[("web", 0, 1, "2026-11-09 19:00:00")]);
    let viejo = [("MAGI_TMUX_VERSION", "tmux 2.9a")];
    let salida = entorno.ejecutar_con(&["preparar", "magi", "agente"], &viejo);
    assert_eq!(salida.codigo, 74);
    assert!(salida
        .stderr
        .contains("tmux demasiado antiguo (3.0 o posterior)"));
    // El habitual solo pide 1.8, y a él no se le toca ni la configuración ni
    // las opciones.
    let antes = entorno.llamadas().lines().count();
    let salida = entorno.ejecutar_con(&["preparar", "habitual", "web"], &viejo);
    assert_eq!(salida.codigo, 0, "{}", salida.stderr);
    assert!(salida.stdout.starts_with("MAGI_ID=$0\n"));
    let llamadas = entorno.llamadas();
    let nuevas: Vec<&str> = llamadas.lines().skip(antes).collect();
    assert!(nuevas.iter().all(|linea| !linea.contains("-L magi")));
    assert!(nuevas.iter().all(|linea| !linea.contains("set")));
    assert!(entorno.estado_de_magi().contains("agente"));
}

#[test]
fn preparar_valida_sus_argumentos() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    for caso in [
        &["preparar", "otro", "agente"][..],
        &["preparar", "magi", "mal nombre"],
        &["preparar", "magi", ""],
        &["preparar", "magi"],
        &["preparar", "magi", "agente", "de-mas"],
    ] {
        let salida = entorno.ejecutar(caso);
        assert_eq!(salida.codigo, 64, "caso rechazado: {caso:?}");
    }
    // Las subórdenes antiguas de la Fase 16 ya no existen en el script grande.
    assert_eq!(
        entorno
            .ejecutar(&["enganchar", "magi", "agente", "1", "portatil"])
            .codigo,
        64
    );
    assert!(entorno.llamadas().is_empty());
}

#[test]
fn el_enganche_suelta_pone_el_equipo_y_engancha() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["agente"]);
    let salida = entorno.enganche(&["magi", "agente", "1", "portatil", "0"]);
    // Con la entrada cerrada, el tmux falso vuelve y la sesión sigue viva:
    // a este cliente lo soltaron.
    assert_eq!(salida.codigo, 100, "{}", salida.stderr);
    assert_eq!(fin_de_enganchar(salida.codigo), FinEnganchar::Soltada);
    assert!(salida.stdout.contains("MAGI_TMUX_ADJUNTO=agente soltar=1"));
    // La lista exacta (existe), su identificador interno, el equipo, el
    // enganche y, al volver, si el identificador sigue vivo (no el nombre: un
    // renombrado no la hace «terminada»). Sin configuración: eso fue de
    // `preparar`.
    let propias = lineas_del_propio(&entorno);
    assert_eq!(propias.len(), 5, "{propias:?}");
    assert!(propias[0].ends_with("list-sessions -F #{session_name}"));
    assert!(propias[1].ends_with("list-sessions -F #{session_id}\t#{session_name}"));
    assert!(propias[2].ends_with("set-option -q -t =agente: @magi_equipo portatil"));
    assert!(propias[3].ends_with("attach-session -d -t =agente"));
    assert!(propias[4].ends_with("list-sessions -F #{session_id}"));
    assert!(!entorno.llamadas().contains("set -gq"));
    // @magi_equipo quedó en la sesión.
    let linea = entorno
        .estado_de_magi()
        .lines()
        .find(|linea| linea.starts_with("agente\t"))
        .unwrap()
        .to_string();
    assert_eq!(linea.split('\t').nth(4), Some("portatil"));
}

#[test]
fn el_enganche_al_compartir_no_suelta_a_los_demas() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["agente"]);
    let salida = entorno.enganche(&["magi", "agente", "0", "web-01", "0"]);
    assert_eq!(salida.codigo, 100);
    assert!(salida.stdout.contains("soltar=0"));
    let propias = lineas_del_propio(&entorno);
    assert!(propias
        .iter()
        .any(|linea| linea.ends_with("attach-session -t =agente")));
    assert!(!propias
        .iter()
        .any(|linea| linea.contains("attach-session -d")));
}

#[test]
fn el_enganche_sale_con_0_si_la_sesion_termino_y_con_75_si_no_existia() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["agente"]);
    // El programa de la sesión termina mientras se está enganchado.
    let salida = entorno.enganche_con(
        &["magi", "agente", "1", "portatil", "0"],
        &[("MAGI_TMUX_ATTACH_MATA", "1")],
    );
    assert_eq!(salida.codigo, 0, "{}", salida.stderr);
    assert_eq!(fin_de_enganchar(salida.codigo), FinEnganchar::Terminada);
    assert!(!entorno.magi_tiene("agente"));

    // No existía: ni se pone la marca ni se engancha.
    let antes = entorno.llamadas().lines().count();
    let salida = entorno.enganche(&["magi", "fantasma", "1", "portatil", "0"]);
    assert_eq!(salida.codigo, 75);
    assert_eq!(fin_de_enganchar(salida.codigo), FinEnganchar::NoExiste);
    assert!(salida.stderr.contains("fantasma"));
    assert!(salida.stdout.is_empty());
    // Solo la versión y la lista exacta.
    let llamadas = entorno.llamadas();
    let nuevas: Vec<&str> = llamadas.lines().skip(antes).collect();
    assert_eq!(
        nuevas,
        vec![
            "-V",
            "-L magi -f /dev/null list-sessions -F #{session_name}"
        ]
    );
}

#[test]
fn el_enganche_a_magi_a_1_no_se_engancha_a_magi_a_10() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["magi-a-10"]);
    let salida = entorno.enganche(&["magi", "magi-a-1", "1", "portatil", "0"]);
    assert_eq!(salida.codigo, 75);
    assert!(!entorno.llamadas().contains("attach-session"));
    assert!(!entorno.llamadas().contains("@magi_equipo"));
}

#[test]
fn el_enganche_que_no_puede_engancharse_devuelve_el_codigo_de_tmux() {
    // Sin terminal, el cliente de tmux sale con error y la sesión sigue viva:
    // no es «soltada».
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["agente"]);
    let salida = entorno.enganche_con(
        &["magi", "agente", "1", "portatil", "0"],
        &[("MAGI_TMUX_ATTACH_ESTADO", "1")],
    );
    assert_eq!(salida.codigo, 1);
    assert_eq!(fin_de_enganchar(salida.codigo), FinEnganchar::Error(1));
}

#[test]
fn el_enganche_al_habitual_no_toca_su_configuracion_ni_sus_opciones() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_sesiones(&[("web", 0, 1, "2026-11-09 19:00:00")]);
    let salida = entorno.enganchar("habitual", "web", "1", "portatil");
    assert_eq!(salida.codigo, 100, "{}", salida.stderr);
    let llamadas = entorno.llamadas();
    assert!(!llamadas.contains("-L magi"));
    assert!(!llamadas.contains("set"));
    assert!(!llamadas.contains("@magi_equipo"));
    assert!(llamadas.contains("attach-session -d -t =web"));
    // La sesión de MAGI no se creó: es otro servidor.
    assert!(entorno.estado_de_magi().is_empty());
}

#[test]
fn el_enganche_exige_tmux_y_3_0_para_el_propio() {
    let sin = Entorno::nuevo(None);
    let salida = sin.enganche(&["magi", "agente", "1", "portatil", "0"]);
    assert_eq!(salida.codigo, 76);
    assert_eq!(fin_de_enganchar(salida.codigo), FinEnganchar::SinTmux);
    assert!(salida.stderr.contains("instala tmux en el host"));

    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["agente"]);
    entorno.sembrar_sesiones(&[("web", 0, 1, "2026-11-09 19:00:00")]);
    let viejo = [("MAGI_TMUX_VERSION", "tmux 2.9a")];
    let salida = entorno.enganche_con(&["magi", "agente", "1", "portatil", "0"], &viejo);
    assert_eq!(salida.codigo, 74);
    assert_eq!(fin_de_enganchar(salida.codigo), FinEnganchar::Antiguo);
    assert!(salida
        .stderr
        .contains("tmux demasiado antiguo (3.0 o posterior)"));
    assert!(!entorno.llamadas().contains("attach-session"));
    // El habitual solo pide 1.8.
    let salida = entorno.enganche_con(&["habitual", "web", "1", "portatil", "0"], &viejo);
    assert_eq!(salida.codigo, 100);
}

#[test]
fn el_enganche_valida_sus_argumentos() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    let casos: [[&str; 5]; 9] = [
        ["otro", "agente", "1", "portatil", "0"],
        ["magi", "mal nombre", "1", "portatil", "0"],
        ["magi", "agente", "2", "portatil", "0"],
        ["magi", "agente", "", "portatil", "0"],
        ["magi", "agente", "1", "Portatil", "0"],
        ["magi", "agente", "1", "", "0"],
        ["magi", "agente", "1", "equipo-demasiado-largo-x", "0"],
        ["magi", "agente", "1", "portatil", "2"],
        ["magi", "agente", "1", "portatil", ""],
    ];
    for caso in casos {
        let salida = entorno.enganche(&caso);
        assert_eq!(salida.codigo, 64, "caso rechazado: {caso:?}");
        assert!(salida.stderr.contains("uso:"), "{caso:?}");
    }
    // De menos y de más.
    assert_eq!(
        entorno
            .enganche(&["magi", "agente", "1", "portatil"])
            .codigo,
        64
    );
    assert_eq!(
        entorno
            .enganche(&["magi", "agente", "1", "portatil", "0", "de-mas"])
            .codigo,
        64
    );
    assert!(entorno.llamadas().is_empty());
}

#[test]
fn los_scripts_quitan_la_variable_tmux_para_no_anidar() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["agente"]);
    let ajena = [("TMUX", "/tmp/otro-tmux,123,0")];
    let salida = entorno.ejecutar_con(&["preparar", "magi", "agente"], &ajena);
    assert_eq!(salida.codigo, 0, "{}", salida.stderr);
    let salida = entorno.enganche_con(&["magi", "agente", "1", "portatil", "0"], &ajena);
    assert_eq!(salida.codigo, 100);
    let visto = fs::read_to_string(&entorno.entorno_tmux).unwrap();
    assert!(visto.lines().count() >= 6);
    assert!(visto.lines().all(|linea| linea == "TMUX="), "{visto}");
}

// La guarda de la reconexión automática (R157): solo se engancha si la sesión
// está libre o es de este equipo.

#[test]
fn con_guarda_una_sesion_de_otro_equipo_no_se_toca_y_sale_con_101() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    // Un cliente de otro equipo la tiene enganchada ahora.
    entorno.sembrar_lineas(
        &entorno.estado_magi,
        &["agente\t1\t1\t2026-11-09 19:00:00\tsobremesa".to_string()],
    );
    let salida = entorno.enganche(&["magi", "agente", "1", "portatil", "1"]);
    assert_eq!(salida.codigo, 101, "{}", salida.stderr);
    assert_eq!(fin_de_enganchar(salida.codigo), FinEnganchar::OtroEquipo);
    assert!(salida.stderr.contains("otro equipo"));
    // Ni la marca ni el enganche: el otro equipo conserva la sesión, con su
    // cliente y su marca.
    let llamadas = entorno.llamadas();
    assert!(!llamadas.contains("attach-session"), "{llamadas}");
    // (La guarda lee `#{@magi_equipo}` de la lista, pero no escribe la marca.)
    assert!(!llamadas.contains("set-option"), "{llamadas}");
    let linea = entorno.estado_de_magi();
    assert_eq!(linea.lines().next().unwrap().split('\t').nth(1), Some("1"));
    assert_eq!(
        linea.lines().next().unwrap().split('\t').nth(4),
        Some("sobremesa")
    );
}

#[test]
fn con_guarda_una_sesion_con_clientes_sin_marca_tambien_es_de_otro() {
    // Un cliente que no es de MAGI (o de una versión sin marca): no se sabe de
    // quién es, así que no se le quita.
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_lineas(
        &entorno.estado_magi,
        &["agente\t2\t1\t2026-11-09 19:00:00".to_string()],
    );
    let salida = entorno.enganche(&["magi", "agente", "1", "portatil", "1"]);
    assert_eq!(salida.codigo, 101, "{}", salida.stderr);
    assert!(!entorno.llamadas().contains("attach-session"));
}

#[test]
fn con_guarda_una_sesion_libre_o_de_este_equipo_se_engancha() {
    // Libre: sin clientes (aunque otro equipo la tuviera antes).
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_lineas(
        &entorno.estado_magi,
        &["agente\t0\t1\t2026-11-09 19:00:00\tsobremesa".to_string()],
    );
    let salida = entorno.enganche(&["magi", "agente", "1", "portatil", "1"]);
    assert_eq!(salida.codigo, 100, "{}", salida.stderr);
    assert!(entorno.llamadas().contains("attach-session -d -t =agente"));
    // La marca se puso después de la guarda.
    assert!(entorno
        .llamadas()
        .contains("set-option -q -t =agente: @magi_equipo portatil"));

    // De este equipo: los clientes son los de la conexión que se cayó.
    let propio = Entorno::nuevo(Some(FAKE_TMUX));
    propio.sembrar_lineas(
        &propio.estado_magi,
        &["agente\t1\t1\t2026-11-09 19:00:00\tportatil".to_string()],
    );
    let salida = propio.enganche(&["magi", "agente", "1", "portatil", "1"]);
    assert_eq!(salida.codigo, 100, "{}", salida.stderr);
    assert!(propio.llamadas().contains("attach-session -d -t =agente"));
}

#[test]
fn sin_guarda_el_enganche_se_lleva_la_sesion_aunque_la_tenga_otro() {
    // Es lo que hace el usuario a mano (prefijo r): manda él.
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_lineas(
        &entorno.estado_magi,
        &["agente\t1\t1\t2026-11-09 19:00:00\tsobremesa".to_string()],
    );
    let salida = entorno.enganche(&["magi", "agente", "1", "portatil", "0"]);
    assert_eq!(salida.codigo, 100, "{}", salida.stderr);
    assert!(entorno.llamadas().contains("attach-session -d -t =agente"));
}

#[test]
fn la_guarda_no_aplica_al_tmux_habitual() {
    // Al habitual no se le pone marca ni se le mira el equipo.
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_lineas(
        &entorno.estado,
        &["web\t1\t1\t2026-11-09 19:00:00\tsobremesa".to_string()],
    );
    let salida = entorno.enganche(&["habitual", "web", "0", "portatil", "1"]);
    assert_eq!(salida.codigo, 100, "{}", salida.stderr);
    assert!(entorno.llamadas().contains("attach-session -t =web"));
}

// ----------------------------------------------------------------- estado

#[test]
fn estado_lista_las_sesiones_de_los_dos_servidores() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    // nombre adheridos ventanas creada equipo actividad campana titulo creada_epoch
    entorno.sembrar_lineas(
        &entorno.estado_magi,
        &[
            "agente-api\t1\t3\t2026-11-09 19:00:00\tportatil\t1794845343\t1\tclaude · api\t1794845000"
                .to_string(),
            "reposo\t0\t1\t2026-11-09 19:00:00\t\t1794800000\t0\t\t1794799000".to_string(),
        ],
    );
    entorno.sembrar_lineas(
        &entorno.estado,
        &["web\t2\t1\t2026-11-09 19:00:00\t\t1794845300\t0\tbash\t1794845100".to_string()],
    );
    let salida = entorno.ejecutar(&["estado"]);
    assert_eq!(salida.codigo, 0, "{}", salida.stderr);
    let lineas: Vec<&str> = salida.stdout.lines().collect();
    assert_eq!(lineas[0], "MAGI_TMUX=tmux 3.4");
    assert!(lineas[1].starts_with("MAGI_AHORA="));
    let ahora: i64 = lineas[1]["MAGI_AHORA=".len()..].parse().expect("segundos");
    let reloj = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    assert!(
        (reloj - ahora).abs() < 120,
        "MAGI_AHORA es la hora del host"
    );
    // Primero el propio, luego el habitual; con 0x1F entre los campos.
    assert_eq!(
        lineas[2],
        linea_de_estado(&[
            "magi",
            "agente-api",
            "$0",
            "1",
            "portatil",
            "1794845343",
            "1794845000",
            "1",
            "3",
            "claude · api"
        ])
    );
    assert_eq!(
        lineas[3],
        linea_de_estado(&[
            "magi",
            "reposo",
            "$1",
            "0",
            "",
            "1794800000",
            "1794799000",
            "0",
            "1",
            ""
        ])
    );
    assert_eq!(
        lineas[4],
        linea_de_estado(&[
            "habitual",
            "web",
            "$0",
            "2",
            "",
            "1794845300",
            "1794845100",
            "0",
            "1",
            "bash"
        ])
    );
    assert_eq!(lineas.len(), 5);

    // El lector del núcleo entiende lo que imprime el script.
    let lectura = leer_estado(&salida.stdout).expect("legible");
    assert_eq!(lectura.tmux, Some(VersionTmux::nueva(3, 4)));
    assert_eq!(lectura.ahora, ahora);
    assert_eq!(lectura.sesiones.len(), 3);
    let agente = &lectura.sesiones[0];
    assert_eq!(agente.servidor, ServidorTmux::Magi);
    assert_eq!(agente.nombre, "agente-api");
    // El identificador interno de tmux, con el que MAGI la sigue si la renombran.
    assert_eq!(agente.id.as_deref(), Some("$0"));
    assert_eq!(agente.clientes, 1);
    assert_eq!(agente.equipo.as_deref(), Some("portatil"));
    // La actividad y la campana son las de la ventana más reciente.
    assert_eq!(agente.actividad, 1794845343);
    assert!(agente.campana);
    assert_eq!(agente.ventanas, 3);
    assert_eq!(agente.titulo.as_deref(), Some("claude · api"));
    assert!(!lectura.sesiones[1].campana);
    assert_eq!(lectura.sesiones[1].titulo, None);
    assert_eq!(lectura.sesiones[2].servidor, ServidorTmux::Habitual);

    // Las órdenes: una lista de sesiones y otra de ventanas por servidor.
    let llamadas = entorno.llamadas();
    assert!(llamadas.contains("-L magi -f /dev/null list-sessions -F"));
    assert!(llamadas.contains("-L magi -f /dev/null list-windows -a -F"));
    assert!(llamadas
        .lines()
        .any(|linea| linea.starts_with("list-windows -a -F")));
}

#[test]
fn estado_sin_tmux_o_sin_sesiones() {
    let sin = Entorno::nuevo(None);
    let salida = sin.ejecutar(&["estado"]);
    assert_eq!(salida.codigo, 0);
    assert_eq!(salida.stdout, "MAGI_TMUX=no\n");
    let lectura = leer_estado(&salida.stdout).unwrap();
    assert_eq!(lectura.tmux, None);

    // Con tmux pero sin servidores (el falso falla como «no server running»).
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    let salida = entorno.ejecutar(&["estado"]);
    assert_eq!(salida.codigo, 0);
    assert_eq!(salida.stdout.lines().count(), 2);
    let lectura = leer_estado(&salida.stdout).unwrap();
    assert!(lectura.sesiones.is_empty());
    assert!(lectura.ahora > 0);
}

#[test]
fn estado_con_un_tmux_que_falla_no_dice_que_no_hay_sesiones() {
    // Regresión: tras un `apt upgrade tmux` con sesiones vivas, el cliente
    // nuevo no habla con el servidor viejo («protocol version mismatch»). El
    // script salía con 0 y una lista vacía, y MAGI daba por terminadas las
    // sesiones vivas (la tecla `C` habría borrado sus fichas).
    for orden in ["list-sessions", "list-windows"] {
        let entorno = Entorno::nuevo(Some(FAKE_TMUX));
        entorno.sembrar_magi(&["agente"]);
        let salida = entorno.ejecutar_con(&["estado"], &[("MAGI_TMUX_FALLA", orden)]);
        assert_eq!(salida.codigo, 77, "{orden}: {}", salida.stderr);
        assert!(!salida.stdout.contains("MAGI_SES"), "{orden}");
        assert!(salida.stderr.contains("no pudo listar"), "{orden}");
    }
}

#[test]
fn estado_sanea_lo_que_viene_del_tmux_ajeno() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    let titulo_largo = "t".repeat(600);
    entorno.sembrar_lineas(
        &entorno.estado,
        &[
            // Nombres que MAGI no puede manejar: no salen.
            "con espacio\t0\t1\tx\t\t1\t0\ta\t1".to_string(),
            "a.b\t0\t1\tx\t\t1\t0\ta\t1".to_string(),
            "ñandú\t0\t1\tx\t\t1\t0\ta\t1".to_string(),
            // Controles en el título y en el equipo: se quitan.
            "controles\t0\t1\tx\tequi\u{7}po\t1\t0\t\u{1b}[31mrojo\u{7}\t1".to_string(),
            // Título larguísimo: se corta a 400 bytes.
            format!("largo\t0\t1\tx\t\t1\t0\t{titulo_largo}\t1"),
            // Campos ilegibles: se descartan.
            "roto\tmuchos\t1\tx\t\t1\t0\ta\t1".to_string(),
            "ok\t0\t1\tx\t\t1\t0\ta\t1".to_string(),
        ],
    );
    let salida = entorno.ejecutar(&["estado"]);
    let lineas: Vec<&str> = salida
        .stdout
        .lines()
        .filter(|linea| linea.starts_with("MAGI_SES"))
        .collect();
    let nombres: Vec<&str> = lineas
        .iter()
        .map(|linea| linea.split(US).nth(2).unwrap())
        .collect();
    assert_eq!(nombres, vec!["controles", "largo", "ok"]);
    let controles: Vec<&str> = lineas[0].split(US).collect();
    assert_eq!(controles[5], "equipo");
    assert_eq!(controles[10], "[31mrojo");
    let largo: Vec<&str> = lineas[1].split(US).collect();
    assert_eq!(largo[10].len(), 400);
    assert!(salida
        .stdout
        .bytes()
        .all(|byte| byte == b'\n' || byte == 0x1f || byte >= 0x20));
}

#[test]
fn estado_no_pasa_de_500_sesiones_por_servidor() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    let lineas: Vec<String> = (0..650)
        .map(|numero| format!("s-{numero}\t0\t1\tx\t\t1\t0\ta\t1"))
        .collect();
    entorno.sembrar_lineas(&entorno.estado_magi, &lineas);
    let salida = entorno.ejecutar(&["estado"]);
    assert_eq!(
        salida
            .stdout
            .lines()
            .filter(|linea| linea.starts_with("MAGI_SES"))
            .count(),
        500
    );
    let lectura = leer_estado(&salida.stdout).unwrap();
    assert_eq!(lectura.sesiones.len(), 500);
}

#[test]
fn estado_ignora_los_argumentos_de_mas() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    assert_eq!(entorno.ejecutar(&["estado", "magi"]).codigo, 64);
}

// ---------------------------------------------------------- vista, historial

#[test]
fn vista_da_las_ultimas_lineas_sin_espacios_finales_ni_vacias_al_final() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["agente"]);
    let salida = entorno.ejecutar(&["vista", "magi", "agente", "20"]);
    assert_eq!(salida.codigo, 0, "{}", salida.stderr);
    assert_eq!(salida.stdout, "linea uno\nlinea dos\n\nlinea tres\n");
    let salida = entorno.ejecutar(&["vista", "magi", "agente", "2"]);
    assert_eq!(salida.stdout, "\nlinea tres\n");
    let salida = entorno.ejecutar(&["vista", "magi", "agente", "1"]);
    assert_eq!(salida.stdout, "linea tres\n");
    // capture-pane con el panel de la sesión («=nombre:») y en texto plano.
    let propias = lineas_del_propio(&entorno);
    assert!(propias
        .iter()
        .any(|linea| linea.ends_with("capture-pane -p -J -t =agente:")));
    assert!(!entorno.llamadas().contains("capture-pane -p -e"));
}

#[test]
fn vista_y_historial_de_una_sesion_inexistente_dan_75() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["magi-a-10"]);
    for suborden in ["vista", "historial"] {
        let salida = entorno.ejecutar(&[suborden, "magi", "magi-a-1", "10"]);
        assert_eq!(salida.codigo, 75, "{suborden}");
        assert!(salida.stdout.is_empty(), "{suborden}");
    }
    assert!(!entorno.llamadas().contains("capture-pane"));
    let sin = Entorno::nuevo(None);
    assert_eq!(sin.ejecutar(&["vista", "magi", "a", "10"]).codigo, 76);
    assert_eq!(
        sin.ejecutar(&["historial", "habitual", "a", "10"]).codigo,
        76
    );
}

#[test]
fn vista_e_historial_con_una_captura_que_falla_no_dan_un_exito_vacio() {
    // Regresión: con la captura en una tubería, el código de salida era el de
    // awk o sed y una sesión que desaparecía a mitad salía con 0 y vacía.
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["agente"]);
    for suborden in ["vista", "historial"] {
        let salida = entorno.ejecutar_con(
            &[suborden, "magi", "agente", "10"],
            &[("MAGI_TMUX_FALLA", "capture-pane")],
        );
        assert_eq!(salida.codigo, 75, "{suborden}: {}", salida.stderr);
        assert!(salida.stdout.is_empty(), "{suborden}");
    }
}

#[test]
fn historial_pide_los_colores_y_las_lineas_anteriores() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["agente"]);
    fs::write(
        format!("{}.pantalla.agente", entorno.estado_magi.display()),
        "\u{1b}[32mverde\u{1b}[0m   \nsin color  \n",
    )
    .unwrap();
    let salida = entorno.ejecutar(&["historial", "magi", "agente", "2000"]);
    assert_eq!(salida.codigo, 0, "{}", salida.stderr);
    // Los colores se conservan y los espacios finales de cada línea no.
    assert_eq!(salida.stdout, "\u{1b}[32mverde\u{1b}[0m\nsin color\n");
    let propias = lineas_del_propio(&entorno);
    assert!(propias
        .iter()
        .any(|linea| linea.ends_with("capture-pane -p -e -J -S -2000 -t =agente:")));
    // Y con «habitual», el historial es del tmux habitual.
    entorno.sembrar_sesiones(&[("web", 0, 1, "x")]);
    let habitual = entorno.ejecutar(&["historial", "habitual", "web", "50"]);
    assert_eq!(habitual.codigo, 0);
    assert!(entorno
        .llamadas()
        .lines()
        .any(|linea| linea == "capture-pane -p -e -J -S -50 -t =web:"));
}

#[test]
fn vista_e_historial_validan_las_lineas() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    for suborden in ["vista", "historial"] {
        for lineas in ["0", "-1", "x", "", "100001", "007"] {
            let salida = entorno.ejecutar(&[suborden, "magi", "a", lineas]);
            assert_eq!(salida.codigo, 64, "{suborden} {lineas:?}");
        }
        assert_eq!(entorno.ejecutar(&[suborden, "otro", "a", "10"]).codigo, 64);
        assert_eq!(
            entorno.ejecutar(&[suborden, "magi", "a b", "10"]).codigo,
            64
        );
        assert_eq!(entorno.ejecutar(&[suborden, "magi", "a"]).codigo, 64);
    }
    assert!(entorno.llamadas().is_empty());
}

// --------------------------------------------------------------- renombrar

#[test]
fn renombrar_cambia_el_nombre_y_conserva_la_sesion() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_lineas(
        &entorno.estado_magi,
        &["agente\t1\t2\tx\tportatil\t5\t0\t\t5".to_string()],
    );
    let salida = entorno.ejecutar(&["renombrar", "agente", "revision"]);
    assert_eq!(salida.codigo, 0);
    assert_eq!(salida.stdout, "MAGI_RESULTADO=ok\n");
    assert_eq!(leer_renombrar(&salida.stdout), ResultadoRenombrar::Ok);
    assert!(entorno.magi_tiene("revision"));
    assert!(!entorno.magi_tiene("agente"));
    // Lo demás de la sesión sigue igual (equipo, clientes…).
    assert!(entorno
        .estado_de_magi()
        .starts_with("revision\t1\t2\tx\tportatil\t"));
    assert!(lineas_del_propio(&entorno)
        .iter()
        .any(|linea| linea.ends_with("rename-session -t =agente -- revision")));
}

#[test]
fn renombrar_a_un_nombre_ocupado_o_de_una_sesion_inexistente() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["magi-a-10", "otra"]);
    // Ocupado.
    let salida = entorno.ejecutar(&["renombrar", "magi-a-10", "otra"]);
    assert_eq!(salida.stdout, "MAGI_RESULTADO=ocupado\n");
    assert_eq!(leer_renombrar(&salida.stdout), ResultadoRenombrar::Ocupado);
    // El mismo nombre también está ocupado.
    let salida = entorno.ejecutar(&["renombrar", "otra", "otra"]);
    assert_eq!(salida.stdout, "MAGI_RESULTADO=ocupado\n");
    // magi-a-1 no existe (solo magi-a-10): no se renombra la otra.
    let salida = entorno.ejecutar(&["renombrar", "magi-a-1", "nueva"]);
    assert_eq!(salida.stdout, "MAGI_RESULTADO=no_existe\n");
    assert_eq!(leer_renombrar(&salida.stdout), ResultadoRenombrar::NoExiste);
    assert!(entorno.magi_tiene("magi-a-10") && !entorno.magi_tiene("nueva"));
    assert!(!entorno.llamadas().contains("rename-session"));
}

#[test]
fn renombrar_con_un_nombre_que_empieza_por_guion_no_es_una_opcion() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["agente"]);
    let salida = entorno.ejecutar(&["renombrar", "agente", "-x"]);
    assert_eq!(salida.stdout, "MAGI_RESULTADO=ok\n");
    assert!(entorno.magi_tiene("-x"));
}

#[test]
fn renombrar_sin_tmux_y_con_fallo_de_tmux() {
    let sin = Entorno::nuevo(None);
    let salida = sin.ejecutar(&["renombrar", "agente", "otra"]);
    assert_eq!(salida.codigo, 0);
    assert_eq!(salida.stdout, "MAGI_RESULTADO=no_existe\n");

    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["agente"]);
    let salida = entorno.ejecutar_con(
        &["renombrar", "agente", "otra"],
        &[("MAGI_TMUX_FALLA", "rename-session")],
    );
    assert_eq!(salida.stdout, "MAGI_RESULTADO=error\n");
    assert_eq!(leer_renombrar(&salida.stdout), ResultadoRenombrar::Error);
    assert!(entorno.magi_tiene("agente"));
}

#[test]
fn renombrar_valida_sus_argumentos() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    for caso in [
        ["mal nombre", "otra"],
        ["agente", "mal nombre"],
        ["agente", ""],
        ["", "otra"],
    ] {
        let salida = entorno.ejecutar(&["renombrar", caso[0], caso[1]]);
        assert_eq!(salida.codigo, 64, "caso rechazado: {caso:?}");
    }
    assert_eq!(entorno.ejecutar(&["renombrar", "agente"]).codigo, 64);
    assert!(entorno.llamadas().is_empty());
}

// ------------------------------------------------------------- Rust (S1)

#[test]
fn las_ordenes_nuevas_llevan_sus_datos_como_argumentos_sueltos() {
    let crear = OrdenTmux::Crear {
        nombre: "agente",
        carpeta: Some("~/proyectos/api"),
        comando: Some("claude --continue"),
        lineas_historial: 10_000,
        columnas: 120,
        filas: 40,
    };
    assert_eq!(
        crear.argumentos().unwrap(),
        vec![
            "sh",
            "-c",
            ARRANQUE,
            "magi-tmux",
            "crear",
            "agente",
            "~/proyectos/api",
            "claude --continue",
            "10000",
            "120",
            "40",
        ]
    );
    // Sin carpeta ni comando van vacíos.
    let simple = OrdenTmux::Crear {
        nombre: "agente",
        carpeta: None,
        comando: None,
        lineas_historial: 0,
        columnas: 2,
        filas: 1,
    };
    assert_eq!(
        &simple.argumentos().unwrap()[5..],
        ["agente", "", "", "0", "2", "1"]
    );

    let comprobar = OrdenTmux::Comprobar {
        servidor: ServidorTmux::Habitual,
        nombre: "web",
    };
    assert_eq!(
        &comprobar.argumentos().unwrap()[4..],
        ["comprobar", "habitual", "web"]
    );
    assert_eq!(&OrdenTmux::Estado.argumentos().unwrap()[4..], ["estado"]);
    let preparar = OrdenTmux::Preparar {
        servidor: ServidorTmux::Magi,
        nombre: "agente",
    };
    assert_eq!(
        &preparar.argumentos().unwrap()[4..],
        ["preparar", "magi", "agente"]
    );
    // El enganche con PTY lleva su script mínimo en la línea de órdenes y sus
    // datos detrás, sin suborden: es otro script.
    let enganchar = OrdenTmux::Enganchar {
        servidor: ServidorTmux::Magi,
        nombre: "agente",
        soltar: true,
        equipo: "portatil",
        guardar: true,
    };
    assert_eq!(
        enganchar.argumentos().unwrap(),
        vec![
            "sh",
            "-c",
            SCRIPT_ENGANCHE,
            "magi-enganche",
            "magi",
            "agente",
            "1",
            "portatil",
            "1",
        ]
    );
    let vista = OrdenTmux::Vista {
        servidor: ServidorTmux::Magi,
        nombre: "agente",
        lineas: 20,
    };
    assert_eq!(
        &vista.argumentos().unwrap()[4..],
        ["vista", "magi", "agente", "20"]
    );
    let historial = OrdenTmux::Historial {
        servidor: ServidorTmux::Habitual,
        nombre: "web",
        lineas: 2000,
    };
    assert_eq!(
        &historial.argumentos().unwrap()[4..],
        ["historial", "habitual", "web", "2000"]
    );
    let renombrar = OrdenTmux::Renombrar {
        nombre: "agente",
        nuevo: "revision",
    };
    assert_eq!(
        &renombrar.argumentos().unwrap()[4..],
        ["renombrar", "agente", "revision"]
    );
    let terminar = OrdenTmux::Terminar {
        servidor: ServidorTmux::Magi,
        nombre: "agente",
    };
    assert_eq!(
        &terminar.argumentos().unwrap()[4..],
        ["terminar", "magi", "agente"]
    );
}

fn terminar_con(nombre: &str) -> Option<Vec<String>> {
    OrdenTmux::Terminar {
        servidor: ServidorTmux::Magi,
        nombre,
    }
    .argumentos()
}

fn crear_con(
    carpeta: Option<&str>,
    comando: Option<&str>,
    lineas_historial: u32,
    columnas: u16,
    filas: u16,
) -> Option<Vec<String>> {
    OrdenTmux::Crear {
        nombre: "a",
        carpeta,
        comando,
        lineas_historial,
        columnas,
        filas,
    }
    .argumentos()
}

fn enganchar_con(equipo: &str) -> Option<Vec<String>> {
    OrdenTmux::Enganchar {
        servidor: ServidorTmux::Magi,
        nombre: "a",
        soltar: false,
        equipo,
        guardar: false,
    }
    .argumentos()
}

#[test]
fn las_ordenes_nuevas_rechazan_datos_que_el_script_rechazaria() {
    assert!(terminar_con("agente").is_some());
    for nombre in ["", "mal nombre", "a;b", "ñ", &"a".repeat(65)] {
        assert!(terminar_con(nombre).is_none(), "{nombre:?}");
    }
    let largo = "c".repeat(4097);
    let carpeta_larga = format!("/{}", "a".repeat(1024));
    assert!(crear_con(None, None, 5000, 80, 24).is_some());
    assert!(crear_con(Some("relativa"), None, 5000, 80, 24).is_none());
    assert!(crear_con(Some("/con\nsalto"), None, 5000, 80, 24).is_none());
    assert!(crear_con(Some(&carpeta_larga), None, 5000, 80, 24).is_none());
    assert!(crear_con(None, Some(&largo), 5000, 80, 24).is_none());
    assert!(crear_con(None, Some("con\0nulo"), 5000, 80, 24).is_none());
    assert!(crear_con(None, None, 1_000_001, 80, 24).is_none());
    assert!(crear_con(None, None, 5000, 1, 24).is_none());
    assert!(crear_con(None, None, 5000, 1001, 24).is_none());
    assert!(crear_con(None, None, 5000, 80, 0).is_none());
    assert!(crear_con(None, None, 5000, 80, 2001).is_none());
    // Un comando con saltos de línea sí es válido: es texto libre.
    assert!(crear_con(None, Some("a\nb"), 5000, 80, 24).is_some());

    assert!(OrdenTmux::Preparar {
        servidor: ServidorTmux::Magi,
        nombre: "mal nombre"
    }
    .argumentos()
    .is_none());
    assert!(enganchar_con("web-01").is_some());
    assert!(enganchar_con("").is_none());
    assert!(enganchar_con("Web").is_none());
    assert!(enganchar_con("con espacio").is_none());
    assert!(enganchar_con(&"x".repeat(21)).is_none());
    for lineas in [0, 100_001] {
        let vista = OrdenTmux::Vista {
            servidor: ServidorTmux::Magi,
            nombre: "a",
            lineas,
        };
        assert!(vista.argumentos().is_none(), "{lineas}");
    }
    assert!(OrdenTmux::Renombrar {
        nombre: "a",
        nuevo: "b c"
    }
    .argumentos()
    .is_none());
}

#[test]
fn los_validadores_de_rust_coinciden_con_los_del_script() {
    assert!(equipo_valido("web-01") && equipo_valido(&"a".repeat(20)));
    assert!(!equipo_valido("") && !equipo_valido(&"a".repeat(21)));
    assert!(!equipo_valido("Web") && !equipo_valido("a_b") && !equipo_valido("ñ"));
    assert!(carpeta_valida("/") && carpeta_valida("~/") && carpeta_valida("/home/usuario/a b"));
    assert!(!carpeta_valida("") && !carpeta_valida("~") && !carpeta_valida("~otro/x"));
    assert!(!carpeta_valida("rel") && !carpeta_valida("/a\nb") && !carpeta_valida("/a\0"));
    assert!(carpeta_valida(&format!("/{}", "a".repeat(1023))));
    assert!(!carpeta_valida(&format!("/{}", "a".repeat(1024))));
    assert!(comando_valido("") && comando_valido("a\nb") && comando_valido(&"c".repeat(4096)));
    assert!(!comando_valido(&"c".repeat(4097)) && !comando_valido("a\0b"));
}

#[test]
fn la_version_de_tmux_se_lee_con_letra_y_prefijos() {
    let v = |a, b| Some(VersionTmux::nueva(a, b));
    assert_eq!(version_de_texto("tmux 3.3a"), v(3, 3));
    assert_eq!(version_de_texto("tmux 3.4"), v(3, 4));
    assert_eq!(version_de_texto("tmux next-3.6"), v(3, 6));
    assert_eq!(version_de_texto("tmux master"), v(99, 0));
    assert_eq!(version_de_texto("tmux openbsd-7.4"), v(99, 0));
    assert_eq!(version_de_texto("3.2a"), v(3, 2));
    assert_eq!(version_de_texto("tmux 3.5-rc2"), v(3, 5));
    assert_eq!(version_de_texto("  tmux 1.8\n"), v(1, 8));
    assert_eq!(version_de_texto("tmux 3"), v(3, 0));
    assert_eq!(version_de_texto("tmux"), None);
    assert_eq!(version_de_texto("tmux a.b"), None);
    assert_eq!(version_de_texto("tmux 123456.1"), None);
    assert_eq!(version_de_texto(""), None);
    // Comparar con el tipo del contrato: la letra no cuenta.
    assert!(version_de_texto("tmux 3.3a").unwrap() >= VersionTmux::MINIMA);
    assert!(version_de_texto("tmux 2.9a").unwrap() < VersionTmux::MINIMA);
    assert!(version_de_texto("tmux 3.0a").unwrap() >= VersionTmux::MINIMA);
    assert!(version_de_texto("tmux 3.5").unwrap() > version_de_texto("tmux 3.4").unwrap());
    assert!(version_de_texto("tmux master").unwrap() > version_de_texto("tmux 3.7c").unwrap());
}

#[test]
fn la_linea_del_exec_escapa_cada_argumento_y_es_una_sola_linea_para_el_shell() {
    let enganchar = OrdenTmux::Enganchar {
        servidor: ServidorTmux::Magi,
        nombre: "agente",
        soltar: true,
        equipo: "portatil",
        guardar: false,
    };
    let linea = enganchar.comando_exec().expect("línea");
    assert!(linea.starts_with("sh -c '"));
    assert!(linea.ends_with(" magi-enganche magi agente 1 portatil 0"));
    // Es una sola línea de órdenes para el shell: el script va entre comillas
    // simples, con sus saltos de línea dentro, y no acaba en salto.
    assert!(!linea.ends_with('\n'));
    assert!(linea.matches("'\\''").count() > 0);
    assert!(OrdenTmux::Enganchar {
        servidor: ServidorTmux::Magi,
        nombre: "mal nombre",
        soltar: true,
        equipo: "x",
        guardar: false,
    }
    .comando_exec()
    .is_none());
    // Las demás llevan solo el arranque y los datos.
    let estado = OrdenTmux::Estado.comando_exec().unwrap();
    assert!(estado.starts_with("sh -c 'S=$(cat)"), "{estado}");
    assert!(estado.ends_with(" magi-tmux estado"), "{estado}");
}

#[test]
fn los_plazos_son_los_de_t32() {
    use std::time::Duration;
    let vista = OrdenTmux::Vista {
        servidor: ServidorTmux::Magi,
        nombre: "a",
        lineas: 1,
    };
    assert_eq!(vista.plazo(), Some(Duration::from_secs(5)));
    assert_eq!(OrdenTmux::Estado.plazo(), Some(Duration::from_secs(10)));
    assert_eq!(
        OrdenTmux::Enganchar {
            servidor: ServidorTmux::Magi,
            nombre: "a",
            soltar: false,
            equipo: "x",
            guardar: false,
        }
        .plazo(),
        None
    );
    // `preparar` va por la entrada estándar con el plazo de siempre.
    let preparar = OrdenTmux::Preparar {
        servidor: ServidorTmux::Magi,
        nombre: "a",
    };
    assert_eq!(preparar.plazo(), Some(Duration::from_secs(10)));
}

#[test]
fn el_script_embebido_no_interpola_texto_en_las_ordenes_de_tmux() {
    // Ninguna orden de tmux del script lleva nombres ni datos del usuario
    // dentro de un texto: todo va como argumento entrecomillado.
    let sin_comentarios: Vec<&str> = SCRIPT
        .lines()
        .filter(|linea| !linea.trim_start().starts_with('#'))
        .collect();
    assert!(!sin_comentarios.iter().any(|linea| linea.contains("eval ")));
    assert!(!sin_comentarios
        .iter()
        .any(|linea| linea.contains("has-session")));
    for linea in SCRIPT.lines().filter(|linea| {
        let linea = linea.trim_start();
        !linea.starts_with('#') && linea.contains(" -t ")
    }) {
        assert!(
            linea.contains(r#"-t "=$"#),
            "orden con -t sin «=<nombre>»: {linea}"
        );
    }
}

#[test]
fn los_limites_del_script_coinciden_con_los_validadores_de_la_ficha() {
    use magi_nucleo::sesiones_persistentes::validadores;
    use magi_nucleo::terminal::tmux::{MAXIMO_CARPETA, MAXIMO_COMANDO};
    // Lo que valida la ficha (nombre, carpeta, comando) es lo que el script
    // acepta: si cambian unos, tienen que cambiar los otros.
    assert_eq!(MAXIMO_CARPETA, validadores::MAXIMO_CARPETA_BYTES);
    assert_eq!(MAXIMO_COMANDO, validadores::MAXIMO_COMANDO_BYTES);
    assert!(SCRIPT.contains(&format!("MAXIMO_CARPETA={MAXIMO_CARPETA}\n")));
    assert!(SCRIPT.contains(&format!("MAXIMO_COMANDO={MAXIMO_COMANDO}\n")));
    for nombre in [
        "a",
        "A_z-09",
        &"b".repeat(64),
        "",
        "mal nombre",
        &"c".repeat(65),
    ] {
        assert_eq!(
            nombre_valido(nombre),
            validadores::nombre_valido(nombre),
            "{nombre:?}"
        );
    }
}

// ======================================================================
// Cierre de la Fase 16 (Fase 9b): el script por la entrada estándar, el
// enganche mínimo y el renombrado en caliente
// ======================================================================

/// Un tope prudente de la línea de órdenes de un `exec` en un host con
/// Dropbear: su límite es de unos 9000 bytes (cifra de memoria del revisor de
/// la Fase 16, sin comprobar). Es lo que la línea de órdenes de una orden de
/// MAGI no debe pasar cuando lo único que lleva son los datos validados.
const TOPE_DROPBEAR: usize = 9000;

fn todas_las_ordenes<'a>(nombre: &'a str, equipo: &'a str) -> Vec<OrdenTmux<'a>> {
    vec![
        OrdenTmux::Comprobar {
            servidor: ServidorTmux::Magi,
            nombre,
        },
        OrdenTmux::Crear {
            nombre,
            carpeta: Some("~/proyectos/api"),
            comando: Some("claude --continue"),
            lineas_historial: 10_000,
            columnas: 120,
            filas: 40,
        },
        OrdenTmux::Preparar {
            servidor: ServidorTmux::Habitual,
            nombre,
        },
        OrdenTmux::Enganchar {
            servidor: ServidorTmux::Magi,
            nombre,
            soltar: true,
            equipo,
            guardar: true,
        },
        OrdenTmux::Estado,
        OrdenTmux::Vista {
            servidor: ServidorTmux::Magi,
            nombre,
            lineas: 100_000,
        },
        OrdenTmux::Historial {
            servidor: ServidorTmux::Habitual,
            nombre,
            lineas: 100_000,
        },
        OrdenTmux::Renombrar {
            nombre,
            nuevo: nombre,
        },
        OrdenTmux::Terminar {
            servidor: ServidorTmux::Magi,
            nombre,
        },
    ]
}

#[test]
fn la_linea_del_exec_nunca_lleva_el_script_grande() {
    // AC: dado el tmux falso, al crear, comprobar o consultar una sesión, la
    // línea de órdenes que llega al host no contiene el script. El script va
    // aparte, por la entrada estándar.
    let nombre = "n".repeat(64);
    let equipo = "e".repeat(20);
    for orden in todas_las_ordenes(&nombre, &equipo) {
        let comando = orden.comando_exec().expect("orden válida");
        match orden.suborden() {
            "enganchar" => {
                // El enganche con PTY lleva su script mínimo, no el grande, y
                // no tiene entrada estándar que lo lleve.
                assert!(orden.entrada().is_none());
                assert!(comando.contains("terminal_enganche.sh"));
                assert!(!comando.contains("preparar_configuracion"));
                assert!(!comando.contains("listar_servidor"));
            }
            suborden => {
                for rastro in [
                    "set -u",
                    "sesion_existe",
                    "preparar_configuracion",
                    "hacer_",
                ] {
                    assert!(
                        !comando.contains(rastro),
                        "{suborden}: la línea lleva el script ({rastro})"
                    );
                }
                assert_eq!(orden.entrada(), Some(SCRIPT), "{suborden}");
                assert!(comando.contains(" magi-tmux "), "{suborden}");
                assert!(comando.contains("eval "), "{suborden}");
            }
        }
    }
}

#[test]
fn las_lineas_del_exec_son_de_pocos_cientos_de_bytes_y_la_del_enganche_cabe_en_dropbear() {
    // El peor caso de cada orden: nombre de 64 caracteres, equipo de 20 y
    // todos los números en su máximo. `crear` queda aparte: lleva la carpeta
    // (hasta 1024 bytes) y el comando (hasta 4096) como argumentos, y la
    // especificación manda que en la línea solo vayan argumentos validados
    // (ver `una_creacion_con_carpeta_y_comando_enormes_puede_pasar_de_dropbear`).
    let nombre = "n".repeat(64);
    let equipo = "e".repeat(20);
    for orden in todas_las_ordenes(&nombre, &equipo) {
        let comando = orden.comando_exec().expect("orden válida");
        // Se ve con `--nocapture`: lo que llega al host de cada orden.
        eprintln!(
            "exec de {}: {} B de línea de órdenes",
            orden.suborden(),
            comando.len()
        );
        match orden.suborden() {
            "crear" => continue,
            "enganchar" => {
                // El único script que sigue en la línea. Con el nombre y el
                // equipo máximos, una vez escapado, cabe en el límite que se
                // le supone a Dropbear con holgura.
                assert!(
                    comando.len() < TOPE_DROPBEAR,
                    "el exec del enganche pesa {} bytes",
                    comando.len()
                );
                assert!(
                    SCRIPT_ENGANCHE.len() < 6 * 1024,
                    "terminal_enganche.sh pesa {} bytes: de pocos KB",
                    SCRIPT_ENGANCHE.len()
                );
            }
            suborden => assert!(
                comando.len() < 512,
                "el exec de {suborden} pesa {} bytes",
                comando.len()
            ),
        }
    }
    // El arranque mínimo, en bytes, escapado como va en la línea.
    let arranque = linea_de_exec(&["sh".to_string(), "-c".to_string(), ARRANQUE.to_string()]);
    assert!(arranque.len() < 160, "{} bytes: {arranque}", arranque.len());
}

#[test]
fn una_creacion_con_carpeta_y_comando_enormes_puede_pasar_de_dropbear() {
    // Hueco conocido y anotado en el informe: `crear` mete la carpeta (hasta
    // 1024 bytes) y el comando (hasta 4096) como argumentos en la línea de
    // órdenes, sin script, y con sus caracteres escapados puede pasar de los
    // ~9000 bytes de Dropbear. La especificación dice «solo argumentos
    // validados en la línea», así que se deja así: este test fija la cifra.
    let carpeta = format!("/{}", "'".repeat(1023));
    let comando = "'".repeat(4096);
    let orden = OrdenTmux::Crear {
        nombre: "a",
        carpeta: Some(&carpeta),
        comando: Some(&comando),
        lineas_historial: 1_000_000,
        columnas: 1000,
        filas: 2000,
    };
    let linea = orden.comando_exec().expect("orden válida");
    assert!(linea.len() > TOPE_DROPBEAR, "{} bytes", linea.len());
    // Con datos normales, ni se acerca.
    let normal = OrdenTmux::Crear {
        nombre: "agente-api",
        carpeta: Some("~/proyectos/api"),
        comando: Some("claude --continue"),
        lineas_historial: 10_000,
        columnas: 120,
        filas: 40,
    };
    assert!(normal.comando_exec().unwrap().len() < 512);
}

#[test]
fn los_scripts_embebidos_medidos_frente_al_limite_de_dropbear() {
    // Medida de TODOS los scripts embebidos con `include_str!` (R154): lo que
    // pesan y lo que pesaría su línea de órdenes escapada, frente a los ~9000
    // bytes de Dropbear. No cambia ninguno: solo se anota (informe de la 9b).
    let scripts: [(&str, &str); 7] = [
        ("terminal_tmux.sh", SCRIPT),
        ("terminal_enganche.sh", SCRIPT_ENGANCHE),
        ("servicios.sh", magi_nucleo::servicios::orden::SCRIPT),
        ("sondeo.sh", magi_nucleo::flota::SCRIPT),
        (
            "escritorio_voxl.sh",
            magi_nucleo::escritorio::voxl::SCRIPT_VOXL,
        ),
        (
            "instalar_clave.sh",
            magi_nucleo::instalar_clave::SCRIPT_INSTALAR_CLAVE,
        ),
        ("arranque", ARRANQUE),
    ];
    for (nombre, script) in scripts {
        let escapado = linea_de_exec(&[
            "sh".to_string(),
            "-c".to_string(),
            script.to_string(),
            "magi".to_string(),
        ]);
        // Se ve con `--nocapture`.
        eprintln!(
            "{nombre}: {} B, {} B escapado en la línea de órdenes (límite supuesto de Dropbear: {TOPE_DROPBEAR})",
            script.len(),
            escapado.len()
        );
    }
    // Los dos de tmux: el grande ya no viaja en la línea y el mínimo cabe.
    assert!(
        SCRIPT.len() > TOPE_DROPBEAR,
        "el grande es el que iba por la línea y no cabía"
    );
    let enganche = linea_de_exec(&[
        "sh".to_string(),
        "-c".to_string(),
        SCRIPT_ENGANCHE.to_string(),
        "magi-enganche".to_string(),
    ]);
    assert!(enganche.len() < TOPE_DROPBEAR, "{} bytes", enganche.len());
}

#[test]
fn los_scripts_de_tmux_no_usan_rangos_de_caracteres() {
    // R159: un rango como «a-z» o «0-9» depende del idioma del host (un bash
    // antiguo con un idioma que no es C puede casar letras acentuadas o
    // mayúsculas) y un canal exec no lo fija. Las listas van escritas carácter
    // a carácter, en los dos scripts y también en los patrones de awk.
    for (nombre, script) in [
        ("terminal_tmux.sh", SCRIPT),
        ("terminal_enganche.sh", SCRIPT_ENGANCHE),
    ] {
        for (numero, linea) in script.lines().enumerate() {
            if linea.trim_start().starts_with('#') {
                continue;
            }
            let mut dentro = false;
            let mut anterior = None;
            let caracteres: Vec<char> = linea.chars().collect();
            for (i, caracter) in caracteres.iter().enumerate() {
                match caracter {
                    '[' => dentro = true,
                    ']' => dentro = false,
                    '-' if dentro => {
                        // Un guion entre dos caracteres de una lista es un
                        // rango; al principio o al final de la lista, no.
                        let siguiente = caracteres.get(i + 1);
                        let es_rango = anterior.is_some_and(|a: char| a.is_ascii_alphanumeric())
                            && siguiente.is_some_and(|s| s.is_ascii_alphanumeric());
                        assert!(
                            !es_rango,
                            "{nombre}:{}: rango de caracteres: {linea}",
                            numero + 1
                        );
                    }
                    _ => {}
                }
                anterior = Some(*caracter);
            }
        }
    }
}

#[test]
fn los_validadores_de_los_scripts_rechazan_las_letras_de_otros_idiomas() {
    // Con una lista explícita, una letra acentuada, una mayúscula donde solo
    // valen minúsculas o un dígito que no es ASCII nunca pasan, sea cual sea el
    // idioma del host: se ejecuta el script con idiomas distintos.
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["agente"]);
    for idioma in ["C", "es_ES.UTF-8", "en_US.UTF-8", "tr_TR.UTF-8"] {
        let entorno_idioma = [("LC_ALL", idioma), ("LANG", idioma)];
        for nombre in ["agéntica", "agente\u{661}", "ñandú", "agente."] {
            let salida = entorno.ejecutar_con(&["comprobar", "magi", nombre], &entorno_idioma);
            assert_eq!(salida.codigo, 64, "{idioma} {nombre:?}");
            let salida =
                entorno.enganche_con(&["magi", nombre, "1", "portatil", "0"], &entorno_idioma);
            assert_eq!(salida.codigo, 64, "{idioma} {nombre:?}");
        }
        // El equipo solo admite minúsculas ASCII, dígitos y guion.
        for equipo in ["Portatil", "portátil", "port_atil", "portatil\u{661}"] {
            let salida =
                entorno.enganche_con(&["magi", "agente", "1", equipo, "0"], &entorno_idioma);
            assert_eq!(salida.codigo, 64, "{idioma} {equipo:?}");
        }
        let salida = entorno.ejecutar_con(&["comprobar", "magi", "agente"], &entorno_idioma);
        assert_eq!(salida.codigo, 0, "{idioma}: {}", salida.stderr);
    }
}

#[test]
fn el_arranque_no_ejecuta_nada_si_el_script_no_llega_entero() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["agente"]);
    // Sin script (la entrada se cierra vacía): sale con 70 y no toca tmux.
    let salida = entorno.ejecutar_con_entrada(&["terminar", "magi", "agente"], &[], "");
    assert_eq!(salida.codigo, 70, "{}", salida.stderr);
    // Un script cortado a mitad, aunque sea un trozo que se podría ejecutar.
    let mut corte = SCRIPT.len() / 2;
    while !SCRIPT.is_char_boundary(corte) {
        corte += 1;
    }
    let mitad: &'static str = &SCRIPT[..corte];
    let salida = entorno.ejecutar_con_entrada(&["terminar", "magi", "agente"], &[], mitad);
    assert_eq!(salida.codigo, 70, "{}", salida.stderr);
    // Ni siquiera el script entero menos su marca final.
    let sin_marca: &'static str =
        &SCRIPT[..SCRIPT.len() - magi_nucleo::terminal::tmux::MARCA_FIN.len() - 1];
    let salida = entorno.ejecutar_con_entrada(&["terminar", "magi", "agente"], &[], sin_marca);
    assert_eq!(salida.codigo, 70, "{}", salida.stderr);
    assert!(salida.stdout.is_empty());
    assert!(entorno.llamadas().is_empty(), "{}", entorno.llamadas());
    assert!(entorno.magi_tiene("agente"));
    // Con el script entero, sí.
    let salida = entorno.ejecutar(&["terminar", "magi", "agente"]);
    assert_eq!(salida.codigo, 0);
    assert!(!entorno.magi_tiene("agente"));
}

#[test]
fn el_script_termina_en_la_marca_que_exige_el_arranque() {
    assert!(SCRIPT
        .trim_end()
        .ends_with(magi_nucleo::terminal::tmux::MARCA_FIN));
    assert!(ARRANQUE.contains(magi_nucleo::terminal::tmux::MARCA_FIN));
    // El arranque lee TODA la entrada antes de ejecutar nada (`cat` no acaba
    // hasta el fin de la entrada) y no es un `exec` ni un `source`.
    assert!(ARRANQUE.starts_with("S=$(cat)"));
    assert!(ARRANQUE.contains("eval \"$S\""));
}

#[test]
fn el_arranque_funciona_con_un_entorno_sin_utf8_y_con_la_variable_tmux_puesta() {
    // Un canal exec no es una sesión de login: sin idioma, con variables
    // heredadas. El script llega íntegro (con sus «, » y tildes) y funciona.
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["agente"]);
    let sin_idioma = [("LC_ALL", "C"), ("TMUX", "/tmp/ajeno,1,0")];
    let salida = entorno.ejecutar_con(&["preparar", "magi", "fantasma"], &sin_idioma);
    assert_eq!(salida.codigo, 75, "{}", salida.stderr);
    assert!(
        salida.stderr.contains("«fantasma»"),
        "las tildes y las comillas llegan íntegras: {}",
        salida.stderr
    );
}

#[test]
fn el_comando_exec_del_enganche_se_ejecuta_tal_cual_lo_recibiria_el_shell_del_host() {
    // La línea completa, escapada, pasada por un shell como el de un `exec` de
    // ssh: el script mínimo llega íntegro con sus argumentos y engancha.
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["sesion-x"]);
    let orden = OrdenTmux::Enganchar {
        servidor: ServidorTmux::Magi,
        nombre: "sesion-x",
        soltar: true,
        equipo: "portatil",
        guardar: false,
    };
    let salida = entorno.ejecutar_linea(&orden.comando_exec().unwrap(), None);
    // Con la entrada cerrada el cliente falso sale con 0 y la sesión sigue
    // viva: para el script, «te soltaron».
    assert_eq!(salida.codigo, 100, "{}", salida.stderr);
    assert!(salida
        .stdout
        .contains("MAGI_TMUX_ADJUNTO=sesion-x soltar=1"));
    assert_eq!(fin_de_enganchar(salida.codigo), FinEnganchar::Soltada);
    assert!(entorno
        .llamadas()
        .contains("attach-session -d -t =sesion-x"));
}

#[test]
fn el_comando_exec_de_una_suborden_con_su_script_por_la_entrada_se_ejecuta_como_en_el_host() {
    // El exec completo de `comprobar`: la línea escapada por un shell y el
    // script por la entrada estándar, como lo manda el servidor.
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["sesion-x"]);
    let orden = OrdenTmux::Comprobar {
        servidor: ServidorTmux::Magi,
        nombre: "sesion-x",
    };
    let linea = orden.comando_exec().unwrap();
    assert!(linea.len() < 512, "{}", linea.len());
    let salida = entorno.ejecutar_linea(&linea, orden.entrada());
    assert_eq!(salida.codigo, 0, "{}", salida.stderr);
    assert!(
        salida
            .stdout
            .starts_with("MAGI_TMUX=tmux 3.4\nMAGI_SESION=existe\n"),
        "{}",
        salida.stdout
    );
    // La misma línea sin la entrada no hace nada (y no toca tmux).
    let antes = entorno.llamadas().lines().count();
    let sin_script = entorno.ejecutar_linea(&linea, None);
    assert_eq!(sin_script.codigo, 70);
    assert_eq!(entorno.llamadas().lines().count(), antes);
}

#[test]
fn entrada_del_exec_solo_da_el_script_si_los_argumentos_son_los_del_arranque() {
    use magi_nucleo::terminal::tmux::entrada_del_exec;
    let comprobar = OrdenTmux::Comprobar {
        servidor: ServidorTmux::Magi,
        nombre: "a",
    }
    .argumentos()
    .unwrap();
    assert_eq!(entrada_del_exec(&comprobar), Some(SCRIPT));
    let enganchar = enganchar_con("web-01").unwrap();
    assert_eq!(entrada_del_exec(&enganchar), None);
    assert_eq!(entrada_del_exec(&[]), None);
    assert_eq!(
        entrada_del_exec(&["sh".to_string(), "-c".to_string(), "otra cosa".to_string()]),
        None
    );
}

#[test]
fn el_comando_exec_escapa_los_datos_y_no_deja_pasar_nada_invalido() {
    let orden = OrdenTmux::Enganchar {
        servidor: ServidorTmux::Habitual,
        nombre: "ok",
        soltar: false,
        equipo: "x",
        guardar: false,
    };
    let comando = orden.comando_exec().unwrap();
    assert!(comando.ends_with(" magi-enganche habitual ok 0 x 0"));
    // Datos no válidos: ni línea ni comando.
    for (nombre, equipo) in [
        ("a b", "x"),
        ("a;rm", "x"),
        ("ok", "X"),
        ("ok", "a b"),
        ("", "x"),
    ] {
        let orden = OrdenTmux::Enganchar {
            servidor: ServidorTmux::Magi,
            nombre,
            soltar: false,
            equipo,
            guardar: false,
        };
        assert!(orden.comando_exec().is_none(), "{nombre:?} {equipo:?}");
    }
    // Los datos con caracteres raros que sí son válidos para el script (la
    // carpeta y el comando de `crear`) van escapados como un solo argumento.
    let crear = OrdenTmux::Crear {
        nombre: "a",
        carpeta: Some("/tmp/con espacio's"),
        comando: Some("echo $HOME; rm -rf *"),
        lineas_historial: 1,
        columnas: 80,
        filas: 24,
    };
    let linea = crear.comando_exec().unwrap();
    assert!(linea.contains("'/tmp/con espacio'\\''s'"), "{linea}");
    assert!(linea.contains("'echo $HOME; rm -rf *'"), "{linea}");
}

#[test]
fn un_renombrado_mientras_se_esta_enganchado_no_cuenta_como_terminada() {
    // La sesión se renombra mientras este cliente está enganchado y, después,
    // otro equipo se engancha con `-d` y lo suelta. Mirar la existencia por el
    // nombre antiguo dava «terminada» (0) en vez de «soltada» (100): el script
    // mira el identificador interno de la sesión.
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_lineas(
        &entorno.estado_magi,
        &[
            "otra\t0\t1\t2026-11-09 19:00:00".to_string(),
            "refactor\t0\t1\t2026-11-09 19:00:00".to_string(),
        ],
    );
    let mut hijo = entorno
        .preparar_enganche(
            &["magi", "refactor", "1", "sobremesa", "0"],
            &[("MAGI_TMUX_AL_SOLTAR_RENOMBRA", "refactor-2")],
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("arrancar el enganche");
    // La entrada se mantiene abierta hasta el final: `wait_with_output` cierra
    // la que quede en el hijo y el cliente falso saldría por ahí.
    let _entrada = hijo.stdin.take();
    // El cliente falso espera con la entrada abierta; cuando hay llamada a
    // attach-session, otro equipo escribe el marcador de la sesión y el falso
    // renombra antes de soltar al cliente.
    let inicio = std::time::Instant::now();
    while !entorno.llamadas().contains("attach-session") {
        assert!(
            inicio.elapsed() < std::time::Duration::from_secs(10),
            "no se enganchó: {}",
            entorno.llamadas()
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    std::thread::sleep(std::time::Duration::from_millis(300));
    fs::write(
        format!("{}.cliente.refactor", entorno.estado_magi.display()),
        "otro\n",
    )
    .unwrap();
    let salida = hijo.wait_with_output().expect("esperar al enganche");
    assert_eq!(
        salida.status.code(),
        Some(100),
        "stdout: {} stderr: {}",
        String::from_utf8_lossy(&salida.stdout),
        String::from_utf8_lossy(&salida.stderr)
    );
    assert!(
        entorno.magi_tiene("refactor-2"),
        "{}",
        entorno.estado_de_magi()
    );
    assert!(!entorno.magi_tiene("refactor"));
    // El identificador se pidió con la lista exacta, sin has-session.
    assert!(entorno
        .llamadas()
        .contains("list-sessions -F #{session_id}"));
}
