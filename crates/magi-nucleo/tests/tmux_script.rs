//! Pruebas de caja de `terminal_tmux.sh` (Fase 15): se ejecuta de verdad con
//! `sh` y el tmux falso compartido de `tests/fijos/terminal/tmux` en el
//! `PATH` (nunca el tmux del sistema), y se comprueba la salida exacta que lee
//! el núcleo. Sin red.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use magi_nucleo::terminal::tmux::{
    etiqueta_equipo, leer_listado, leer_terminar, nombre_nuevo, nombre_valido, orden, Listado,
    ResultadoTerminar, SesionTmux, Suborden, SCRIPT,
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

/// Temporal con el tmux falso (o sin él) y un shell de acceso falso.
struct Entorno {
    _temporal: tempfile::TempDir,
    bin: PathBuf,
    con_tmux: bool,
    estado: PathBuf,
    llamadas: PathBuf,
    shell_estado: PathBuf,
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
            bin
        };
        let estado = temporal.path().join("estado");
        let llamadas = temporal.path().join("llamadas");
        let shell_estado = temporal.path().join("shell.txt");
        Self {
            _temporal: temporal,
            bin,
            con_tmux: tmux.is_some(),
            estado,
            llamadas,
            shell_estado,
        }
    }

    /// Siembra el fichero de estado del tmux falso con estas sesiones
    /// (nombre, adheridos, ventanas, creada), una por línea y con tabuladores.
    fn sembrar_sesiones(&self, sesiones: &[(&str, u32, u16, &str)]) {
        let mut contenido = String::new();
        for (nombre, adheridos, ventanas, creada) in sesiones {
            contenido.push_str(&format!("{nombre}\t{adheridos}\t{ventanas}\t{creada}\n"));
        }
        fs::write(&self.estado, contenido).expect("escribir el estado del tmux falso");
    }

    fn estado(&self) -> String {
        fs::read_to_string(&self.estado).unwrap_or_default()
    }

    fn tiene_sesion(&self, nombre: &str) -> bool {
        self.estado()
            .lines()
            .any(|linea| linea.split('\t').next() == Some(nombre))
    }

    fn llamadas(&self) -> String {
        fs::read_to_string(&self.llamadas).unwrap_or_default()
    }

    fn shell_recibido(&self) -> Option<String> {
        fs::read_to_string(&self.shell_estado).ok()
    }

    fn ejecutar(&self, argumentos: &[&str]) -> Salida {
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
        let salida = comando
            .env("PATH", path)
            .env("SHELL", self.bin.join("shell"))
            .env("MAGI_TMUX_ESTADO", &self.estado)
            .env("MAGI_TMUX_LLAMADAS", &self.llamadas)
            .env("MAGI_SHELL_ESTADO", &self.shell_estado)
            .output()
            .expect("ejecutar terminal_tmux.sh");
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
    let sep = '\u{1f}';
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
    assert!(llamadas.contains("kill-session -t magi-portatil-1"));
    assert!(llamadas.contains("kill-session -t magi-portatil-9"));
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
