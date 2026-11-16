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
    carpeta_valida, comando_valido, equipo_valido, etiqueta_equipo, fin_de_enganchar, leer_listado,
    leer_terminar, nombre_nuevo, nombre_valido, orden, version_de_texto, FinEnganchar, Listado,
    OrdenTmux, ResultadoTerminar, SesionTmux, Suborden, SCRIPT,
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

    /// Ejecuta el script con variables de entorno adicionales (versión del
    /// tmux falso, fallos, `HOME`…).
    fn ejecutar_con(&self, argumentos: &[&str], entorno: &[(&str, &str)]) -> Salida {
        // /bin/sh absoluto: el PATH del hijo es solo el de los falsos, para
        // que el script no vea el tmux real de la máquina.
        let mut comando = Command::new("/bin/sh");
        comando.arg("-c").arg(SCRIPT).arg("magi-tmux");
        for argumento in argumentos {
            comando.arg(argumento);
        }
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
        let salida = comando.output().expect("ejecutar terminal_tmux.sh");
        Salida {
            stdout: String::from_utf8_lossy(&salida.stdout).to_string(),
            stderr: String::from_utf8_lossy(&salida.stderr).to_string(),
            codigo: salida.status.code().unwrap_or(-1),
        }
    }
}

/// Binarios falsos compartidos por todas las pruebas: se escriben una sola
/// vez, al principio, y nunca se vuelven a abrir para escritura.
fn binarios_compartidos() -> &'static PathBuf {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("magi-falsos-tmux-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("directorio de binarios falsos compartidos");
        instalar(&dir.join("tmux"), FAKE_TMUX);
        instalar(&dir.join("shell"), FAKE_SHELL);
        instalar(&dir.join("infocmp"), FAKE_INFOCMP);
        dir
    })
}

fn instalar(destino: &Path, contenido: &str) {
    fs::write(destino, contenido).expect("escribir el binario falso");
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(destino, fs::Permissions::from_mode(0o755))
        .expect("permisos del binario falso");
}

#[test]
fn listar_pasa_el_separador_a_tabuladores_y_normaliza_enganchada() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_sesiones(&[
        ("magi-portatil-1", 2, 2, "Mon Nov  9 19:20:00 2026"),
        ("magi-portatil-2", 0, 1, "Tue Nov 10 08:00:00 2026"),
    ]);
    let salida = entorno.ejecutar(&["listar"]);
    assert_eq!(salida.codigo, 0);
    assert!(salida.stderr.is_empty());
    assert_eq!(
        salida.stdout,
        concat!(
            "MAGI_TMUX=tmux 3.4\n",
            "MAGI_SESION\tmagi-portatil-1\t1\t2\tMon Nov  9 19:20:00 2026\n",
            "MAGI_SESION\tmagi-portatil-2\t0\t1\tTue Nov 10 08:00:00 2026\n",
        )
    );
    let llamadas = entorno.llamadas();
    // Tabulador entre tmux y el script: tmux 3.4 y 3.5 escriben 0x1f como
    // «\037» en la salida de list-sessions.
    let sep = '\t';
    assert!(llamadas.contains(&format!(
        "list-sessions -F #{{session_name}}{sep}#{{session_attached}}{sep}#{{session_windows}}{sep}#{{session_created_string}}"
    )));

    // Lo que imprime el script es lo que entiende el lector del núcleo.
    let listado = leer_listado(&salida.stdout);
    assert_eq!(listado.tmux.as_deref(), Some("tmux 3.4"));
    assert_eq!(listado.sesiones.len(), 2);
    assert_eq!(listado.sesiones[0].nombre, "magi-portatil-1");
    assert!(listado.sesiones[0].enganchada);
    assert_eq!(listado.sesiones[0].ventanas, 2);
    assert_eq!(listado.sesiones[0].creada, "Mon Nov  9 19:20:00 2026");
    assert!(!listado.sesiones[1].enganchada);
}

#[test]
fn listar_sin_sesiones_solo_da_la_version() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    // Sin fichero de estado, el tmux falso falla como "no hay servidor".
    let salida = entorno.ejecutar(&["listar"]);
    assert_eq!(salida.codigo, 0);
    assert_eq!(salida.stdout, "MAGI_TMUX=tmux 3.4\n");
    let listado = leer_listado(&salida.stdout);
    assert!(listado.sesiones.is_empty());
}

#[test]
fn sin_tmux_listar_y_comprobar_dicen_que_no() {
    let entorno = Entorno::nuevo(None);
    let salida = entorno.ejecutar(&["listar"]);
    assert_eq!(salida.codigo, 0);
    assert_eq!(salida.stdout, "MAGI_TMUX=no\n");
    assert!(salida.stderr.is_empty());

    let comprobar = entorno.ejecutar(&["comprobar", "magi-portatil-1"]);
    assert_eq!(comprobar.codigo, 0);
    assert_eq!(comprobar.stdout, "MAGI_TMUX=no\nMAGI_SESION=nueva\n");
    // No se llamó a ningún tmux: no existe.
    assert!(entorno.llamadas().is_empty());
}

#[test]
fn un_tmux_que_falla_al_pedir_la_version_cuenta_como_ausente() {
    let entorno = Entorno::nuevo(Some(
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$MAGI_TMUX_LLAMADAS\"\nexit 1\n",
    ));
    let salida = entorno.ejecutar(&["listar"]);
    assert_eq!(salida.codigo, 0);
    assert_eq!(salida.stdout, "MAGI_TMUX=no\n");
    assert_eq!(entorno.llamadas().trim_end(), "-V");
}

#[test]
fn un_tmux_anterior_a_1_8_se_trata_como_sin_tmux() {
    let viejo = "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$MAGI_TMUX_LLAMADAS\"\ncase \"$1\" in\n    -V) printf '%s\\n' 'tmux 1.7' ;;\nesac\n";
    let entorno = Entorno::nuevo(Some(viejo));
    let salida = entorno.ejecutar(&["listar"]);
    assert_eq!(salida.stdout, "MAGI_TMUX=no\n");
    // Abrir con un tmux viejo también usa el shell de acceso.
    let abrir = entorno.ejecutar(&["abrir", "magi-portatil-1"]);
    assert_eq!(abrir.codigo, 0);
    assert_eq!(entorno.shell_recibido().as_deref(), Some("-l\n"));
    assert!(!entorno.llamadas().contains("new-session"));
}

#[test]
fn comprobar_dice_si_la_sesion_existe_por_nombre_exacto() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_sesiones(&[("magi-portatil-1", 0, 1, "2026-11-09 19:00:00")]);
    let existe = entorno.ejecutar(&["comprobar", "magi-portatil-1"]);
    assert_eq!(existe.codigo, 0);
    assert_eq!(existe.stdout, "MAGI_TMUX=tmux 3.4\nMAGI_SESION=existe\n");
    let nueva = entorno.ejecutar(&["comprobar", "magi-portatil-2"]);
    assert_eq!(nueva.stdout, "MAGI_TMUX=tmux 3.4\nMAGI_SESION=nueva\n");
    let llamadas = entorno.llamadas();
    // Comparación exacta por nombre (no `has-session -t`, que casa prefijos).
    assert!(llamadas.contains("list-sessions -F #{session_name}"));
}

#[test]
fn abrir_sustituye_el_proceso_por_new_session() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    // Sin estado previo: el tmux falso crea la sesión y se queda enganchado
    // leyendo la entrada; con la entrada cerrada sale y deja su marca.
    let salida = entorno.ejecutar(&["abrir", "magi-portatil-1"]);
    assert_eq!(salida.codigo, 0);
    assert_eq!(salida.stdout, "MAGI_TMUX_SESION=magi-portatil-1\n");
    assert!(entorno.tiene_sesion("magi-portatil-1"));
    let llamadas = entorno.llamadas();
    assert!(llamadas.contains("-V"));
    assert!(llamadas.contains("new-session -A -s magi-portatil-1"));
    assert!(entorno.shell_recibido().is_none());
}

#[test]
fn abrir_sin_tmux_abre_el_shell_de_acceso() {
    let entorno = Entorno::nuevo(None);
    let salida = entorno.ejecutar(&["abrir", "magi-portatil-1"]);
    assert_eq!(salida.codigo, 0);
    assert!(salida.stdout.is_empty());
    assert_eq!(entorno.shell_recibido().as_deref(), Some("-l\n"));
}

#[test]
fn terminar_usa_kill_session_y_dice_el_resultado() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_sesiones(&[("magi-portatil-1", 0, 1, "2026-11-09 19:00:00")]);
    let terminada = entorno.ejecutar(&["terminar", "magi-portatil-1"]);
    assert_eq!(terminada.codigo, 0);
    assert_eq!(terminada.stdout, "MAGI_RESULTADO=ok\n");
    assert_eq!(leer_terminar(&terminada.stdout), ResultadoTerminar::Ok);
    // El tmux falso quita la sesión de su estado.
    assert!(!entorno.tiene_sesion("magi-portatil-1"));

    let perdida = entorno.ejecutar(&["terminar", "magi-portatil-9"]);
    assert_eq!(perdida.codigo, 0);
    assert_eq!(perdida.stdout, "MAGI_RESULTADO=no_existe\n");
    assert_eq!(leer_terminar(&perdida.stdout), ResultadoTerminar::NoExiste);
    let llamadas = entorno.llamadas();
    // R146: «=» para que tmux no case por prefijo.
    assert!(llamadas.contains("kill-session -t =magi-portatil-1"));
    assert!(llamadas.contains("kill-session -t =magi-portatil-9"));
}

#[test]
fn terminar_sin_tmux_dice_que_no_existe() {
    let entorno = Entorno::nuevo(None);
    let salida = entorno.ejecutar(&["terminar", "magi-portatil-1"]);
    assert_eq!(salida.codigo, 0);
    assert_eq!(salida.stdout, "MAGI_RESULTADO=no_existe\n");
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
        let salida = entorno.ejecutar(&["comprobar", nombre]);
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
    let casos: [&[&str]; 5] = [
        &[],
        &["comprobar"],
        &["listar", "magi-portatil-1"],
        &["abrir", "magi-portatil-1", "de-mas"],
        &["pepe", "magi-portatil-1"],
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
    let salida = entorno.ejecutar(&["comprobar", &nombre]);
    assert_eq!(salida.codigo, 0);
    assert_eq!(salida.stdout, "MAGI_TMUX=tmux 3.4\nMAGI_SESION=nueva\n");
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
fn el_nombre_nuevo_es_magi_equipo_n_y_evita_los_usados() {
    assert_eq!(nombre_nuevo("portátil", &[]), "magi-portatil-1");
    let existentes = vec![
        "magi-portatil-1".to_string(),
        "magi-portatil-3".to_string(),
        "magi-otro-1".to_string(),
        "suelta".to_string(),
    ];
    assert_eq!(nombre_nuevo("portátil", &existentes), "magi-portatil-2");
    let existentes = vec![
        "magi-portatil-2".to_string(),
        "magi-portatil-x".to_string(),
        "magi-portatil-".to_string(),
    ];
    assert_eq!(nombre_nuevo("portátil", &existentes), "magi-portatil-1");
    // La etiqueta del equipo normaliza el nombre libre.
    assert_eq!(
        nombre_nuevo("Portátil de Héctor", &[]),
        "magi-portatil-de-hector-1"
    );
}

#[test]
fn la_orden_del_script_lleva_el_nombre_como_argumento() {
    let comprobar = orden(Suborden::Comprobar, Some("magi-portatil-1")).expect("orden válida");
    assert_eq!(
        comprobar,
        vec![
            "sh",
            "-c",
            SCRIPT,
            "magi-tmux",
            "comprobar",
            "magi-portatil-1",
        ]
    );
    assert_eq!(
        orden(Suborden::Abrir, Some("magi-portatil-1")).unwrap()[4],
        "abrir"
    );
    assert_eq!(
        orden(Suborden::Terminar, Some("magi-portatil-1")).unwrap()[4],
        "terminar"
    );
    let listar = orden(Suborden::Listar, None).expect("listar sin nombre");
    assert_eq!(listar.len(), 5);
    assert_eq!(listar[4], "listar");

    assert!(orden(Suborden::Listar, Some("magi-portatil-1")).is_none());
    assert!(orden(Suborden::Comprobar, None).is_none());
    assert!(orden(Suborden::Abrir, Some("mal nombre")).is_none());
    assert!(orden(Suborden::Terminar, Some("")).is_none());
}

#[test]
fn leer_listado_entiende_lo_bueno_y_tolera_crlf() {
    let salida =
        "MAGI_TMUX=tmux 3.4\r\nMAGI_SESION\tmagi-portatil-1\t1\t2\tlun 9 nov 19:20:00 2026\r\n";
    let listado = leer_listado(salida);
    assert_eq!(listado.tmux.as_deref(), Some("tmux 3.4"));
    assert_eq!(
        listado.sesiones,
        vec![SesionTmux {
            nombre: "magi-portatil-1".to_string(),
            enganchada: true,
            ventanas: 2,
            creada: "lun 9 nov 19:20:00 2026".to_string(),
        }]
    );
    // También vale un espacio como separador tras MAGI_SESION.
    let listado = leer_listado("MAGI_SESION magi-1\t0\t1\t2026");
    assert_eq!(listado.sesiones.len(), 1);
    assert!(!listado.sesiones[0].enganchada);
}

#[test]
fn leer_listado_sin_tmux_o_vacio() {
    assert_eq!(leer_listado("MAGI_TMUX=no\n"), Listado::default());
    assert_eq!(leer_listado("MAGI_TMUX=\n"), Listado::default());
    assert_eq!(leer_listado(""), Listado::default());
    assert!(leer_listado("MAGI_TMUX=tmux 3.4\n").sesiones.is_empty());
}

#[test]
fn leer_listado_acota_la_entrada_no_fiable() {
    // 200 sesiones como mucho.
    let mut salida = String::new();
    for numero in 0..250 {
        salida.push_str(&format!("MAGI_SESION\ts-{numero}\t0\t1\tx\n"));
    }
    assert_eq!(leer_listado(&salida).sesiones.len(), 200);

    // Nombres de más de 64 caracteres o con controles: fuera.
    let largo = "a".repeat(65);
    assert!(leer_listado(&format!("MAGI_SESION\t{largo}\t0\t1\tx\n"))
        .sesiones
        .is_empty());
    assert!(leer_listado("MAGI_SESION\tmagi\u{1}-1\t0\t1\tx\n")
        .sesiones
        .is_empty());
    // Una línea de más de 1 KiB: fuera.
    let enorme = format!("MAGI_SESION\tmagi-1\t0\t1\t{}\n", "x".repeat(1100));
    assert!(leer_listado(&enorme).sesiones.is_empty());
    // Líneas que no encajan: fuera, sin pánico.
    assert!(leer_listado("MAGI_SESION\tmagi-1\t0\n").sesiones.is_empty());
    assert!(leer_listado("MAGI_SESIONX\tsi\n").sesiones.is_empty());
    assert!(leer_listado("\u{1b}[31mMAGI_SESION\tmagi-1\t0\t1\tx\n")
        .sesiones
        .is_empty());
    // La versión se limpia y se acota.
    let version = "v".repeat(200);
    let listado = leer_listado(&format!("MAGI_TMUX={version}\n"));
    assert_eq!(listado.tmux.as_deref().map(str::len), Some(64));
}

#[test]
fn leer_listado_no_se_rompe_con_campos_ilegibles() {
    let listado = leer_listado("MAGI_SESION\tmagi-1\tno\tmuchas\tlun\u{7} 9\n");
    assert_eq!(listado.sesiones.len(), 1);
    assert!(!listado.sesiones[0].enganchada);
    assert_eq!(listado.sesiones[0].ventanas, 0);
    assert_eq!(listado.sesiones[0].creada, "lun 9");
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

    // Lo mismo en el tmux habitual y con la forma antigua del script.
    entorno.sembrar_sesiones(&[("magi-a-10", 0, 1, "2026-11-09 19:00:00")]);
    let habitual = entorno.ejecutar(&["terminar", "habitual", "magi-a-1"]);
    assert_eq!(habitual.stdout, "MAGI_RESULTADO=no_existe\n");
    let antigua = entorno.ejecutar(&["terminar", "magi-a-1"]);
    assert_eq!(antigua.stdout, "MAGI_RESULTADO=no_existe\n");
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
        &["enganchar", "magi", "agente", "1", "portatil"],
        &["enganchar", "habitual", "web", "0", "portatil"],
        &["estado"],
        &["vista", "magi", "agente", "10"],
        &["historial", "habitual", "web", "10"],
        &["renombrar", "otra", "otra-2"],
        &["terminar", "magi", "agente"],
        &["terminar", "habitual", "web"],
        &["listar"],
    ];
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

// -------------------------------------------------------------- enganchar

#[test]
fn enganchar_suelta_pone_el_equipo_aplica_la_configuracion_y_engancha() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["agente"]);
    let salida = entorno.ejecutar(&["enganchar", "magi", "agente", "1", "portatil"]);
    // Con la entrada cerrada, el tmux falso vuelve y la sesión sigue viva:
    // a este cliente lo soltaron.
    assert_eq!(salida.codigo, 100, "{}", salida.stderr);
    assert_eq!(fin_de_enganchar(salida.codigo), FinEnganchar::Soltada);
    assert!(salida.stdout.contains("MAGI_TMUX_ADJUNTO=agente soltar=1"));
    // La lista exacta, el equipo, la configuración, el enganche y, al volver,
    // otra vez la lista exacta para saber si la sesión sigue viva.
    let propias = lineas_del_propio(&entorno);
    assert_eq!(propias.len(), 5, "{propias:?}");
    assert!(propias[0].ends_with("list-sessions -F #{session_name}"));
    assert!(propias[1].ends_with("set-option -q -t =agente: @magi_equipo portatil"));
    assert!(propias[2].contains("set -gq status off"));
    assert!(propias[2].contains("set -sgq extended-keys on"));
    // Idempotente y sin start-server ni history-limit.
    assert!(!propias[2].contains("start-server"));
    assert!(!propias[2].contains("history-limit"));
    assert!(!propias[2].contains("terminal-overrides"));
    assert!(propias[3].ends_with("attach-session -d -t =agente"));
    assert!(propias[4].ends_with("list-sessions -F #{session_name}"));
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
fn enganchar_al_compartir_no_suelta_a_los_demas() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["agente"]);
    let salida = entorno.ejecutar(&["enganchar", "magi", "agente", "0", "web-01"]);
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
fn enganchar_sale_con_0_si_la_sesion_termino_y_con_75_si_no_existia() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["agente"]);
    // El programa de la sesión termina mientras se está enganchado.
    let salida = entorno.ejecutar_con(
        &["enganchar", "magi", "agente", "1", "portatil"],
        &[("MAGI_TMUX_ATTACH_MATA", "1")],
    );
    assert_eq!(salida.codigo, 0, "{}", salida.stderr);
    assert_eq!(fin_de_enganchar(salida.codigo), FinEnganchar::Terminada);
    assert!(!entorno.magi_tiene("agente"));

    // No existía: ni se aplica la configuración ni se engancha.
    let antes = entorno.llamadas().lines().count();
    let salida = entorno.ejecutar(&["enganchar", "magi", "fantasma", "1", "portatil"]);
    assert_eq!(salida.codigo, 75);
    assert_eq!(fin_de_enganchar(salida.codigo), FinEnganchar::NoExiste);
    assert!(salida.stderr.contains("fantasma"));
    assert!(salida.stdout.is_empty());
    // Solo la versión y la lista exacta: ni configuración ni enganche.
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
fn enganchar_a_magi_a_1_no_se_engancha_a_magi_a_10() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["magi-a-10"]);
    let salida = entorno.ejecutar(&["enganchar", "magi", "magi-a-1", "1", "portatil"]);
    assert_eq!(salida.codigo, 75);
    assert!(!entorno.llamadas().contains("attach-session"));
    assert!(!entorno.llamadas().contains("@magi_equipo"));
}

#[test]
fn enganchar_que_no_puede_engancharse_devuelve_el_codigo_de_tmux() {
    // Sin terminal, el cliente de tmux sale con error y la sesión sigue viva:
    // no es «soltada».
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["agente"]);
    let salida = entorno.ejecutar_con(
        &["enganchar", "magi", "agente", "1", "portatil"],
        &[("MAGI_TMUX_ATTACH_ESTADO", "1")],
    );
    assert_eq!(salida.codigo, 1);
    assert_eq!(fin_de_enganchar(salida.codigo), FinEnganchar::Error(1));
}

#[test]
fn enganchar_al_habitual_no_toca_su_configuracion_ni_sus_opciones() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_sesiones(&[("web", 0, 1, "2026-11-09 19:00:00")]);
    let salida = entorno.ejecutar(&["enganchar", "habitual", "web", "1", "portatil"]);
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
fn enganchar_exige_tmux_y_3_0_para_el_propio() {
    let sin = Entorno::nuevo(None);
    let salida = sin.ejecutar(&["enganchar", "magi", "agente", "1", "portatil"]);
    assert_eq!(salida.codigo, 76);
    assert_eq!(fin_de_enganchar(salida.codigo), FinEnganchar::SinTmux);
    assert!(salida.stderr.contains("instala tmux en el host"));

    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["agente"]);
    entorno.sembrar_sesiones(&[("web", 0, 1, "2026-11-09 19:00:00")]);
    let viejo = [("MAGI_TMUX_VERSION", "tmux 2.9a")];
    let salida = entorno.ejecutar_con(&["enganchar", "magi", "agente", "1", "portatil"], &viejo);
    assert_eq!(salida.codigo, 74);
    assert_eq!(fin_de_enganchar(salida.codigo), FinEnganchar::Antiguo);
    assert!(salida
        .stderr
        .contains("tmux demasiado antiguo (3.0 o posterior)"));
    // El habitual solo pide 1.8.
    let salida = entorno.ejecutar_con(&["enganchar", "habitual", "web", "1", "portatil"], &viejo);
    assert_eq!(salida.codigo, 100);
}

#[test]
fn enganchar_valida_sus_argumentos() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    let casos: [[&str; 4]; 7] = [
        ["otro", "agente", "1", "portatil"],
        ["magi", "mal nombre", "1", "portatil"],
        ["magi", "agente", "2", "portatil"],
        ["magi", "agente", "", "portatil"],
        ["magi", "agente", "1", "Portatil"],
        ["magi", "agente", "1", ""],
        ["magi", "agente", "1", "equipo-demasiado-largo-x"],
    ];
    for caso in casos {
        let mut argumentos = vec!["enganchar"];
        argumentos.extend(caso);
        let salida = entorno.ejecutar(&argumentos);
        assert_eq!(salida.codigo, 64, "caso rechazado: {caso:?}");
    }
    assert_eq!(
        entorno
            .ejecutar(&["enganchar", "magi", "agente", "1"])
            .codigo,
        64
    );
    assert!(entorno.llamadas().is_empty());
}

#[test]
fn el_script_quita_la_variable_tmux_para_no_anidar() {
    let entorno = Entorno::nuevo(Some(FAKE_TMUX));
    entorno.sembrar_magi(&["agente"]);
    let salida = entorno.ejecutar_con(
        &["enganchar", "magi", "agente", "1", "portatil"],
        &[("TMUX", "/tmp/otro-tmux,123,0")],
    );
    assert_eq!(salida.codigo, 100);
    let visto = fs::read_to_string(&entorno.entorno_tmux).unwrap();
    assert!(visto.lines().count() >= 3);
    assert!(visto.lines().all(|linea| linea == "TMUX="), "{visto}");
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
    assert_eq!(controles[4], "equipo");
    assert_eq!(controles[9], "[31mrojo");
    let largo: Vec<&str> = lineas[1].split(US).collect();
    assert_eq!(largo[9].len(), 400);
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
            SCRIPT,
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
    let enganchar = OrdenTmux::Enganchar {
        servidor: ServidorTmux::Magi,
        nombre: "agente",
        soltar: true,
        equipo: "portatil",
    };
    assert_eq!(
        &enganchar.argumentos().unwrap()[4..],
        ["enganchar", "magi", "agente", "1", "portatil"]
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
fn la_linea_del_pty_hace_exec_y_escapa_cada_argumento() {
    let enganchar = OrdenTmux::Enganchar {
        servidor: ServidorTmux::Magi,
        nombre: "agente",
        soltar: true,
        equipo: "portatil",
    };
    let linea = enganchar.linea_para_pty().expect("línea");
    assert!(linea.starts_with("exec sh -c "));
    assert!(linea.ends_with(" magi-tmux enganchar magi agente 1 portatil\n"));
    // Es una sola línea de órdenes para el shell: el script va entre comillas
    // simples, con sus saltos de línea dentro, y termina en un único salto.
    assert!(linea.ends_with('\n'));
    assert!(linea.matches("'\\''").count() > 0);
    assert!(OrdenTmux::Enganchar {
        servidor: ServidorTmux::Magi,
        nombre: "mal nombre",
        soltar: true,
        equipo: "x",
    }
    .linea_para_pty()
    .is_none());
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
            equipo: "x"
        }
        .plazo(),
        None
    );
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
