//! Pruebas de `terminal_tmux.sh` con tmux de verdad (Fase 16).
//!
//! Cada prueba lleva su propio servidor de tmux: `TMUX_TMPDIR` apunta a un
//! temporal corto (el socket no puede pasar de ~108 bytes), `TMUX` se quita
//! del entorno y al acabar se mata el servidor (`kill-server`, propio y
//! habitual). Nunca se toca el tmux del usuario ni su `~/.tmux.conf`: el
//! `HOME` de las pruebas es otro.
//!
//! Se omiten con un aviso si no hay tmux 3.0 o posterior o si falta lo
//! necesario para darle un terminal (el comando `script` de util-linux).
//! `MAGI_TMUX_DIR` apunta al directorio que contiene el `tmux` que se quiere
//! probar (para repetirlas con 3.2a, 3.4, 3.5 y 3.7c); sin ella se usa el del
//! `PATH`.

use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use magi_nucleo::sesiones_persistentes::lectura::{
    leer_comprobar, leer_crear, leer_estado, leer_renombrar, leer_terminar, ResultadoCrear,
    ResultadoRenombrar,
};
use magi_nucleo::sesiones_persistentes::{ServidorTmux, SesionVista, VersionTmux};
use magi_nucleo::terminal::tmux::{
    fin_de_enganchar, version_de_texto, FinEnganchar, ResultadoTerminar, SCRIPT,
};

const PLAZO: Duration = Duration::from_secs(15);

/// Salida cruda de una ejecución del script.
struct Salida {
    stdout: String,
    stderr: String,
    codigo: i32,
}

/// Un entorno aislado con tmux de verdad.
struct Real {
    /// Base de todo: `TMUX_TMPDIR`, el `HOME` y el script.
    base: tempfile::TempDir,
    home: PathBuf,
    tmpdir: PathBuf,
    path: String,
    script: PathBuf,
    version: VersionTmux,
}

impl Real {
    /// `None` (con un aviso en la salida de error) si falta lo necesario.
    fn nuevo(prueba: &str) -> Option<Self> {
        let omitir = |motivo: &str| {
            eprintln!("tmux_real::{prueba}: se omite, {motivo}");
            None
        };
        let directorio = std::env::var("MAGI_TMUX_DIR").ok();
        let path = match &directorio {
            Some(directorio) => format!("{directorio}:/usr/bin:/bin:/usr/local/bin"),
            None => std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string()),
        };
        let version = Command::new("tmux")
            .arg("-V")
            .env_clear()
            .env("PATH", &path)
            .output()
            .ok()
            .filter(|salida| salida.status.success())
            .and_then(|salida| version_de_texto(&String::from_utf8_lossy(&salida.stdout)));
        let Some(version) = version else {
            return omitir("no hay tmux");
        };
        if version < VersionTmux::MINIMA {
            return omitir("tmux es anterior a 3.0");
        }
        // `script` de util-linux (la sintaxis de BSD es otra).
        let util_linux = Command::new("script")
            .arg("--version")
            .output()
            .ok()
            .is_some_and(|salida| String::from_utf8_lossy(&salida.stdout).contains("util-linux"));
        if !util_linux {
            return omitir("falta `script` de util-linux para darle un terminal a tmux");
        }
        // Un directorio corto: el socket de tmux va en TMUX_TMPDIR.
        let base = tempfile::Builder::new()
            .prefix("mtr")
            .tempdir_in("/tmp")
            .or_else(|_| tempfile::Builder::new().prefix("mtr").tempdir())
            .ok()?;
        let home = base.path().join("h");
        let tmpdir = base.path().join("t");
        fs::create_dir_all(&home).ok()?;
        fs::create_dir_all(&tmpdir).ok()?;
        let script = base.path().join("terminal_tmux.sh");
        fs::write(&script, SCRIPT).ok()?;
        Some(Self {
            base,
            home,
            tmpdir,
            path,
            script,
            version,
        })
    }

    /// El entorno de todo lo que se lanza: limpio, sin `TMUX` y con un
    /// `HOME` y un `TMUX_TMPDIR` propios.
    fn entorno<'a>(&self, comando: &'a mut Command) -> &'a mut Command {
        comando
            .env_clear()
            .env("PATH", &self.path)
            .env("HOME", &self.home)
            .env("TMUX_TMPDIR", &self.tmpdir)
            .env("SHELL", "/bin/sh")
            .env("TERM", "xterm-256color")
            .env("LANG", "C.UTF-8")
    }

    /// Ejecuta el script con `sh -c`, como el servidor por un canal `exec`.
    fn script(&self, argumentos: &[&str]) -> Salida {
        self.script_con(argumentos, &[])
    }

    fn script_con(&self, argumentos: &[&str], entorno: &[(&str, &str)]) -> Salida {
        let mut comando = Command::new("/bin/sh");
        comando.arg("-c").arg(SCRIPT).arg("magi-tmux");
        for argumento in argumentos {
            comando.arg(argumento);
        }
        self.entorno(&mut comando);
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

    /// Ejecuta el tmux real contra un servidor (`Some("magi")` con `-L`).
    fn tmux(&self, socket: Option<&str>, argumentos: &[&str]) -> Salida {
        let mut comando = Command::new("tmux");
        if let Some(socket) = socket {
            comando.args(["-L", socket]);
        }
        comando.args(argumentos);
        self.entorno(&mut comando);
        let salida = comando.output().expect("ejecutar tmux");
        Salida {
            stdout: String::from_utf8_lossy(&salida.stdout).to_string(),
            stderr: String::from_utf8_lossy(&salida.stderr).to_string(),
            codigo: salida.status.code().unwrap_or(-1),
        }
    }

    /// Lanza `enganchar` con un terminal de verdad (`script`) y la entrada
    /// abierta: el cliente queda enganchado hasta que lo suelten.
    fn cliente(&self, argumentos: &[&str]) -> Cliente {
        let orden = format!("sh {} {}", self.script.display(), argumentos.join(" "));
        let mut comando = Command::new("script");
        comando.args(["-qefc", &orden, "/dev/null"]);
        self.entorno(&mut comando);
        let mut hijo = comando
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("lanzar script");
        let entrada = hijo.stdin.take().expect("entrada de script");
        let mut salida_hijo = hijo.stdout.take().expect("salida de script");
        let texto = Arc::new(Mutex::new(Vec::<u8>::new()));
        let copia = Arc::clone(&texto);
        std::thread::spawn(move || {
            let mut bloque = [0u8; 4096];
            while let Ok(leidos) = salida_hijo.read(&mut bloque) {
                if leidos == 0 {
                    break;
                }
                if let Ok(mut texto) = copia.lock() {
                    texto.extend_from_slice(&bloque[..leidos]);
                }
            }
        });
        Cliente {
            hijo,
            _entrada: entrada,
            texto,
        }
    }

    /// Las sesiones de los dos servidores, tal como las ve `estado`.
    fn estado(&self) -> (Option<VersionTmux>, Vec<SesionVista>) {
        let salida = self.script(&["estado"]);
        assert_eq!(salida.codigo, 0, "estado: {}", salida.stderr);
        let lectura = leer_estado(&salida.stdout).expect("estado legible");
        (lectura.tmux, lectura.sesiones)
    }

    fn sesion(&self, servidor: ServidorTmux, nombre: &str) -> Option<SesionVista> {
        self.estado()
            .1
            .into_iter()
            .find(|sesion| sesion.servidor == servidor && sesion.nombre == nombre)
    }

    fn crear(&self, nombre: &str, carpeta: &str, comando: &str) -> Salida {
        self.script(&["crear", nombre, carpeta, comando, "5000", "100", "30"])
    }

    /// Espera a que se cumpla la condición; falla con `motivo` si no.
    fn esperar(&self, motivo: &str, condicion: impl Fn(&Self) -> bool) {
        let limite = Instant::now() + PLAZO;
        while Instant::now() < limite {
            if condicion(self) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("se agotó el plazo esperando: {motivo}");
    }

    /// Espera a que el cliente enganchado cuente: `n` clientes en la sesión.
    fn esperar_clientes(&self, nombre: &str, n: u32) {
        self.esperar(&format!("{n} clientes en {nombre}"), |real| {
            real.sesion(ServidorTmux::Magi, nombre)
                .is_some_and(|sesion| sesion.clientes == n)
        });
    }

    /// Escribe en la sesión como si se tecleara.
    fn teclear(&self, nombre: &str, texto: &str) {
        let destino = format!("={nombre}:");
        let salida = self.tmux(Some("magi"), &["send-keys", "-t", &destino, texto, "Enter"]);
        assert_eq!(salida.codigo, 0, "send-keys: {}", salida.stderr);
    }

    /// Espera a que la vista previa de la sesión contenga el texto.
    fn esperar_en_vista(&self, nombre: &str, texto: &str) -> String {
        let limite = Instant::now() + PLAZO;
        let mut ultima = String::new();
        while Instant::now() < limite {
            let vista = self.script(&["vista", "magi", nombre, "50"]);
            if vista.codigo == 0 && vista.stdout.contains(texto) {
                return vista.stdout;
            }
            ultima = vista.stdout;
            std::thread::sleep(Duration::from_millis(150));
        }
        panic!("«{texto}» no salió en la vista de {nombre}: {ultima:?}");
    }

    fn opcion_global(&self, opcion: &str, alcance: &str) -> String {
        let salida = self.tmux(Some("magi"), &["show", alcance, "-v", opcion]);
        salida.stdout.trim().to_string()
    }
}

impl Drop for Real {
    fn drop(&mut self) {
        // Los dos servidores de la prueba, nunca el del usuario: TMUX_TMPDIR
        // es el temporal.
        let _ = self.tmux(Some("magi"), &["kill-server"]);
        let _ = self.tmux(None, &["kill-server"]);
        let _ = &self.base;
    }
}

/// Un cliente enganchado en un terminal de `script`.
struct Cliente {
    hijo: Child,
    _entrada: ChildStdin,
    texto: Arc<Mutex<Vec<u8>>>,
}

impl Cliente {
    /// Espera el código con el que sale el script (y, por tanto, el cliente).
    fn esperar_codigo(&mut self, plazo: Duration) -> Option<i32> {
        let limite = Instant::now() + plazo;
        while Instant::now() < limite {
            if let Ok(Some(estado)) = self.hijo.try_wait() {
                return estado.code();
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        None
    }

    #[allow(dead_code)]
    fn salida(&self) -> String {
        let texto = self
            .texto
            .lock()
            .map(|texto| texto.clone())
            .unwrap_or_default();
        String::from_utf8_lossy(&texto).to_string()
    }
}

impl Drop for Cliente {
    fn drop(&mut self) {
        let _ = self.hijo.kill();
        let _ = self.hijo.wait();
    }
}

fn existe(base: &Path, nombre: &str) -> bool {
    base.join(nombre).exists()
}

// ------------------------------------------------------------------ pruebas

#[test]
fn crear_enganchar_soltar_historial_renombrar_y_terminar() {
    let Some(real) = Real::nuevo("crear_enganchar_soltar_historial_renombrar_y_terminar") else {
        return;
    };
    // magi-a-10 viva y ninguna magi-a-1: nunca se confunden.
    let salida = real.crear("magi-a-10", "", "");
    assert_eq!(salida.codigo, 0, "{}{}", salida.stdout, salida.stderr);
    assert_eq!(
        leer_crear(&salida.stdout, Some(salida.codigo)).resultado,
        ResultadoCrear::Creada
    );
    let comprobar = leer_comprobar(&real.script(&["comprobar", "magi", "magi-a-1"]).stdout);
    assert_eq!(comprobar.tmux, Some(real.version));
    assert!(!comprobar.existe, "magi-a-1 no existe: solo magi-a-10");

    let terminar = real.script(&["terminar", "magi", "magi-a-1"]);
    assert_eq!(leer_terminar(&terminar.stdout), ResultadoTerminar::NoExiste);
    assert!(real.sesion(ServidorTmux::Magi, "magi-a-10").is_some());

    let salida = real.crear("magi-a-1", "", "");
    assert_eq!(
        leer_crear(&salida.stdout, Some(salida.codigo)).resultado,
        ResultadoCrear::Creada
    );
    // Crear otra vez la misma: existe (73) y no la toca.
    let repetida = real.crear("magi-a-1", "", "");
    assert_eq!(repetida.codigo, 73);
    assert_eq!(
        leer_crear(&repetida.stdout, Some(73)).resultado,
        ResultadoCrear::Existe
    );
    assert!(leer_comprobar(&real.script(&["comprobar", "magi", "magi-a-1"]).stdout).existe);

    // Sin nadie enganchado, con una ventana y sin equipo.
    let uno = real
        .sesion(ServidorTmux::Magi, "magi-a-1")
        .expect("magi-a-1");
    assert_eq!(uno.clientes, 0);
    assert_eq!(uno.ventanas, 1);
    assert_eq!(uno.equipo, None);
    assert!(uno.creada > 1_700_000_000);

    // El portátil se engancha soltando a los demás y se queda con la sesión.
    let mut portatil = real.cliente(&["enganchar", "magi", "magi-a-1", "1", "portatil"]);
    real.esperar_clientes("magi-a-1", 1);
    let vista = real.sesion(ServidorTmux::Magi, "magi-a-1").unwrap();
    assert_eq!(vista.equipo.as_deref(), Some("portatil"));

    // La sobremesa se engancha soltando: al portátil lo sueltan y su script
    // sale con 100, sin que la sesión termine.
    let mut sobremesa = real.cliente(&["enganchar", "magi", "magi-a-1", "1", "sobremesa"]);
    let codigo = portatil.esperar_codigo(PLAZO).expect("el portátil sale");
    assert_eq!(codigo, 100, "a este cliente lo soltó otro");
    assert_eq!(fin_de_enganchar(codigo), FinEnganchar::Soltada);
    real.esperar("la sobremesa es la única enganchada", |real| {
        real.sesion(ServidorTmux::Magi, "magi-a-1")
            .is_some_and(|sesion| {
                sesion.clientes == 1 && sesion.equipo.as_deref() == Some("sobremesa")
            })
    });

    // Compartir: un tercer cliente entra sin soltar a nadie.
    let mut invitado = real.cliente(&["enganchar", "magi", "magi-a-1", "0", "web-01"]);
    real.esperar_clientes("magi-a-1", 2);
    assert!(sobremesa.hijo.try_wait().unwrap().is_none(), "no se soltó");
    assert!(invitado.hijo.try_wait().unwrap().is_none());

    // Historial y vista: lo escrito en la sesión se lee con capture-pane.
    real.teclear("magi-a-1", "echo hola-historial-uno");
    real.teclear("magi-a-1", "printf '\\033[32mverde\\033[0m\\n'");
    let vista = real.esperar_en_vista("magi-a-1", "hola-historial-uno");
    assert!(!vista.contains('\u{1b}'), "la vista es texto plano");
    assert!(!vista.lines().any(|linea| linea.ends_with(' ')));
    let historial = real.script(&["historial", "magi", "magi-a-1", "1000"]);
    assert_eq!(historial.codigo, 0, "{}", historial.stderr);
    assert!(historial.stdout.contains("hola-historial-uno"));
    // Con los colores en bruto (tmux 3.4 y 3.5 escapan los controles de
    // list-sessions, pero no los de capture-pane).
    real.esperar_en_vista("magi-a-1", "verde");
    let historial = real.script(&["historial", "magi", "magi-a-1", "1000"]);
    assert!(
        historial.stdout.contains("\u{1b}[32mverde"),
        "el historial no lleva el color en bruto: {:?}",
        historial.stdout
    );

    // La atención y el título se leen del host.
    let titulo = "printf '\\033]2;mi titulo\\033\\\\'";
    real.teclear("magi-a-1", titulo);
    real.esperar("el título llega a estado", |real| {
        real.sesion(ServidorTmux::Magi, "magi-a-1")
            .is_some_and(|sesion| sesion.titulo.as_deref() == Some("mi titulo"))
    });

    // Renombrar: ocupado, inexistente y bien; las pestañas siguen enganchadas.
    let ocupado = real.script(&["renombrar", "magi-a-1", "magi-a-10"]);
    assert_eq!(leer_renombrar(&ocupado.stdout), ResultadoRenombrar::Ocupado);
    let fantasma = real.script(&["renombrar", "magi-a-2", "otra"]);
    assert_eq!(
        leer_renombrar(&fantasma.stdout),
        ResultadoRenombrar::NoExiste
    );
    let bien = real.script(&["renombrar", "magi-a-1", "revision"]);
    assert_eq!(leer_renombrar(&bien.stdout), ResultadoRenombrar::Ok);
    assert!(real.sesion(ServidorTmux::Magi, "magi-a-1").is_none());
    let renombrada = real
        .sesion(ServidorTmux::Magi, "revision")
        .expect("renombrada");
    assert_eq!(renombrada.clientes, 2, "las pestañas siguen enganchadas");
    assert!(sobremesa.hijo.try_wait().unwrap().is_none());

    // Terminar: el script de los enganchados sale con 0 (la sesión terminó).
    let terminada = real.script(&["terminar", "magi", "revision"]);
    assert_eq!(leer_terminar(&terminada.stdout), ResultadoTerminar::Ok);
    assert_eq!(sobremesa.esperar_codigo(PLAZO), Some(0));
    assert_eq!(invitado.esperar_codigo(PLAZO), Some(0));
    assert_eq!(fin_de_enganchar(0), FinEnganchar::Terminada);
    // Y la otra, la magi-a-10, sigue viva.
    assert!(real.sesion(ServidorTmux::Magi, "magi-a-10").is_some());
    // Engancharse a una que ya no existe: 75.
    let salida = real.script(&["enganchar", "magi", "revision", "1", "portatil"]);
    assert_eq!(salida.codigo, 75);
}

#[test]
fn magi_a_1_nunca_toca_a_magi_a_10() {
    let Some(real) = Real::nuevo("magi_a_1_nunca_toca_a_magi_a_10") else {
        return;
    };
    assert_eq!(real.crear("magi-a-10", "", "").codigo, 0);
    // tmux casa prefijos con -t: ninguna suborden puede llegar a magi-a-10.
    let terminar = real.script(&["terminar", "magi", "magi-a-1"]);
    assert_eq!(terminar.stdout, "MAGI_RESULTADO=no_existe\n");
    let renombrar = real.script(&["renombrar", "magi-a-1", "otra"]);
    assert_eq!(renombrar.stdout, "MAGI_RESULTADO=no_existe\n");
    for suborden in ["vista", "historial"] {
        let salida = real.script(&[suborden, "magi", "magi-a-1", "10"]);
        assert_eq!(salida.codigo, 75, "{suborden}");
        assert!(salida.stdout.is_empty(), "{suborden}: {}", salida.stdout);
    }
    let enganchar = real.script(&["enganchar", "magi", "magi-a-1", "1", "portatil"]);
    assert_eq!(enganchar.codigo, 75);
    let comprobar = leer_comprobar(&real.script(&["comprobar", "magi", "magi-a-1"]).stdout);
    assert!(!comprobar.existe);
    // Con la forma antigua del script (legado) ocurre lo mismo.
    let antigua = real.script(&["terminar", "magi-a-1"]);
    assert_eq!(antigua.stdout, "MAGI_RESULTADO=no_existe\n");
    let sesion = real
        .sesion(ServidorTmux::Magi, "magi-a-10")
        .expect("sigue viva");
    assert_eq!(sesion.equipo, None, "nadie le puso equipo");
    // Y al revés: terminar magi-a-10 no mata una magi-a-1 que sí existe.
    assert_eq!(real.crear("magi-a-1", "", "").codigo, 0);
    let terminada = real.script(&["terminar", "magi", "magi-a-10"]);
    assert_eq!(leer_terminar(&terminada.stdout), ResultadoTerminar::Ok);
    assert!(real.sesion(ServidorTmux::Magi, "magi-a-1").is_some());
    assert!(real.sesion(ServidorTmux::Magi, "magi-a-10").is_none());
}

#[test]
fn el_comando_deja_un_shell_en_la_carpeta_con_el_codigo_de_salida() {
    let Some(real) = Real::nuevo("el_comando_deja_un_shell_en_la_carpeta_con_el_codigo_de_salida")
    else {
        return;
    };
    fs::create_dir_all(real.home.join("proyectos/api")).unwrap();
    // AC: con comando = false se ve «terminó con código 1» y queda un shell.
    let salida = real.crear("falso", "~/proyectos/api", "false");
    assert_eq!(salida.codigo, 0, "{}{}", salida.stdout, salida.stderr);
    let vista = real.esperar_en_vista("falso", "terminó con código 1");
    assert!(
        vista.contains("[MAGI] false terminó con código 1"),
        "{vista}"
    );
    // El shell que queda está en la carpeta (la `~/` se cambió por $HOME/).
    real.teclear("falso", "pwd");
    let carpeta = format!("{}/proyectos/api", real.home.display());
    real.esperar_en_vista("falso", &carpeta);
    real.teclear("falso", "echo sigue-vivo-$((20+22))");
    real.esperar_en_vista("falso", "sigue-vivo-42");

    // Con éxito y con otro código; el comando se ejecuta con un shell, de modo
    // que sus caracteres especiales significan lo de siempre (es el comando).
    let salida = real.crear("bien", "", "echo \"uno $((1+1))\"; exit 0");
    assert_eq!(salida.codigo, 0);
    real.esperar_en_vista("bien", "uno 2");
    real.esperar_en_vista("bien", "terminó con código 0");
    let salida = real.crear("tres", "", "exit 3");
    assert_eq!(salida.codigo, 0);
    real.esperar_en_vista("tres", "terminó con código 3");

    // Sin comando, es el shell de acceso.
    assert_eq!(real.crear("libre", "", "").codigo, 0);
    real.teclear("libre", "echo libre-$((6*7))");
    real.esperar_en_vista("libre", "libre-42");

    // Una carpeta que no existe no crea nada.
    let sin = real.crear("nada", "/no/existe/nunca", "");
    assert_eq!(sin.codigo, 72);
    assert!(real.sesion(ServidorTmux::Magi, "nada").is_none());
}

#[test]
fn el_comando_va_como_dato_y_nunca_se_interpola_en_el_envoltorio() {
    let Some(real) = Real::nuevo("el_comando_va_como_dato_y_nunca_se_interpola_en_el_envoltorio")
    else {
        return;
    };
    // Un comando que intenta cerrar el envoltorio y ejecutar otra cosa: solo
    // se ejecuta como el comando del usuario (su texto se ve tal cual).
    let marca = real.base.path().join("inyectado");
    let comando = format!(
        "echo '\"; touch {} #' && echo fin-del-comando",
        marca.display()
    );
    let salida = real.crear("raro", "", &comando);
    assert_eq!(salida.codigo, 0, "{}", salida.stderr);
    let vista = real.esperar_en_vista("raro", "fin-del-comando");
    assert!(vista.contains("terminó con código 0"), "{vista}");
    assert!(!existe(real.base.path(), "inyectado"));
}

#[test]
fn no_carga_el_tmux_conf_del_host_ni_toca_el_tmux_habitual() {
    let Some(real) = Real::nuevo("no_carga_el_tmux_conf_del_host_ni_toca_el_tmux_habitual") else {
        return;
    };
    // El host tiene su propia configuración, que deja una marca al cargarse.
    let marca = real.base.path().join("conf-cargada");
    let conf = format!(
        "set -g status on\nset -g prefix C-a\nset -g mouse off\nrun-shell 'touch {}'\n",
        marca.display()
    );
    fs::write(real.home.join(".tmux.conf"), &conf).unwrap();
    fs::create_dir_all(real.home.join(".config/tmux")).unwrap();
    fs::write(real.home.join(".config/tmux/tmux.conf"), &conf).unwrap();

    // El tmux habitual del usuario, con una sesión suya.
    let suya = real.tmux(
        None,
        &["new-session", "-d", "-s", "suya", "-x", "80", "-y", "24"],
    );
    assert_eq!(suya.codigo, 0, "{}", suya.stderr);
    real.esperar("el tmux habitual carga su configuración", |real| {
        existe(real.base.path(), "conf-cargada")
    });
    let opciones_antes = real.tmux(None, &["show", "-g"]).stdout;
    let sesiones_antes = real
        .tmux(
            None,
            &["list-sessions", "-F", "#{session_name} #{session_created}"],
        )
        .stdout;
    assert!(opciones_antes.contains("prefix C-a"));
    fs::remove_file(&marca).unwrap();

    // MAGI crea y engancha una sesión suya: nada de eso se carga.
    let creada = real.crear("agente", "", "");
    assert_eq!(creada.codigo, 0, "{}{}", creada.stdout, creada.stderr);
    let mut cliente = real.cliente(&["enganchar", "magi", "agente", "1", "portatil"]);
    real.esperar_clientes("agente", 1);
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        !existe(real.base.path(), "conf-cargada"),
        "se cargó ~/.tmux.conf"
    );
    // La configuración de MAGI es la suya, no la del usuario.
    assert_eq!(real.opcion_global("status", "-g"), "off");
    assert_eq!(real.opcion_global("prefix", "-g"), "None");
    assert_eq!(real.opcion_global("mouse", "-g"), "on");

    // El tmux habitual sigue exactamente igual.
    assert_eq!(real.tmux(None, &["show", "-g"]).stdout, opciones_antes);
    assert_eq!(
        real.tmux(
            None,
            &["list-sessions", "-F", "#{session_name} #{session_created}"]
        )
        .stdout,
        sesiones_antes
    );
    // Y las dos listas son de servidores distintos.
    let (_, sesiones) = real.estado();
    assert!(sesiones
        .iter()
        .any(|s| s.servidor == ServidorTmux::Habitual && s.nombre == "suya"));
    assert!(sesiones
        .iter()
        .any(|s| s.servidor == ServidorTmux::Magi && s.nombre == "agente"));
    assert_eq!(sesiones.len(), 2);

    // Engancharse a la del usuario no le cambia nada: ni opciones ni equipo.
    let mut suya_cliente = real.cliente(&["enganchar", "habitual", "suya", "1", "portatil"]);
    real.esperar("el cliente se engancha a la sesión habitual", |real| {
        real.sesion(ServidorTmux::Habitual, "suya")
            .is_some_and(|sesion| sesion.clientes == 1)
    });
    assert_eq!(real.tmux(None, &["show", "-g"]).stdout, opciones_antes);
    let equipo = real.tmux(None, &["show", "-t", "=suya:", "-v", "@magi_equipo"]);
    assert!(equipo.stdout.trim().is_empty(), "{}", equipo.stdout);
    assert_eq!(
        real.sesion(ServidorTmux::Habitual, "suya").unwrap().equipo,
        None
    );
    // Terminarla desde MAGI es a petición del usuario y funciona.
    let terminada = real.script(&["terminar", "habitual", "suya"]);
    assert_eq!(leer_terminar(&terminada.stdout), ResultadoTerminar::Ok);
    assert!(suya_cliente.esperar_codigo(PLAZO).is_some());
    drop(cliente.hijo.kill());
}

#[test]
fn la_configuracion_de_magi_se_aplica_con_cada_version_de_tmux() {
    let Some(real) = Real::nuevo("la_configuracion_de_magi_se_aplica_con_cada_version_de_tmux")
    else {
        return;
    };
    let salida = real.script(&["crear", "conf", "", "", "7777", "100", "30"]);
    assert_eq!(salida.codigo, 0, "{}{}", salida.stdout, salida.stderr);
    let v = real.version;
    // Siempre.
    assert_eq!(real.opcion_global("status", "-g"), "off");
    assert_eq!(real.opcion_global("prefix", "-g"), "None");
    assert_eq!(real.opcion_global("prefix2", "-g"), "None");
    assert_eq!(real.opcion_global("mouse", "-g"), "on");
    assert_eq!(real.opcion_global("history-limit", "-g"), "7777");
    assert_eq!(real.opcion_global("set-titles", "-g"), "on");
    assert_eq!(real.opcion_global("set-titles-string", "-g"), "#T");
    assert_eq!(real.opcion_global("monitor-bell", "-g"), "on");
    assert_eq!(real.opcion_global("bell-action", "-g"), "any");
    assert_eq!(real.opcion_global("set-clipboard", "-s"), "on");
    assert_eq!(real.opcion_global("focus-events", "-s"), "on");
    assert_eq!(real.opcion_global("escape-time", "-s"), "10");
    let terminal = real.opcion_global("default-terminal", "-s");
    assert!(
        terminal == "tmux-256color" || terminal == "screen-256color",
        "{terminal}"
    );
    // Según la versión.
    if v >= VersionTmux::nueva(3, 2) {
        assert_eq!(real.opcion_global("extended-keys", "-s"), "on");
        let caracteristicas = real
            .tmux(Some("magi"), &["show", "-s", "terminal-features"])
            .stdout;
        assert!(caracteristicas.contains("extkeys"), "{caracteristicas}");
        assert!(caracteristicas.contains("RGB"), "{caracteristicas}");
    }
    if v >= VersionTmux::nueva(3, 5) {
        assert_eq!(real.opcion_global("extended-keys-format", "-s"), "csi-u");
    }
    if v >= VersionTmux::nueva(3, 3) {
        assert_eq!(real.opcion_global("allow-passthrough", "-g"), "on");
    }
    if v >= VersionTmux::nueva(3, 1) {
        assert_eq!(real.opcion_global("window-size", "-g"), "latest");
    }
    // Aplicarla otra vez al enganchar es idempotente y no falla.
    let mut cliente = real.cliente(&["enganchar", "magi", "conf", "1", "portatil"]);
    real.esperar_clientes("conf", 1);
    assert_eq!(real.opcion_global("status", "-g"), "off");
    assert_eq!(real.opcion_global("history-limit", "-g"), "7777");
    if v >= VersionTmux::nueva(3, 2) {
        let caracteristicas = real
            .tmux(Some("magi"), &["show", "-s", "terminal-features"])
            .stdout;
        assert_eq!(
            caracteristicas.matches("extkeys").count(),
            1,
            "{caracteristicas}"
        );
    }
    let _ = cliente.hijo.kill();
}

#[test]
fn estado_lee_actividad_campana_titulo_y_hora_del_host_de_verdad() {
    let Some(real) = Real::nuevo("estado_lee_actividad_campana_titulo_y_hora_del_host_de_verdad")
    else {
        return;
    };
    assert_eq!(real.crear("libre", "", "").codigo, 0);
    assert_eq!(real.crear("avisa", "", "").codigo, 0);
    // Una sesión del tmux habitual también sale, con su propio servidor.
    let suya = real.tmux(
        None,
        &["new-session", "-d", "-s", "suya", "-x", "80", "-y", "24"],
    );
    assert_eq!(suya.codigo, 0, "{}", suya.stderr);

    // La campana sin nadie enganchado marca la sesión (D335).
    real.teclear("avisa", "printf '\\a'");
    real.esperar("la campana llega a estado", |real| {
        real.sesion(ServidorTmux::Magi, "avisa")
            .is_some_and(|sesion| sesion.campana)
    });
    assert!(!real.sesion(ServidorTmux::Magi, "libre").unwrap().campana);

    let salida = real.script(&["estado"]);
    let lectura = leer_estado(&salida.stdout).expect("estado legible");
    assert_eq!(lectura.tmux, Some(real.version));
    let reloj = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    assert!((lectura.ahora - reloj).abs() < 60, "hora del host");
    let avisa = lectura
        .sesiones
        .iter()
        .find(|s| s.nombre == "avisa")
        .unwrap();
    assert!(avisa.actividad > 0 && avisa.actividad <= lectura.ahora + 2);
    assert!(avisa.creada > 0 && avisa.creada <= avisa.actividad);
    assert_eq!(lectura.sesiones[0].servidor, ServidorTmux::Magi);
    let suya = lectura
        .sesiones
        .iter()
        .find(|s| s.nombre == "suya")
        .unwrap();
    assert_eq!(suya.servidor, ServidorTmux::Habitual);
    assert_eq!(suya.equipo, None);

    // Un título con caracteres de control llega saneado.
    real.teclear("libre", "printf '\\033]2;t\\001\\002itulo\\033\\\\'");
    real.esperar("el título llega a estado", |real| {
        real.sesion(ServidorTmux::Magi, "libre")
            .and_then(|sesion| sesion.titulo)
            .is_some_and(|titulo| !titulo.chars().any(char::is_control))
    });
}

#[test]
fn con_la_variable_tmux_puesta_el_script_sigue_eligiendo_su_servidor() {
    let Some(real) =
        Real::nuevo("con_la_variable_tmux_puesta_el_script_sigue_eligiendo_su_servidor")
    else {
        return;
    };
    // Un shell que arranca tmux por su cuenta deja TMUX en el entorno: sin
    // quitarla, tmux se niega a engancharse y los comandos van a otro socket.
    let ajena = format!("{}/tmux-0/ajeno,1,0", real.tmpdir.display());
    let creada = real.script_con(
        &["crear", "anidada", "", "", "5000", "100", "30"],
        &[("TMUX", &ajena)],
    );
    assert_eq!(creada.codigo, 0, "{}{}", creada.stdout, creada.stderr);
    assert!(real.sesion(ServidorTmux::Magi, "anidada").is_some());
    let orden = format!(
        "env TMUX={ajena} sh {} enganchar magi anidada 1 portatil",
        real.script.display()
    );
    let mut comando = Command::new("script");
    comando.args(["-qefc", &orden, "/dev/null"]);
    real.entorno(&mut comando);
    let mut hijo = comando
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("lanzar script");
    real.esperar_clientes("anidada", 1);
    let _ = hijo.kill();
    let _ = hijo.wait();
}

#[test]
fn estado_funciona_con_un_entorno_que_no_dice_utf8() {
    let Some(real) = Real::nuevo("estado_funciona_con_un_entorno_que_no_dice_utf8") else {
        return;
    };
    assert_eq!(real.crear("libre", "", "").codigo, 0);
    let suya = real.tmux(
        None,
        &["new-session", "-d", "-s", "suya", "-x", "80", "-y", "24"],
    );
    assert_eq!(suya.codigo, 0, "{}", suya.stderr);
    real.teclear("libre", "printf '\\033]2;claude · api\\033\\\\'");
    // Como lo ejecuta un canal exec de ssh en un host sin LANG: tmux sin -u
    // cambia por «_» el tabulador y el 0x1F, y `estado` no ve ni una sesión.
    let sin_utf8 = [("LANG", "C"), ("LC_ALL", "C")];
    real.esperar("estado ve las sesiones y el título sin UTF-8", |real| {
        let salida = real.script_con(&["estado"], &sin_utf8);
        leer_estado(&salida.stdout).is_some_and(|lectura| {
            lectura.sesiones.len() == 2
                && lectura.sesiones.iter().any(|sesion| {
                    sesion.servidor == ServidorTmux::Magi
                        && sesion.nombre == "libre"
                        && sesion.titulo.as_deref() == Some("claude · api")
                })
                && lectura
                    .sesiones
                    .iter()
                    .any(|sesion| sesion.servidor == ServidorTmux::Habitual)
        })
    });
}

#[test]
fn un_comando_y_una_carpeta_que_acaban_en_punto_y_coma_llegan_enteros() {
    let Some(real) =
        Real::nuevo("un_comando_y_una_carpeta_que_acaban_en_punto_y_coma_llegan_enteros")
    else {
        return;
    };
    // tmux toma un argumento que acaba en «;» como el fin de la orden y se lo
    // quita (con «\;», deja solo el «;»): el comando y la carpeta del usuario
    // son texto suyo y tienen que llegar tal cual.
    fs::create_dir_all(real.home.join("datos;")).unwrap();
    let salida = real.crear("find", "", r"find . -maxdepth 0 -exec echo ok-find {} \;");
    assert_eq!(salida.codigo, 0, "{}{}", salida.stdout, salida.stderr);
    let vista = real.esperar_en_vista("find", "terminó con código");
    assert!(vista.contains("ok-find ."), "{vista}");
    assert!(vista.contains("terminó con código 0"), "{vista}");
    assert!(!vista.contains("missing argument"), "{vista}");

    let salida = real.crear("barra", "", r"echo uno\;");
    assert_eq!(salida.codigo, 0, "{}{}", salida.stdout, salida.stderr);
    let vista = real.esperar_en_vista("barra", "terminó con código");
    assert!(vista.contains(r"[MAGI] echo uno\;"), "{vista}");
    let salida = real.crear("punto", "", "echo dos;");
    assert_eq!(salida.codigo, 0, "{}{}", salida.stdout, salida.stderr);
    let vista = real.esperar_en_vista("punto", "terminó con código");
    assert!(vista.contains("[MAGI] echo dos;"), "{vista}");

    let salida = real.crear("carpeta", "~/datos;", "");
    assert_eq!(salida.codigo, 0, "{}{}", salida.stdout, salida.stderr);
    let esperada = format!("{}/datos;", real.home.display());
    real.esperar("la sesión arranca en la carpeta con «;»", |real| {
        real.tmux(
            Some("magi"),
            &[
                "display-message",
                "-p",
                "-t",
                "=carpeta:",
                "#{pane_current_path}",
            ],
        )
        .stdout
        .trim()
            == esperada
    });
}

#[test]
fn una_carpeta_con_almohadillas_no_se_expande_como_formato_de_tmux() {
    let Some(real) = Real::nuevo("una_carpeta_con_almohadillas_no_se_expande_como_formato_de_tmux")
    else {
        return;
    };
    // tmux expande formatos en `-c`: «#D» es el id del panel, «##» es «#» y
    // «#(orden)» ejecuta la orden. La carpeta es texto del usuario y tiene que
    // llegar tal cual.
    for (indice, nombre) in ["#Docs", "a##b", "a#(true)b", "c#", "#{session_name}"]
        .into_iter()
        .enumerate()
    {
        let carpeta = real.home.join(nombre);
        fs::create_dir_all(&carpeta).unwrap();
        let sesion = format!("almohadilla-{indice}");
        let salida = real.crear(&sesion, &carpeta.display().to_string(), "");
        assert_eq!(
            salida.codigo, 0,
            "{nombre}: {}{}",
            salida.stdout, salida.stderr
        );
        let esperada = carpeta.display().to_string();
        let destino = format!("={sesion}:");
        real.esperar(&format!("la sesión arranca en «{nombre}»"), |real| {
            real.tmux(
                Some("magi"),
                &[
                    "display-message",
                    "-p",
                    "-t",
                    &destino,
                    "#{pane_current_path}",
                ],
            )
            .stdout
            .trim()
                == esperada
        });
    }
}

#[test]
fn el_legado_de_la_fase_15_sigue_funcionando_con_tmux_real() {
    // LEGADO-S2: el servidor del escritorio usa `comprobar <nombre>`,
    // `listar` y `terminar <nombre>` (tmux habitual) hasta el Sprint 2.
    let Some(real) = Real::nuevo("el_legado_de_la_fase_15_sigue_funcionando_con_tmux_real") else {
        return;
    };
    let uno = real.tmux(
        None,
        &[
            "new-session",
            "-d",
            "-s",
            "magi-x-10",
            "-x",
            "80",
            "-y",
            "24",
        ],
    );
    assert_eq!(uno.codigo, 0, "{}", uno.stderr);
    let comprobar = real.script(&["comprobar", "magi-x-10"]);
    assert!(
        comprobar.stdout.contains("MAGI_SESION=existe\n"),
        "{}",
        comprobar.stdout
    );
    let otra = real.script(&["comprobar", "magi-x-1"]);
    assert!(
        otra.stdout.contains("MAGI_SESION=nueva\n"),
        "{}",
        otra.stdout
    );
    // El separador de tmux sobrevive a 3.4 y 3.5: la sesión sale con sus campos.
    let listar = real.script(&["listar"]);
    let listado = magi_nucleo::terminal::tmux::leer_listado(&listar.stdout);
    assert_eq!(listado.sesiones.len(), 1, "{}", listar.stdout);
    assert_eq!(listado.sesiones[0].nombre, "magi-x-10");
    assert_eq!(listado.sesiones[0].ventanas, 1);
    assert!(!listado.sesiones[0].enganchada);
    // R146: terminar magi-x-1 (que no existe) no mata a magi-x-10.
    let terminar = real.script(&["terminar", "magi-x-1"]);
    assert_eq!(terminar.stdout, "MAGI_RESULTADO=no_existe\n");
    assert_eq!(
        real.script(&["terminar", "magi-x-10"]).stdout,
        "MAGI_RESULTADO=ok\n"
    );
}
