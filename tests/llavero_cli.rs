//! Pruebas de `magi llavero` y del llavero de las demás órdenes de la CLI
//! (Fase 17, Termux): el binario real, el flujo con la consola abstraída y un
//! terminal de verdad (`script` de util-linux).
//!
//! Sin red, sin el llavero del sistema y sin tocar el HOME real: todo en un
//! directorio temporal con `HOME`, `XDG_*`, `PREFIX` y `TMPDIR` propios. El
//! llavero de fichero de las pruebas usa un factor de scrypt bajo; el binario
//! solo usa el de producción (2^17) cuando recifra (el camino feliz de
//! `cambiar` con terminal), que en depuración es rápido porque `Cargo.toml`
//! optimiza solo `scrypt`.
//!
//! El binario solo se porta como Termux en compilaciones de depuración con
//! `MAGI_PLATAFORMA=termux`: con `cargo test --release` esas pruebas se
//! omiten con un aviso.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::io::{Read as _, Write as _};
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use zeroize::Zeroizing;

use magi::config::Rutas;
use magi::llavero::{EstadoLlavero, LlaveroCliente, LlaveroEnMemoria, LlaveroSecreto};
use magi::llavero_cli::{
    algun_host_usa_llavero, aplicar_evento, desbloquear_para_cli, orden_borrar, orden_cambiar,
    orden_estado, Consola, ErrorEntrada, Paso, INTENTOS_FRASE, TOPE_LECTURA,
};
use magi::modelo::{DatosHost, Host, IdentidadRef, Origen};
use magi_nucleo::llavero_fichero::{ErrorLlavero, LlaveroFichero, MAX_FRASE_BYTES, NOMBRE_FICHERO};

const FRASE_VIEJA: &str = "la frase vieja de las pruebas";
const FRASE_NUEVA: &str = "otra frase nueva de las pruebas";
const FRASE_ERRONEA: &str = "una frase que no es la buena";
/// Factor de scrypt de las pruebas (2^10): milisegundos incluso en depuración.
const LOG_N_PRUEBA: u8 = 10;
const CUENTA: &str = "usuario@192.0.2.10";
const SECRETO: &str = "contraseña de prueba";

// ------------------------------------------------------------------ el banco

/// Un equipo de pruebas: rutas aisladas en un directorio temporal y un reloj
/// que las pruebas pueden adelantar.
struct Banco {
    _raiz: tempfile::TempDir,
    rutas: Rutas,
    xdg_datos: PathBuf,
    xdg_config: PathBuf,
    xdg_estado: PathBuf,
    home: PathBuf,
    prefix: PathBuf,
    tmp: PathBuf,
    runtime: PathBuf,
    /// Segundos que el reloj del llavero va por delante de «ahora».
    adelanto: Arc<AtomicU64>,
}

impl Banco {
    fn nuevo() -> Self {
        let raiz = tempfile::Builder::new()
            .prefix("mlc")
            .tempdir()
            .expect("directorio temporal");
        let base = raiz.path().to_path_buf();
        let xdg_datos = base.join("xd");
        let xdg_config = base.join("xc");
        let xdg_estado = base.join("xe");
        let home = base.join("h");
        let prefix = base.join("p");
        let tmp = base.join("t");
        let runtime = base.join("r");
        for directorio in [
            &xdg_datos,
            &xdg_config,
            &xdg_estado,
            &home,
            &prefix,
            &tmp,
            &runtime,
        ] {
            std::fs::create_dir_all(directorio).expect("directorio de prueba");
            std::fs::set_permissions(directorio, std::fs::Permissions::from_mode(0o700))
                .expect("permisos del directorio de prueba");
        }
        // Como en Termux (D383): los directorios de MAGI cuelgan de XDG_*.
        let rutas = Rutas::nuevas(
            xdg_datos.join("magi"),
            xdg_config.join("magi"),
            xdg_estado.join("magi"),
            home.clone(),
            runtime.join("magi"),
        );
        std::fs::create_dir_all(&rutas.datos).expect("directorio de datos");
        std::fs::set_permissions(&rutas.datos, std::fs::Permissions::from_mode(0o700))
            .expect("permisos de datos");
        Self {
            _raiz: raiz,
            rutas,
            xdg_datos,
            xdg_config,
            xdg_estado,
            home,
            prefix,
            tmp,
            runtime,
            adelanto: Arc::new(AtomicU64::new(0)),
        }
    }

    fn ruta_llavero(&self) -> PathBuf {
        self.rutas.datos.join(NOMBRE_FICHERO)
    }

    /// Un objeto de llavero nuevo sobre el fichero del banco: como un proceso
    /// recién arrancado (cerrado si hay fichero) y con el reloj del banco.
    fn fichero(&self) -> LlaveroFichero {
        let base = Instant::now();
        let adelanto = self.adelanto.clone();
        LlaveroFichero::con_ajustes(
            self.ruta_llavero(),
            LOG_N_PRUEBA,
            Arc::new(move || base + Duration::from_secs(adelanto.load(Ordering::SeqCst))),
        )
    }

    /// Crea el llavero con esa frase y un secreto dentro, y lo cierra.
    fn crear(&self, frase: &str) {
        let llavero = self.fichero();
        llavero.crear(frase, frase).expect("crear el llavero");
        LlaveroSecreto::guardar(&llavero, CUENTA, SECRETO).expect("guardar el secreto");
        llavero.bloquear();
    }

    fn bytes_llavero(&self) -> Option<Vec<u8>> {
        std::fs::read(self.ruta_llavero()).ok()
    }

    /// `(tipo, resultado)` de las filas `llavero_*` de REGISTRO; vacío si ni
    /// siquiera hay base de datos.
    fn registro(&self) -> Vec<(String, String)> {
        let base = self.rutas.base_datos();
        if !base.exists() {
            return Vec::new();
        }
        let almacen = magi::almacen::abrir(&base).expect("abrir la base de datos");
        let mut consulta = almacen
            .conexion()
            .prepare("SELECT tipo, resultado FROM REGISTRO WHERE tipo LIKE 'llavero%' ORDER BY id")
            .expect("consulta del registro");
        let filas = consulta
            .query_map([], |fila| Ok((fila.get(0)?, fila.get(1)?)))
            .expect("filas del registro")
            .collect::<rusqlite::Result<Vec<(String, String)>>>()
            .expect("leer el registro");
        filas
    }

    /// El entorno aislado de este banco: nada del HOME ni de los `XDG_*` reales.
    /// `termux` activa `MAGI_PLATAFORMA=termux`.
    fn entorno(&self, termux: bool) -> Vec<(&'static str, OsString)> {
        let mut entorno: Vec<(&'static str, OsString)> = vec![
            ("PATH", std::env::var_os("PATH").unwrap_or_default()),
            ("HOME", self.home.clone().into()),
            ("XDG_DATA_HOME", self.xdg_datos.clone().into()),
            ("XDG_CONFIG_HOME", self.xdg_config.clone().into()),
            ("XDG_STATE_HOME", self.xdg_estado.clone().into()),
            ("XDG_RUNTIME_DIR", self.runtime.clone().into()),
            ("PREFIX", self.prefix.clone().into()),
            ("TMPDIR", self.tmp.clone().into()),
            ("TZ", "UTC".into()),
            ("LC_ALL", "C.UTF-8".into()),
        ];
        if termux {
            entorno.push(("MAGI_PLATAFORMA", "termux".into()));
        }
        entorno
    }

    /// El binario `magi` con el entorno de este banco.
    fn comando(&self, termux: bool, argumentos: &[&str]) -> Command {
        let mut comando = Command::new(env!("CARGO_BIN_EXE_magi"));
        comando
            .args(argumentos)
            .env_clear()
            .envs(self.entorno(termux));
        comando
    }

    /// Ejecuta el binario sin terminal (entrada nula, salidas capturadas).
    fn ejecutar(&self, termux: bool, argumentos: &[&str]) -> Output {
        self.comando(termux, argumentos)
            .stdin(Stdio::null())
            .output()
            .expect("ejecutar magi")
    }
}

/// El binario solo se porta como Termux en depuración con la variable.
fn hay_modo_termux(prueba: &str) -> bool {
    if cfg!(debug_assertions) {
        return true;
    }
    eprintln!("llavero_cli::{prueba}: se omite, MAGI_PLATAFORMA=termux solo vale en depuración");
    false
}

fn texto(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

// ------------------------------------------------------------------ la consola falsa

/// Una consola de mentira: contesta lo que se le programe y apunta lo que se
/// le dice y se le pregunta.
#[derive(Default)]
struct ConsolaFalsa {
    interactiva: bool,
    ocultas: VecDeque<Result<String, ErrorEntrada>>,
    lineas: VecDeque<Result<String, ErrorEntrada>>,
    preguntas: Vec<String>,
    salida: Vec<String>,
    avisos: Vec<String>,
    /// Lo que ocurre «mientras la persona escribe» la confirmación: se ejecuta
    /// una vez, justo antes de devolver la línea.
    al_leer_linea: Option<Box<dyn FnOnce()>>,
}

impl ConsolaFalsa {
    fn con_terminal() -> Self {
        Self {
            interactiva: true,
            ..Self::default()
        }
    }

    fn sin_terminal() -> Self {
        Self::default()
    }

    fn oculta(mut self, respuesta: &str) -> Self {
        self.ocultas.push_back(Ok(respuesta.to_string()));
        self
    }

    fn oculta_cancelada(mut self) -> Self {
        self.ocultas.push_back(Err(ErrorEntrada::Cancelada));
        self
    }

    fn linea(mut self, respuesta: &str) -> Self {
        self.lineas.push_back(Ok(respuesta.to_string()));
        self
    }

    fn todo(&self) -> String {
        format!("{}\n{}", self.salida.join("\n"), self.avisos.join("\n"))
    }

    fn avisos(&self) -> String {
        self.avisos.join("\n")
    }

    fn salida(&self) -> String {
        self.salida.join("\n")
    }
}

impl Consola for ConsolaFalsa {
    fn es_interactiva(&self) -> bool {
        self.interactiva
    }

    fn leer_oculto(&mut self, pregunta: &str) -> Result<Zeroizing<String>, ErrorEntrada> {
        self.preguntas.push(pregunta.to_string());
        self.ocultas
            .pop_front()
            .expect("se preguntó una frase que la prueba no programó")
            .map(Zeroizing::new)
    }

    fn leer_linea(&mut self, pregunta: &str) -> Result<String, ErrorEntrada> {
        self.preguntas.push(pregunta.to_string());
        if let Some(durante) = self.al_leer_linea.take() {
            durante();
        }
        self.lineas
            .pop_front()
            .expect("se preguntó una línea que la prueba no programó")
    }

    fn decir(&mut self, texto: &str) {
        self.salida.push(texto.to_string());
    }

    fn avisar(&mut self, texto: &str) {
        self.avisos.push(texto.to_string());
    }
}

// ------------------------------------------------------------------ la lectura sin eco (eventos)

fn tecla(codigo: KeyCode) -> Event {
    Event::Key(KeyEvent::new(codigo, KeyModifiers::NONE))
}

fn escribir_texto(buffer: &mut Zeroizing<String>, texto: &str) {
    for caracter in texto.chars() {
        assert_eq!(
            aplicar_evento(buffer, tecla(KeyCode::Char(caracter))),
            Paso::Seguir
        );
    }
}

#[test]
fn la_frase_se_escribe_se_corrige_y_se_acepta_con_intro() {
    let mut buffer = Zeroizing::new(String::new());
    escribir_texto(&mut buffer, "frasex");
    assert_eq!(
        aplicar_evento(&mut buffer, tecla(KeyCode::Backspace)),
        Paso::Seguir
    );
    escribir_texto(&mut buffer, "ñandú");
    // Retroceso quita un carácter entero, no un byte.
    assert_eq!(
        aplicar_evento(&mut buffer, tecla(KeyCode::Backspace)),
        Paso::Seguir
    );
    assert_eq!(buffer.as_str(), "fraseñand");
    assert_eq!(
        aplicar_evento(&mut buffer, tecla(KeyCode::Enter)),
        Paso::Aceptar
    );
    // Retroceso con la frase vacía no hace nada.
    let mut vacio = Zeroizing::new(String::new());
    assert_eq!(
        aplicar_evento(&mut vacio, tecla(KeyCode::Backspace)),
        Paso::Seguir
    );
    assert!(vacio.is_empty());
}

#[test]
fn ctrl_c_y_ctrl_d_cancelan_y_las_demas_combinaciones_no_hacen_nada() {
    let mut buffer = Zeroizing::new(String::new());
    escribir_texto(&mut buffer, "abc");
    for caracter in ['c', 'C', 'd', 'D'] {
        let evento = Event::Key(KeyEvent::new(
            KeyCode::Char(caracter),
            KeyModifiers::CONTROL,
        ));
        assert_eq!(
            aplicar_evento(&mut buffer, evento),
            Paso::Cancelar,
            "Ctrl+{caracter}"
        );
    }
    for caracter in ['a', 'u', 'z', 'j'] {
        let evento = Event::Key(KeyEvent::new(
            KeyCode::Char(caracter),
            KeyModifiers::CONTROL,
        ));
        assert_eq!(
            aplicar_evento(&mut buffer, evento),
            Paso::Seguir,
            "Ctrl+{caracter}"
        );
    }
    // Ni Ctrl+letra ni las teclas sin carácter tocan la frase.
    for codigo in [
        KeyCode::Esc,
        KeyCode::Tab,
        KeyCode::Left,
        KeyCode::Up,
        KeyCode::F(1),
    ] {
        assert_eq!(aplicar_evento(&mut buffer, tecla(codigo)), Paso::Seguir);
    }
    assert_eq!(buffer.as_str(), "abc");
}

#[test]
fn soltar_una_tecla_no_la_repite() {
    let mut buffer = Zeroizing::new(String::new());
    let soltada = Event::Key(KeyEvent::new_with_kind(
        KeyCode::Char('a'),
        KeyModifiers::NONE,
        KeyEventKind::Release,
    ));
    assert_eq!(aplicar_evento(&mut buffer, soltada), Paso::Seguir);
    let intro_soltada = Event::Key(KeyEvent::new_with_kind(
        KeyCode::Enter,
        KeyModifiers::NONE,
        KeyEventKind::Release,
    ));
    assert_eq!(aplicar_evento(&mut buffer, intro_soltada), Paso::Seguir);
    assert!(buffer.is_empty());
}

#[test]
fn un_pegado_se_anade_sin_saltos_y_no_acepta_la_frase() {
    let mut buffer = Zeroizing::new(String::new());
    escribir_texto(&mut buffer, "uno ");
    let pegado = Event::Paste("dos tres\r\n".to_string());
    assert_eq!(aplicar_evento(&mut buffer, pegado), Paso::Seguir);
    // Un salto de línea pegado no acepta ni se cuela: el usuario pulsa Intro.
    assert_eq!(buffer.as_str(), "uno dos tres");
    let con_control = Event::Paste("a\tb\x1b[31mc\0d".to_string());
    assert_eq!(aplicar_evento(&mut buffer, con_control), Paso::Seguir);
    assert_eq!(buffer.as_str(), "uno dos tresab[31mcd");
}

#[test]
fn la_lectura_tiene_tope_y_no_crece_sin_medida() {
    let mut buffer = Zeroizing::new(String::with_capacity(TOPE_LECTURA));
    let capacidad = buffer.capacity();
    let enorme = "ñ".repeat(TOPE_LECTURA);
    assert_eq!(
        aplicar_evento(&mut buffer, Event::Paste(enorme)),
        Paso::Seguir
    );
    assert!(buffer.len() <= TOPE_LECTURA, "{} bytes", buffer.len());
    escribir_texto(&mut buffer, "xxxx");
    assert!(buffer.len() <= TOPE_LECTURA);
    // Sin reasignaciones: no queda ninguna copia suelta de la frase.
    assert_eq!(buffer.capacity(), capacidad);
}

#[test]
fn otros_eventos_del_terminal_se_ignoran() {
    let mut buffer = Zeroizing::new(String::new());
    for evento in [Event::FocusGained, Event::FocusLost, Event::Resize(80, 24)] {
        assert_eq!(aplicar_evento(&mut buffer, evento), Paso::Seguir);
    }
    assert!(buffer.is_empty());
}

// ------------------------------------------------------------------ estado

#[test]
fn estado_sin_fichero_dice_que_no_hay_y_que_se_crea_al_guardar() {
    let banco = Banco::nuevo();
    let mut consola = ConsolaFalsa::con_terminal();
    let codigo = orden_estado(&banco.fichero(), &mut consola);
    assert_eq!(codigo, 0, "{}", consola.todo());
    assert!(
        consola.salida().contains("No hay llavero"),
        "{}",
        consola.todo()
    );
    assert!(
        consola.salida().contains("primer secreto"),
        "{}",
        consola.todo()
    );
    assert!(consola.preguntas.is_empty(), "no pregunta nada");
    assert!(!banco.ruta_llavero().exists(), "no crea nada");
}

#[test]
fn estado_con_fichero_da_ruta_tamano_y_fecha_sin_pedir_la_frase_ni_tocarlo() {
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let antes = banco.bytes_llavero().expect("fichero");
    let modificado_antes = std::fs::metadata(banco.ruta_llavero())
        .and_then(|datos| datos.modified())
        .expect("fecha del fichero");
    let mut consola = ConsolaFalsa::con_terminal();
    let codigo = orden_estado(&banco.fichero(), &mut consola);
    assert_eq!(codigo, 0, "{}", consola.todo());
    let salida = consola.salida();
    assert!(
        salida.contains(&banco.ruta_llavero().display().to_string()),
        "{salida}"
    );
    assert!(
        salida.contains(&format!("{} bytes", antes.len())),
        "{salida}"
    );
    assert!(
        salida.contains("Modificado: 20"),
        "una fecha legible: {salida}"
    );
    // Ni pregunta, ni abre, ni enseña entradas ni frases.
    assert!(consola.preguntas.is_empty());
    for secreto in [CUENTA, SECRETO, FRASE_VIEJA] {
        assert!(!consola.todo().contains(secreto), "{secreto} en la salida");
    }
    assert_eq!(banco.bytes_llavero().expect("fichero"), antes, "no lo toca");
    let modificado_despues = std::fs::metadata(banco.ruta_llavero())
        .and_then(|datos| datos.modified())
        .expect("fecha del fichero");
    assert_eq!(modificado_antes, modificado_despues);
}

#[test]
fn estado_con_el_fichero_danado_lo_dice_y_remite_a_abrir_magi() {
    let banco = Banco::nuevo();
    std::fs::write(banco.ruta_llavero(), b"esto no es un fichero age").expect("escribir");
    let mut consola = ConsolaFalsa::con_terminal();
    let codigo = orden_estado(&banco.fichero(), &mut consola);
    assert_eq!(codigo, 1, "{}", consola.todo());
    assert!(consola.salida().contains("dañado"), "{}", consola.todo());
    assert!(consola.salida().contains("Abre MAGI"), "{}", consola.todo());
    assert!(consola.preguntas.is_empty());
    assert_eq!(
        banco.bytes_llavero().as_deref(),
        Some(&b"esto no es un fichero age"[..]),
        "no lo toca"
    );
}

#[test]
fn estado_con_un_enlace_roto_en_la_ruta_no_dice_que_no_hay_llavero() {
    let banco = Banco::nuevo();
    let destino = banco.ruta_llavero().with_file_name("no-existe-nada");
    std::os::unix::fs::symlink(&destino, banco.ruta_llavero()).expect("enlace");
    let mut consola = ConsolaFalsa::con_terminal();
    let codigo = orden_estado(&banco.fichero(), &mut consola);
    // Hay algo en la ruta: se dice dañado y se remite a apartarlo desde MAGI.
    assert_eq!(codigo, 1, "{}", consola.todo());
    assert!(consola.salida().contains("dañado"), "{}", consola.todo());
    assert!(
        consola.salida().contains("enlace roto"),
        "{}",
        consola.todo()
    );
    assert!(consola.salida().contains("Abre MAGI"), "{}", consola.todo());
    assert!(!consola.salida().contains("No hay llavero"));
    assert!(consola.preguntas.is_empty());
    assert!(
        std::fs::symlink_metadata(banco.ruta_llavero()).is_ok(),
        "no lo toca"
    );
}

// ------------------------------------------------------------------ cambiar

#[test]
fn cambiar_con_la_frase_correcta_recifra_y_anota_en_el_registro() {
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let mut consola = ConsolaFalsa::con_terminal()
        .oculta(FRASE_VIEJA)
        .oculta(FRASE_NUEVA)
        .oculta(FRASE_NUEVA);
    let codigo = orden_cambiar(&banco.fichero(), &banco.rutas, &mut consola);
    assert_eq!(codigo, 0, "{}", consola.todo());
    assert_eq!(consola.preguntas.len(), 3, "actual, nueva y repetida");
    assert!(
        consola.avisos().contains("Recifrando"),
        "dice que trabaja: {}",
        consola.todo()
    );
    assert!(
        consola.salida().contains("Frase maestra cambiada"),
        "{}",
        consola.todo()
    );

    // La nueva abre (con el secreto de antes) y la vieja ya no.
    let nuevo = banco.fichero();
    nuevo.desbloquear(FRASE_NUEVA).expect("abre con la nueva");
    let guardado = LlaveroSecreto::recuperar(&nuevo, CUENTA)
        .expect("leer")
        .expect("hay secreto");
    assert_eq!(guardado.as_str(), SECRETO);
    assert!(matches!(
        banco.fichero().desbloquear(FRASE_VIEJA),
        Err(ErrorLlavero::FraseIncorrecta { .. })
    ));

    // Una fila, con el efecto ya producido, sin frases.
    assert_eq!(
        banco.registro(),
        vec![("llavero_frase_cambiada".to_string(), "ok".to_string())]
    );
    for frase in [FRASE_VIEJA, FRASE_NUEVA] {
        assert!(
            !consola.todo().contains(frase),
            "la frase no se escribe nunca"
        );
    }
}

#[test]
fn cambiar_con_la_frase_actual_incorrecta_no_toca_el_fichero_ni_anota() {
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let antes = banco.bytes_llavero();
    let mut consola = ConsolaFalsa::con_terminal()
        .oculta(FRASE_ERRONEA)
        .oculta(FRASE_NUEVA)
        .oculta(FRASE_NUEVA);
    let codigo = orden_cambiar(&banco.fichero(), &banco.rutas, &mut consola);
    assert_eq!(codigo, 1);
    assert!(
        consola.avisos().contains("actual es incorrecta"),
        "{}",
        consola.todo()
    );
    assert!(
        consola.avisos().contains("no ha cambiado"),
        "{}",
        consola.todo()
    );
    assert_eq!(banco.bytes_llavero(), antes, "el fichero no cambia");
    assert!(
        banco.registro().is_empty(),
        "no anota un cambio que no ocurrió"
    );
    banco
        .fichero()
        .desbloquear(FRASE_VIEJA)
        .expect("sigue con la vieja");
}

#[test]
fn cambiar_con_una_frase_nueva_que_no_vale_no_toca_nada() {
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let antes = banco.bytes_llavero();

    // Demasiado corta.
    let mut corta = ConsolaFalsa::con_terminal()
        .oculta(FRASE_VIEJA)
        .oculta("corta")
        .oculta("corta");
    assert_eq!(orden_cambiar(&banco.fichero(), &banco.rutas, &mut corta), 1);
    assert!(corta.avisos().contains("12"), "{}", corta.todo());

    // Las dos veces no coinciden.
    let mut distinta = ConsolaFalsa::con_terminal()
        .oculta(FRASE_VIEJA)
        .oculta(FRASE_NUEVA)
        .oculta("otra frase distinta del todo");
    assert_eq!(
        orden_cambiar(&banco.fichero(), &banco.rutas, &mut distinta),
        1
    );
    assert!(
        distinta.avisos().contains("coinciden"),
        "{}",
        distinta.todo()
    );

    assert_eq!(banco.bytes_llavero(), antes);
    assert!(banco.registro().is_empty());
}

#[test]
fn cambiar_sin_terminal_dice_como_hacerlo_y_no_pregunta_ni_toca() {
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let antes = banco.bytes_llavero();
    let mut consola = ConsolaFalsa::sin_terminal();
    let codigo = orden_cambiar(&banco.fichero(), &banco.rutas, &mut consola);
    assert_eq!(codigo, 1);
    assert!(
        consola.avisos().contains("hace falta un terminal"),
        "{}",
        consola.todo()
    );
    assert!(
        consola.avisos().contains("Termux"),
        "con instrucción: {}",
        consola.todo()
    );
    assert!(consola.preguntas.is_empty());
    assert_eq!(banco.bytes_llavero(), antes);
    assert!(banco.registro().is_empty());
}

#[test]
fn cambiar_sin_llavero_o_con_el_llavero_danado_da_un_error_claro() {
    let banco = Banco::nuevo();
    let mut sin = ConsolaFalsa::con_terminal();
    assert_eq!(orden_cambiar(&banco.fichero(), &banco.rutas, &mut sin), 1);
    assert!(sin.avisos().contains("no hay llavero"), "{}", sin.todo());
    assert!(sin.preguntas.is_empty(), "no pide frases para nada");

    std::fs::write(banco.ruta_llavero(), b"basura que no es age").expect("escribir");
    let mut danado = ConsolaFalsa::con_terminal();
    assert_eq!(
        orden_cambiar(&banco.fichero(), &banco.rutas, &mut danado),
        1
    );
    assert!(danado.avisos().contains("dañado"), "{}", danado.todo());
    assert!(danado.avisos().contains("abre MAGI"), "{}", danado.todo());
    assert!(danado.preguntas.is_empty());
    assert_eq!(
        banco.bytes_llavero().as_deref(),
        Some(&b"basura que no es age"[..])
    );
    assert!(banco.registro().is_empty());
}

#[test]
fn cambiar_cancelado_con_ctrl_c_no_toca_nada() {
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let antes = banco.bytes_llavero();
    // Cancela en la segunda pregunta.
    let mut consola = ConsolaFalsa::con_terminal()
        .oculta(FRASE_VIEJA)
        .oculta_cancelada();
    assert_eq!(
        orden_cambiar(&banco.fichero(), &banco.rutas, &mut consola),
        1
    );
    assert!(consola.avisos().contains("Cancelado"), "{}", consola.todo());
    assert_eq!(banco.bytes_llavero(), antes);
    assert!(banco.registro().is_empty());
}

// ------------------------------------------------------------------ borrar

#[test]
fn borrar_con_la_palabra_borra_explica_que_se_pierde_y_anota() {
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let mut consola = ConsolaFalsa::con_terminal().linea("borrar\n");
    let codigo = orden_borrar(&banco.fichero(), &banco.rutas, &mut consola);
    assert_eq!(codigo, 0, "{}", consola.todo());
    assert!(!banco.ruta_llavero().exists(), "el fichero ya no existe");
    let avisos = consola.avisos();
    assert!(avisos.contains("contraseñas"), "{avisos}");
    assert!(avisos.contains("clave de sincronización"), "{avisos}");
    assert!(avisos.contains("No se puede deshacer"), "{avisos}");
    // Borrar no pide la frase: solo la confirmación.
    assert_eq!(consola.preguntas.len(), 1, "{:?}", consola.preguntas);
    assert!(consola.preguntas[0].contains("borrar"));
    assert_eq!(
        banco.registro(),
        vec![("llavero_borrado".to_string(), "ok".to_string())]
    );
    assert!(
        consola.salida().contains("Llavero borrado"),
        "{}",
        consola.todo()
    );
}

#[test]
fn borrar_sin_escribir_exactamente_borrar_no_hace_nada() {
    for respuesta in [
        "",
        "\n",
        "no\n",
        "Borrar\n",
        "BORRAR\n",
        "borra\n",
        "borrar ya\n",
        "si\n",
    ] {
        let banco = Banco::nuevo();
        banco.crear(FRASE_VIEJA);
        let antes = banco.bytes_llavero();
        let mut consola = ConsolaFalsa::con_terminal().linea(respuesta);
        let codigo = orden_borrar(&banco.fichero(), &banco.rutas, &mut consola);
        assert_eq!(codigo, 1, "respuesta {respuesta:?}: {}", consola.todo());
        assert!(
            consola.avisos().contains("no se ha borrado nada"),
            "{}",
            consola.todo()
        );
        assert_eq!(banco.bytes_llavero(), antes, "respuesta {respuesta:?}");
        assert!(banco.registro().is_empty(), "respuesta {respuesta:?}");
    }
}

#[test]
fn borrar_acepta_los_espacios_alrededor_de_la_palabra() {
    // Los teclados de móvil añaden un espacio al autocompletar.
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let mut consola = ConsolaFalsa::con_terminal().linea("  borrar \r\n");
    assert_eq!(
        orden_borrar(&banco.fichero(), &banco.rutas, &mut consola),
        0
    );
    assert!(!banco.ruta_llavero().exists());
}

#[test]
fn borrar_cancelado_o_sin_terminal_no_toca_nada() {
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let antes = banco.bytes_llavero();

    let mut sin_terminal = ConsolaFalsa::sin_terminal();
    assert_eq!(
        orden_borrar(&banco.fichero(), &banco.rutas, &mut sin_terminal),
        1
    );
    assert!(
        sin_terminal.avisos().contains("hace falta un terminal"),
        "{}",
        sin_terminal.todo()
    );
    assert!(sin_terminal.preguntas.is_empty());

    let mut cancelado = ConsolaFalsa::con_terminal();
    cancelado.lineas.push_back(Err(ErrorEntrada::Cancelada));
    assert_eq!(
        orden_borrar(&banco.fichero(), &banco.rutas, &mut cancelado),
        1
    );
    assert!(
        cancelado.avisos().contains("Cancelado"),
        "{}",
        cancelado.todo()
    );

    assert_eq!(banco.bytes_llavero(), antes);
    assert!(banco.registro().is_empty());
}

#[test]
fn borrar_un_llavero_danado_no_pregunta_ni_borra_ni_anota() {
    // Un dañado se aparta, nunca se borra (informe §3.6, transiciones
    // inválidas): la orden se niega sin preguntar nada y remite a MAGI.
    let banco = Banco::nuevo();
    std::fs::write(banco.ruta_llavero(), b"esto no es age").expect("escribir");
    // Sin ninguna línea programada: si preguntase, la consola falsa caería.
    let mut consola = ConsolaFalsa::con_terminal();
    let codigo = orden_borrar(&banco.fichero(), &banco.rutas, &mut consola);
    assert_eq!(codigo, 1, "{}", consola.todo());
    assert!(consola.preguntas.is_empty(), "{:?}", consola.preguntas);
    let avisos = consola.avisos();
    assert!(avisos.contains("dañado"), "{avisos}");
    assert!(avisos.contains("abre MAGI"), "{avisos}");
    assert!(avisos.contains("apartarlo"), "{avisos}");
    assert!(avisos.contains("no se borra"), "{avisos}");
    assert!(!consola.salida().contains("Llavero borrado"));
    assert_eq!(
        banco.bytes_llavero().as_deref(),
        Some(&b"esto no es age"[..]),
        "no lo toca"
    );
    assert!(banco.registro().is_empty(), "no anota nada");
}

#[test]
fn borrar_si_el_llavero_se_danya_mientras_se_confirma_no_lo_borra() {
    // La carrera: el estado era sano al explicar y al pedir la palabra, pero
    // otra ventana lo estropea mientras la persona escribe. El núcleo mira bajo
    // el cerrojo y lo rechaza: la orden lo dice y no anota nada.
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let ruta = banco.ruta_llavero();
    let mut consola = ConsolaFalsa::con_terminal().linea("borrar\n");
    consola.al_leer_linea = Some(Box::new(move || {
        std::fs::write(&ruta, b"se estropeo a mitad").expect("estropear el fichero");
    }));
    let codigo = orden_borrar(&banco.fichero(), &banco.rutas, &mut consola);
    assert_eq!(codigo, 1, "{}", consola.todo());
    let avisos = consola.avisos();
    assert!(avisos.contains("dañado"), "{avisos}");
    assert!(avisos.contains("apartarlo"), "{avisos}");
    assert!(avisos.contains("No se ha borrado nada"), "{avisos}");
    assert!(!consola.salida().contains("Llavero borrado"));
    assert_eq!(
        banco.bytes_llavero().as_deref(),
        Some(&b"se estropeo a mitad"[..]),
        "el fichero sigue ahí, sin borrar"
    );
    assert!(banco.registro().is_empty(), "no anota nada");
}

#[test]
fn borrar_sin_llavero_no_tiene_nada_que_borrar() {
    let banco = Banco::nuevo();
    let mut consola = ConsolaFalsa::con_terminal();
    assert_eq!(
        orden_borrar(&banco.fichero(), &banco.rutas, &mut consola),
        0
    );
    assert!(
        consola.salida().contains("nada que borrar"),
        "{}",
        consola.todo()
    );
    assert!(consola.preguntas.is_empty());
    assert!(banco.registro().is_empty());
}

// ------------------------------------------------------------------ abrir_para_cli

#[test]
fn desbloquear_para_cli_acepta_la_frase_buena_a_la_tercera() {
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let llavero = banco.fichero();
    assert_eq!(LlaveroCliente::estado(&llavero), EstadoLlavero::Bloqueado);
    let mut consola = ConsolaFalsa::con_terminal()
        .oculta(FRASE_ERRONEA)
        .oculta("tampoco es esta frase")
        .oculta(FRASE_VIEJA);
    desbloquear_para_cli(&llavero, &mut consola).expect("a la tercera abre");
    assert_eq!(consola.preguntas.len(), 3);
    assert!(
        consola.avisos().contains("intento 1 de 3"),
        "{}",
        consola.todo()
    );
    assert!(
        consola.avisos().contains("intento 2 de 3"),
        "{}",
        consola.todo()
    );
    assert_eq!(LlaveroCliente::estado(&llavero), EstadoLlavero::Abierto);
    let guardado = LlaveroSecreto::recuperar(&llavero, CUENTA)
        .expect("leer")
        .expect("secreto");
    assert_eq!(guardado.as_str(), SECRETO);
    assert!(banco.registro().is_empty(), "abrir no anota nada");
}

#[test]
fn desbloquear_para_cli_se_rinde_a_los_tres_intentos_y_lo_dice() {
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let llavero = banco.fichero();
    let mut consola = ConsolaFalsa::con_terminal()
        .oculta("mala uno de prueba")
        .oculta("mala dos de prueba")
        .oculta("mala tres de prueba");
    let error = desbloquear_para_cli(&llavero, &mut consola).expect_err("no abre");
    assert!(
        format!("{error:#}").contains(&format!("{INTENTOS_FRASE} veces")),
        "{error:#}"
    );
    assert_eq!(consola.preguntas.len(), INTENTOS_FRASE as usize);
    assert_eq!(LlaveroCliente::estado(&llavero), EstadoLlavero::Bloqueado);
    assert!(
        LlaveroSecreto::recuperar(&llavero, CUENTA).is_err(),
        "sigue cerrado: leer da «bloqueado»"
    );
}

#[test]
fn desbloquear_para_cli_respeta_la_espera_de_la_serie_de_fallos() {
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let llavero = banco.fichero();
    // Cinco fallos seguidos: el llavero impone 30 s de espera.
    for _ in 0..5 {
        assert!(matches!(
            llavero.desbloquear(FRASE_ERRONEA),
            Err(ErrorLlavero::FraseIncorrecta { .. })
        ));
    }
    let mut consola = ConsolaFalsa::con_terminal().oculta(FRASE_VIEJA);
    let error = desbloquear_para_cli(&llavero, &mut consola).expect_err("en espera");
    let mensaje = format!("{error:#}");
    assert!(mensaje.contains("espera"), "{mensaje}");
    assert!(mensaje.contains(" s "), "dice cuántos segundos: {mensaje}");
    assert_eq!(consola.preguntas.len(), 1, "no insiste: sale a la primera");
    assert_eq!(LlaveroCliente::estado(&llavero), EstadoLlavero::Bloqueado);

    // Pasada la espera, vuelve a funcionar.
    banco.adelanto.store(31, Ordering::SeqCst);
    let mut despues = ConsolaFalsa::con_terminal().oculta(FRASE_VIEJA);
    desbloquear_para_cli(&llavero, &mut despues).expect("pasada la espera abre");
    assert_eq!(LlaveroCliente::estado(&llavero), EstadoLlavero::Abierto);
}

#[test]
fn desbloquear_para_cli_trata_una_frase_vacia_o_demasiado_larga_como_un_intento_mas() {
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let llavero = banco.fichero();
    let larga = "a".repeat(MAX_FRASE_BYTES + 1);
    let mut consola = ConsolaFalsa::con_terminal()
        .oculta("")
        .oculta(&larga)
        .oculta(FRASE_VIEJA);
    desbloquear_para_cli(&llavero, &mut consola).expect("a la tercera abre");
    assert_eq!(consola.preguntas.len(), 3, "volvió a preguntar");
    let avisos = consola.avisos();
    assert!(avisos.contains("intento 1 de 3"), "{avisos}");
    assert!(avisos.contains("intento 2 de 3"), "{avisos}");
    assert!(avisos.contains("escribe la frase maestra"), "{avisos}");
    assert!(avisos.contains("demasiado larga"), "{avisos}");
    assert!(!avisos.contains("Frase incorrecta"), "no son frases malas");
    assert_eq!(LlaveroCliente::estado(&llavero), EstadoLlavero::Abierto);
}

#[test]
fn desbloquear_para_cli_se_rinde_tras_tres_frases_no_validas_sin_gastar_la_serie() {
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let llavero = banco.fichero();
    let mut consola = ConsolaFalsa::con_terminal()
        .oculta("")
        .oculta("")
        .oculta("");
    let error = desbloquear_para_cli(&llavero, &mut consola).expect_err("no abre");
    assert!(
        format!("{error:#}").contains(&format!("{INTENTOS_FRASE} veces")),
        "{error:#}"
    );
    assert_eq!(consola.preguntas.len(), INTENTOS_FRASE as usize);
    assert_eq!(LlaveroCliente::estado(&llavero), EstadoLlavero::Bloqueado);
    // Una frase que ni se prueba no cuenta en la serie de fallos del llavero
    // (no hay scrypt ni espera).
    assert_eq!(llavero.intentos_seguidos(), 0);
}

#[test]
fn desbloquear_para_cli_sin_terminal_da_un_error_con_instruccion() {
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let llavero = banco.fichero();
    let mut consola = ConsolaFalsa::sin_terminal();
    let error = desbloquear_para_cli(&llavero, &mut consola).expect_err("sin terminal");
    let mensaje = format!("{error:#}");
    assert!(mensaje.contains("bloqueado"), "{mensaje}");
    assert!(
        mensaje.contains("terminal"),
        "dice cómo desbloquear: {mensaje}"
    );
    assert!(consola.preguntas.is_empty());
    assert_eq!(LlaveroCliente::estado(&llavero), EstadoLlavero::Bloqueado);
}

#[test]
fn desbloquear_para_cli_cancelado_no_abre() {
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let llavero = banco.fichero();
    let mut consola = ConsolaFalsa::con_terminal().oculta_cancelada();
    let error = desbloquear_para_cli(&llavero, &mut consola).expect_err("cancelado");
    assert!(format!("{error:#}").contains("cancelado"), "{error:#}");
    assert_eq!(LlaveroCliente::estado(&llavero), EstadoLlavero::Bloqueado);
}

#[test]
fn desbloquear_para_cli_con_el_llavero_danado_remite_a_abrir_magi() {
    let banco = Banco::nuevo();
    std::fs::write(banco.ruta_llavero(), b"esto no es age").expect("escribir");
    let llavero = banco.fichero();
    let mut consola = ConsolaFalsa::con_terminal();
    let error = desbloquear_para_cli(&llavero, &mut consola).expect_err("dañado");
    let mensaje = format!("{error:#}");
    assert!(mensaje.contains("dañado"), "{mensaje}");
    assert!(mensaje.contains("abre MAGI"), "{mensaje}");
    assert!(mensaje.contains("apartarlo"), "{mensaje}");
    assert!(
        consola.preguntas.is_empty(),
        "no pide la frase de un fichero ilegible"
    );
    assert_eq!(
        banco.bytes_llavero().as_deref(),
        Some(&b"esto no es age"[..]),
        "no lo toca"
    );
}

#[test]
fn desbloquear_para_cli_no_pregunta_si_no_hay_nada_que_abrir() {
    // Sin crear: se devuelve tal cual (las lecturas dan «no hay»).
    let banco = Banco::nuevo();
    let sin_crear = banco.fichero();
    let mut consola = ConsolaFalsa::sin_terminal();
    desbloquear_para_cli(&sin_crear, &mut consola).expect("sin crear no pregunta");
    assert!(consola.preguntas.is_empty());
    assert_eq!(LlaveroCliente::estado(&sin_crear), EstadoLlavero::SinCrear);
    assert!(LlaveroSecreto::recuperar(&sin_crear, CUENTA)
        .expect("leer")
        .is_none());

    // Ya abierto, o un llavero que no es de fichero (el del sistema, en las
    // pruebas el de memoria): tampoco.
    banco.crear(FRASE_VIEJA);
    let abierto = banco.fichero();
    abierto.desbloquear(FRASE_VIEJA).expect("abrir");
    desbloquear_para_cli(&abierto, &mut consola).expect("abierto no pregunta");
    desbloquear_para_cli(&LlaveroEnMemoria::nuevo(), &mut consola).expect("memoria no pregunta");
    assert!(consola.preguntas.is_empty());
}

// ------------------------------------------------------------------ el binario real

#[test]
fn llavero_estado_sin_fichero_en_termux() {
    if !hay_modo_termux("llavero_estado_sin_fichero_en_termux") {
        return;
    }
    let banco = Banco::nuevo();
    let salida = banco.ejecutar(true, &["llavero", "estado"]);
    assert_eq!(salida.status.code(), Some(0), "{}", texto(&salida.stderr));
    let stdout = texto(&salida.stdout);
    assert!(stdout.contains("No hay llavero"), "{stdout}");
    assert!(stdout.contains("primer secreto"), "{stdout}");
    assert!(!banco.ruta_llavero().exists(), "no crea el fichero");
}

#[test]
fn llavero_estado_con_fichero_en_termux_no_pide_la_frase() {
    if !hay_modo_termux("llavero_estado_con_fichero_en_termux_no_pide_la_frase") {
        return;
    }
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let antes = banco.bytes_llavero().expect("fichero");
    let salida = banco.ejecutar(true, &["llavero", "estado"]);
    let stdout = texto(&salida.stdout);
    assert_eq!(
        salida.status.code(),
        Some(0),
        "{stdout}{}",
        texto(&salida.stderr)
    );
    assert!(stdout.contains("Hay un llavero cifrado"), "{stdout}");
    assert!(
        stdout.contains(&banco.ruta_llavero().display().to_string()),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("{} bytes", antes.len())),
        "{stdout}"
    );
    assert!(stdout.contains("Modificado: 20"), "{stdout}");
    for secreto in [CUENTA, SECRETO, FRASE_VIEJA] {
        assert!(!stdout.contains(secreto), "{secreto} no se enseña");
    }
    assert_eq!(banco.bytes_llavero().expect("fichero"), antes);
}

#[test]
fn llavero_estado_con_fichero_danado_sale_con_1() {
    if !hay_modo_termux("llavero_estado_con_fichero_danado_sale_con_1") {
        return;
    }
    let banco = Banco::nuevo();
    std::fs::write(banco.ruta_llavero(), b"no es age").expect("escribir");
    let salida = banco.ejecutar(true, &["llavero", "estado"]);
    assert_eq!(salida.status.code(), Some(1));
    let stdout = texto(&salida.stdout);
    assert!(stdout.contains("dañado"), "{stdout}");
    assert!(stdout.contains("Abre MAGI"), "{stdout}");
}

#[test]
fn llavero_cambiar_y_borrar_sin_terminal_dan_error_con_instruccion() {
    if !hay_modo_termux("llavero_cambiar_y_borrar_sin_terminal_dan_error_con_instruccion") {
        return;
    }
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let antes = banco.bytes_llavero();
    for orden in ["cambiar", "borrar"] {
        let salida = banco.ejecutar(true, &["llavero", orden]);
        assert_eq!(salida.status.code(), Some(1), "magi llavero {orden}");
        let stderr = texto(&salida.stderr);
        assert!(
            stderr.contains("hace falta un terminal"),
            "{orden}: {stderr}"
        );
        assert!(stderr.contains("Termux"), "{orden}: {stderr}");
        assert!(texto(&salida.stdout).is_empty(), "{orden}");
    }
    assert_eq!(banco.bytes_llavero(), antes, "no se toca");
    assert!(banco.registro().is_empty());
}

#[test]
fn llavero_borrar_con_la_palabra_por_una_tuberia_no_borra() {
    if !hay_modo_termux("llavero_borrar_con_la_palabra_por_una_tuberia_no_borra") {
        return;
    }
    // Aunque la entrada traiga la palabra, sin terminal no se borra.
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let mut hijo = banco
        .comando(true, &["llavero", "borrar"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("lanzar magi");
    hijo.stdin
        .take()
        .expect("entrada")
        .write_all(b"borrar\n")
        .expect("escribir");
    let salida = hijo.wait_with_output().expect("esperar");
    assert_eq!(salida.status.code(), Some(1));
    assert!(banco.ruta_llavero().exists());
}

#[test]
fn fuera_de_termux_las_tres_ordenes_dicen_que_el_llavero_es_el_del_sistema() {
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let antes = banco.bytes_llavero();
    for orden in ["estado", "cambiar", "borrar"] {
        let salida = banco.ejecutar(false, &["llavero", orden]);
        assert_eq!(salida.status.code(), Some(1), "magi llavero {orden}");
        let stderr = texto(&salida.stderr);
        assert!(
            stderr.contains("en este equipo el llavero es el del sistema"),
            "{orden}: {stderr}"
        );
        assert!(texto(&salida.stdout).is_empty(), "{orden}");
    }
    assert_eq!(banco.bytes_llavero(), antes, "no toca nada");
}

#[test]
fn fuera_de_termux_magi_termux_dice_solo_en_termux() {
    // La pista de Termux lo completa; aquí solo se comprueba que el
    // subcomando existe y sale con 1 fuera de Termux.
    let banco = Banco::nuevo();
    let salida = banco.ejecutar(false, &["termux", "aviso"]);
    assert_eq!(salida.status.code(), Some(1));
    assert!(
        texto(&salida.stderr).contains("solo en Termux"),
        "{}",
        texto(&salida.stderr)
    );
}

#[test]
fn la_ayuda_de_llavero_y_termux_esta_en_castellano_y_sin_rutas_reales() {
    let banco = Banco::nuevo();
    let salida = banco.ejecutar(false, &["llavero", "--help"]);
    assert_eq!(salida.status.code(), Some(0));
    let ayuda = texto(&salida.stdout);
    for parte in ["estado", "cambiar", "borrar", "frase maestra", "Termux"] {
        assert!(ayuda.contains(parte), "falta «{parte}» en: {ayuda}");
    }
    let ayuda_termux = texto(&banco.ejecutar(false, &["termux", "--help"]).stdout);
    for parte in ["teclas", "aviso", "Termux"] {
        assert!(
            ayuda_termux.contains(parte),
            "falta «{parte}» en: {ayuda_termux}"
        );
    }
    for ayuda in [&ayuda, &ayuda_termux] {
        assert!(!ayuda.contains("/home/"), "ruta real en la ayuda: {ayuda}");
    }
}

/// Crea un host en el inventario de este banco (lo necesita `magi sondear`) y
/// devuelve su id.
fn crear_host_con(
    banco: &Banco,
    nombre: &str,
    direccion: &str,
    puerto: u16,
    identidad: IdentidadRef,
    salto: Option<i64>,
) -> i64 {
    std::fs::create_dir_all(&banco.rutas.datos).expect("datos");
    let almacen = magi::almacen::abrir(&banco.rutas.base_datos()).expect("abrir la base");
    let datos = DatosHost {
        nombre: nombre.to_string(),
        direccion: direccion.to_string(),
        puerto,
        usuario: Some("usuario".to_string()),
        identidad_ref: identidad,
        salto_host_id: salto,
        ..DatosHost::default()
    };
    let id = almacen
        .crear_host(&datos, Origen::Manual)
        .expect("crear el host");
    almacen.cerrar().expect("cerrar la base");
    id
}

/// Un host de documentación que se autentica con la contraseña del llavero:
/// `magi sondear` necesita abrirlo para sondearlo.
fn crear_host(banco: &Banco) {
    crear_host_con(
        banco,
        "web",
        "192.0.2.10",
        22,
        IdentidadRef::ContrasenaLlavero,
        None,
    );
}

/// Los hosts del inventario de este banco, como los lee `magi sondear`.
fn hosts_del_banco(banco: &Banco) -> std::collections::HashMap<i64, Host> {
    let almacen = magi::almacen::abrir(&banco.rutas.base_datos()).expect("abrir la base");
    let hosts = almacen
        .listar_hosts()
        .expect("listar los hosts")
        .into_iter()
        .map(|host| (host.id, host))
        .collect();
    almacen.cerrar().expect("cerrar la base");
    hosts
}

#[test]
fn sondear_con_el_llavero_cerrado_y_sin_terminal_da_error_con_instruccion() {
    if !hay_modo_termux("sondear_con_el_llavero_cerrado_y_sin_terminal_da_error_con_instruccion") {
        return;
    }
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    crear_host(&banco);
    let salida = banco.ejecutar(true, &["sondear", "web"]);
    assert_eq!(salida.status.code(), Some(1));
    let stderr = texto(&salida.stderr);
    assert!(stderr.contains("bloqueado"), "{stderr}");
    assert!(
        stderr.contains("terminal"),
        "dice cómo desbloquear: {stderr}"
    );
    assert!(!stderr.contains(FRASE_VIEJA));
}

#[test]
fn sondear_sin_ningun_host_con_contrasena_de_llavero_no_pide_la_frase_ni_aborta() {
    // El informe (§4.2) dice «CLI que lo necesita»: el sondeo solo abre el
    // llavero si algún host que sondea lo usa. Aquí ninguno, así que con el
    // llavero cerrado y sin terminal el sondeo sigue (el host cae por su
    // cuenta: nadie escucha en el puerto 1 del propio equipo).
    if !hay_modo_termux("sondear_sin_ningun_host_con_contrasena_de_llavero") {
        return;
    }
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    crear_host_con(&banco, "local", "127.0.0.1", 1, IdentidadRef::Auto, None);
    let antes = banco.bytes_llavero();
    let salida = banco.ejecutar(true, &["sondear", "local"]);
    let (stdout, stderr) = (texto(&salida.stdout), texto(&salida.stderr));
    assert_eq!(salida.status.code(), Some(0), "{stdout}{stderr}");
    assert!(stdout.contains("local"), "la tabla del sondeo: {stdout}");
    assert!(
        !stderr.contains("bloqueado"),
        "no se tocó el llavero: {stderr}"
    );
    assert!(!stdout.contains(FRASE_VIEJA) && !stderr.contains(FRASE_VIEJA));
    assert_eq!(banco.bytes_llavero(), antes, "no toca el llavero");
}

#[test]
fn sondear_un_host_cuyo_salto_usa_el_llavero_lo_necesita_y_da_error_con_instruccion() {
    if !hay_modo_termux("sondear_un_host_cuyo_salto_usa_el_llavero") {
        return;
    }
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let salto = crear_host_con(
        &banco,
        "salto",
        "192.0.2.20",
        22,
        IdentidadRef::ContrasenaLlavero,
        None,
    );
    crear_host_con(
        &banco,
        "interno",
        "192.0.2.30",
        22,
        IdentidadRef::Auto,
        Some(salto),
    );
    let salida = banco.ejecutar(true, &["sondear", "interno"]);
    assert_eq!(
        salida.status.code(),
        Some(1),
        "{}{}",
        texto(&salida.stdout),
        texto(&salida.stderr)
    );
    let stderr = texto(&salida.stderr);
    assert!(stderr.contains("bloqueado"), "{stderr}");
    assert!(stderr.contains("terminal"), "{stderr}");
}

#[test]
fn algun_host_usa_llavero_mira_los_hosts_y_sus_saltos() {
    let banco = Banco::nuevo();
    let con_llavero = crear_host_con(
        &banco,
        "con-llavero",
        "192.0.2.10",
        22,
        IdentidadRef::ContrasenaLlavero,
        None,
    );
    let libre = crear_host_con(&banco, "libre", "192.0.2.11", 22, IdentidadRef::Auto, None);
    let pide = crear_host_con(
        &banco,
        "pide",
        "192.0.2.12",
        22,
        IdentidadRef::Contrasena,
        None,
    );
    let tras_llavero = crear_host_con(
        &banco,
        "tras-llavero",
        "192.0.2.13",
        22,
        IdentidadRef::Auto,
        Some(con_llavero),
    );
    let tras_libre = crear_host_con(
        &banco,
        "tras-libre",
        "192.0.2.14",
        22,
        IdentidadRef::Auto,
        Some(libre),
    );
    let todos = hosts_del_banco(&banco);
    let elegir = |ids: &[i64]| -> Vec<Host> { ids.iter().map(|id| todos[id].clone()).collect() };

    assert!(algun_host_usa_llavero(&elegir(&[con_llavero]), &todos));
    assert!(
        algun_host_usa_llavero(&elegir(&[tras_llavero]), &todos),
        "su salto usa el llavero"
    );
    assert!(algun_host_usa_llavero(
        &elegir(&[libre, tras_libre, tras_llavero]),
        &todos
    ));
    // Solo cuentan los hosts que se van a sondear, no todo el inventario.
    assert!(!algun_host_usa_llavero(
        &elegir(&[libre, pide, tras_libre]),
        &todos
    ));
    assert!(!algun_host_usa_llavero(&[], &todos));

    // Una cadena rota falla antes de autenticar nada: no lee el llavero.
    let mut sin_salto = todos.clone();
    sin_salto.remove(&con_llavero);
    assert!(!algun_host_usa_llavero(
        &[todos[&tras_llavero].clone()],
        &sin_salto
    ));
}

#[test]
fn sincronizar_con_el_llavero_cerrado_y_sin_terminal_da_error_con_instruccion() {
    if !hay_modo_termux(
        "sincronizar_con_el_llavero_cerrado_y_sin_terminal_da_error_con_instruccion",
    ) {
        return;
    }
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    // Una carpeta de sincronización configurada: la pasada necesita la clave
    // del llavero, que está cerrado.
    let carpeta = banco.tmp.join("sinc");
    std::fs::create_dir_all(&carpeta).expect("carpeta de sincronización");
    std::fs::create_dir_all(&banco.rutas.config).expect("directorio de configuración");
    std::fs::write(
        banco.rutas.fichero_config(),
        format!("[sincronizacion]\ncarpeta = \"{}\"\n", carpeta.display()),
    )
    .expect("config.toml");
    let salida = banco.ejecutar(true, &["sincronizar"]);
    assert_eq!(salida.status.code(), Some(1), "{}", texto(&salida.stderr));
    let stderr = texto(&salida.stderr);
    assert!(stderr.contains("bloqueado"), "{stderr}");
    assert!(
        stderr.contains("terminal"),
        "dice cómo desbloquear: {stderr}"
    );
    // No se escribió nada en la carpeta compartida.
    assert_eq!(
        std::fs::read_dir(&carpeta)
            .expect("leer la carpeta")
            .count(),
        0
    );
}

// ------------------------------------------------------------------ con un terminal de verdad

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
        "llavero_cli::{prueba}: en CI hace falta el `script` de util-linux"
    );
    eprintln!("llavero_cli::{prueba}: se omite, falta `script` de util-linux");
    false
}

/// Un programa corriendo dentro de un pseudoterminal creado por `script`.
struct SesionPty {
    hijo: Child,
    entrada: Option<ChildStdin>,
    salida: Arc<Mutex<Vec<u8>>>,
    /// El hilo que vuelca la salida de `script`: al acabar el programa se
    /// espera a que lea hasta el final, para no mirar la salida a medias.
    lector: Option<std::thread::JoinHandle<()>>,
}

impl SesionPty {
    /// Lanza `orden` (un texto de shell) dentro del pseudoterminal, con el
    /// entorno de `comando` (el del banco).
    fn lanzar(mut comando: Command, orden: &str) -> Self {
        comando.args(["-qefc", orden, "/dev/null"]);
        let mut hijo = comando
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
        texto(&self.salida.lock().expect("salida del pty"))
    }

    /// Espera a que lo escrito en el terminal contenga `fragmento`.
    fn esperar(&self, fragmento: &str, plazo: Duration) {
        self.esperar_veces(fragmento, 1, plazo);
    }

    /// Espera a que `fragmento` haya salido al menos `veces` veces (una
    /// pregunta que se repite no se confunde con la anterior).
    fn esperar_veces(&self, fragmento: &str, veces: usize, plazo: Duration) {
        let limite = Instant::now() + plazo;
        loop {
            if self.visto().matches(fragmento).count() >= veces {
                return;
            }
            assert!(
                Instant::now() < limite,
                "no apareció «{fragmento}» ({veces} veces) en el terminal; se vio: {:?}",
                self.visto()
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// Escribe lo que escribiría una persona. En modo raw, Intro llega como
    /// `\r`.
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

/// `script` con el entorno del banco y una orden que guarda el modo del
/// terminal antes (`stty -g`), ejecuta `magi` con los argumentos y lo guarda
/// después, para comprobar que lo deja como lo encontró.
fn lanzar_magi_en_pty(
    banco: &Banco,
    termux: bool,
    argumentos: &[&str],
) -> (SesionPty, PathBuf, PathBuf) {
    let antes = banco.tmp.join("modo-antes");
    let despues = banco.tmp.join("modo-despues");
    let mut orden = format!(
        "stty rows 24 cols 80; stty -g > '{}'; '{}'",
        antes.display(),
        env!("CARGO_BIN_EXE_magi")
    );
    for argumento in argumentos {
        orden.push_str(&format!(" '{argumento}'"));
    }
    orden.push_str(&format!(
        "; c=$?; stty -g > '{}'; exit $c",
        despues.display()
    ));
    let mut script = Command::new("script");
    script.env_clear().envs(banco.entorno(termux));
    (SesionPty::lanzar(script, &orden), antes, despues)
}

fn modo_de(ruta: &Path) -> String {
    std::fs::read_to_string(ruta)
        .unwrap_or_else(|error| panic!("no se pudo leer {}: {error}", ruta.display()))
        .trim()
        .to_string()
}

const PLAZO_PROMPT: Duration = Duration::from_secs(30);

#[test]
fn cambiar_en_un_terminal_cancelado_con_ctrl_c_deja_el_terminal_y_el_fichero_como_estaban() {
    let prueba = "cambiar_en_un_terminal_cancelado_con_ctrl_c";
    if !hay_modo_termux(prueba) || !hay_script(prueba) {
        return;
    }
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let antes_fichero = banco.bytes_llavero();
    let (mut sesion, antes, despues) = lanzar_magi_en_pty(&banco, true, &["llavero", "cambiar"]);
    sesion.esperar("Frase maestra actual", PLAZO_PROMPT);
    // Lo escrito no se ve (sin eco) y Ctrl+C cancela.
    sesion.escribir(b"una frase a medias");
    sesion.escribir(&[0x03]);
    let codigo = sesion.esperar_salida(Duration::from_secs(30));
    let visto = sesion.visto();
    assert_eq!(codigo, Some(1), "{visto:?}");
    assert!(visto.contains("Cancelado"), "{visto:?}");
    assert!(
        !visto.contains("una frase a medias"),
        "eco de la frase: {visto:?}"
    );
    assert_eq!(
        modo_de(&antes),
        modo_de(&despues),
        "el terminal queda como estaba"
    );
    assert_eq!(banco.bytes_llavero(), antes_fichero);
    assert!(banco.registro().is_empty());
}

#[test]
fn cambiar_en_un_terminal_con_la_frase_actual_incorrecta_no_toca_el_fichero() {
    let prueba = "cambiar_en_un_terminal_con_la_frase_actual_incorrecta";
    if !hay_modo_termux(prueba) || !hay_script(prueba) {
        return;
    }
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let antes_fichero = banco.bytes_llavero();
    let (mut sesion, antes, despues) = lanzar_magi_en_pty(&banco, true, &["llavero", "cambiar"]);
    sesion.esperar("Frase maestra actual", PLAZO_PROMPT);
    sesion.escribir(format!("{FRASE_ERRONEA}\r").as_bytes());
    sesion.esperar("Frase maestra nueva", PLAZO_PROMPT);
    sesion.escribir(format!("{FRASE_NUEVA}\r").as_bytes());
    sesion.esperar("Repite la frase nueva", PLAZO_PROMPT);
    sesion.escribir(format!("{FRASE_NUEVA}\r").as_bytes());
    let codigo = sesion.esperar_salida(Duration::from_secs(60));
    let visto = sesion.visto();
    assert_eq!(codigo, Some(1), "{visto:?}");
    assert!(visto.contains("actual es incorrecta"), "{visto:?}");
    for frase in [FRASE_ERRONEA, FRASE_NUEVA, FRASE_VIEJA] {
        assert!(!visto.contains(frase), "eco de una frase: {visto:?}");
    }
    assert_eq!(modo_de(&antes), modo_de(&despues));
    assert_eq!(banco.bytes_llavero(), antes_fichero);
    assert!(banco.registro().is_empty());
}

/// El camino feliz de punta a punta. El binario recifra con el factor de
/// producción (2^17): `Cargo.toml` optimiza solo `scrypt` en depuración, así
/// que tarda menos de un segundo; el plazo largo cubre un equipo muy cargado.
#[test]
fn cambiar_en_un_terminal_recifra_sin_eco_y_anota_en_el_registro() {
    let prueba = "cambiar_en_un_terminal_recifra_sin_eco";
    if !hay_modo_termux(prueba) || !hay_script(prueba) {
        return;
    }
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let (mut sesion, antes, despues) = lanzar_magi_en_pty(&banco, true, &["llavero", "cambiar"]);
    sesion.esperar("Frase maestra actual", PLAZO_PROMPT);
    sesion.escribir(format!("{FRASE_VIEJA}\r").as_bytes());
    sesion.esperar("Frase maestra nueva", PLAZO_PROMPT);
    sesion.escribir(format!("{FRASE_NUEVA}\r").as_bytes());
    sesion.esperar("Repite la frase nueva", PLAZO_PROMPT);
    sesion.escribir(format!("{FRASE_NUEVA}\r").as_bytes());
    let codigo = sesion.esperar_salida(Duration::from_secs(300));
    let visto = sesion.visto();
    assert_eq!(codigo, Some(0), "{visto:?}");
    assert!(visto.contains("Recifrando"), "{visto:?}");
    assert!(visto.contains("Frase maestra cambiada"), "{visto:?}");
    for frase in [FRASE_NUEVA, FRASE_VIEJA] {
        assert!(!visto.contains(frase), "eco de una frase: {visto:?}");
    }
    assert_eq!(
        modo_de(&antes),
        modo_de(&despues),
        "el terminal queda como estaba"
    );

    // La nueva abre con el secreto de antes; la vieja ya no.
    let nuevo = banco.fichero();
    nuevo.desbloquear(FRASE_NUEVA).expect("abre con la nueva");
    let guardado = LlaveroSecreto::recuperar(&nuevo, CUENTA)
        .expect("leer")
        .expect("secreto");
    assert_eq!(guardado.as_str(), SECRETO);
    assert!(matches!(
        banco.fichero().desbloquear(FRASE_VIEJA),
        Err(ErrorLlavero::FraseIncorrecta { .. })
    ));
    assert_eq!(
        banco.registro(),
        vec![("llavero_frase_cambiada".to_string(), "ok".to_string())]
    );
}

#[test]
fn borrar_en_un_terminal_pide_la_palabra_y_borra() {
    let prueba = "borrar_en_un_terminal_pide_la_palabra";
    if !hay_modo_termux(prueba) || !hay_script(prueba) {
        return;
    }
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    // Primero, otra palabra: no borra.
    let (mut sesion, antes, despues) = lanzar_magi_en_pty(&banco, true, &["llavero", "borrar"]);
    sesion.esperar("escribe borrar", PLAZO_PROMPT);
    sesion.escribir(b"no\n");
    let codigo = sesion.esperar_salida(Duration::from_secs(30));
    let visto = sesion.visto();
    assert_eq!(codigo, Some(1), "{visto:?}");
    assert!(visto.contains("no se ha borrado nada"), "{visto:?}");
    assert!(visto.contains("clave de sincronización"), "{visto:?}");
    assert_eq!(modo_de(&antes), modo_de(&despues));
    assert!(banco.ruta_llavero().exists());

    // Ahora, la palabra: borra y anota.
    let (mut sesion, antes, despues) = lanzar_magi_en_pty(&banco, true, &["llavero", "borrar"]);
    sesion.esperar("escribe borrar", PLAZO_PROMPT);
    sesion.escribir(b"borrar\n");
    let codigo = sesion.esperar_salida(Duration::from_secs(30));
    let visto = sesion.visto();
    assert_eq!(codigo, Some(0), "{visto:?}");
    assert!(visto.contains("Llavero borrado"), "{visto:?}");
    assert_eq!(modo_de(&antes), modo_de(&despues));
    assert!(!banco.ruta_llavero().exists());
    assert_eq!(
        banco.registro(),
        vec![("llavero_borrado".to_string(), "ok".to_string())]
    );
}

#[test]
fn sondear_con_el_llavero_cerrado_pide_la_frase_en_el_terminal() {
    let prueba = "sondear_con_el_llavero_cerrado_pide_la_frase";
    if !hay_modo_termux(prueba) || !hay_script(prueba) {
        return;
    }
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    crear_host(&banco);
    let (mut sesion, antes, despues) = lanzar_magi_en_pty(&banco, true, &["sondear", "web"]);
    // Tres frases malas: la orden se rinde sin llegar a conectar con nadie.
    for intento in 1..=INTENTOS_FRASE {
        sesion.esperar_veces(
            "Frase maestra del llavero de MAGI",
            intento as usize,
            PLAZO_PROMPT,
        );
        sesion.escribir(format!("{FRASE_ERRONEA}\r").as_bytes());
    }
    let codigo = sesion.esperar_salida(Duration::from_secs(60));
    let visto = sesion.visto();
    assert_eq!(codigo, Some(1), "{visto:?}");
    assert!(
        visto.contains(&format!("{INTENTOS_FRASE} veces")),
        "{visto:?}"
    );
    for intento in 1..INTENTOS_FRASE {
        assert!(
            visto.contains(&format!("intento {intento} de {INTENTOS_FRASE}")),
            "{visto:?}"
        );
    }
    assert!(!visto.contains(FRASE_ERRONEA), "eco de la frase: {visto:?}");
    assert_eq!(modo_de(&antes), modo_de(&despues));
}

/// Con el llavero cerrado, `magi sincronizar` pide la frase UNA sola vez: el
/// llavero que se abre para purgar la clave anterior es el mismo que lee la
/// pasada (antes, el primero moría al acabar su bloque y la pasada volvía a
/// preguntar, con otra vuelta de scrypt).
#[test]
fn sincronizar_con_el_llavero_cerrado_pide_la_frase_una_sola_vez() {
    let prueba = "sincronizar_con_el_llavero_cerrado_pide_la_frase_una_sola_vez";
    if !hay_modo_termux(prueba) || !hay_script(prueba) {
        return;
    }
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let carpeta = banco.tmp.join("sinc");
    std::fs::create_dir_all(&carpeta).expect("carpeta de sincronización");
    std::fs::create_dir_all(&banco.rutas.config).expect("directorio de configuración");
    std::fs::write(
        banco.rutas.fichero_config(),
        format!("[sincronizacion]\ncarpeta = \"{}\"\n", carpeta.display()),
    )
    .expect("config.toml");
    let (mut sesion, antes, despues) = lanzar_magi_en_pty(&banco, true, &["sincronizar"]);
    sesion.esperar("Frase maestra del llavero de MAGI", PLAZO_PROMPT);
    sesion.escribir(format!("{FRASE_VIEJA}\r").as_bytes());
    // Con la frase dada una vez, la orden llega al final sola. Si volviera a
    // preguntar, se quedaría esperando y esto daría `None`.
    let codigo = sesion.esperar_salida(Duration::from_secs(60));
    let visto = sesion.visto();
    assert!(codigo.is_some(), "volvió a preguntar la frase: {visto:?}");
    assert_eq!(
        visto.matches("Frase maestra del llavero de MAGI").count(),
        1,
        "{visto:?}"
    );
    // El llavero no tiene clave de sincronización: lo dice la pasada, que sí
    // pudo leer el llavero abierto.
    assert_eq!(codigo, Some(1), "{visto:?}");
    assert!(visto.contains("no hay clave en el llavero"), "{visto:?}");
    assert!(!visto.contains(FRASE_VIEJA), "eco de la frase: {visto:?}");
    assert_eq!(modo_de(&antes), modo_de(&despues));
}

#[test]
fn borrar_en_un_terminal_con_el_llavero_danado_sale_con_1_sin_preguntar() {
    let prueba = "borrar_en_un_terminal_con_el_llavero_danado";
    if !hay_modo_termux(prueba) || !hay_script(prueba) {
        return;
    }
    let banco = Banco::nuevo();
    std::fs::write(banco.ruta_llavero(), b"esto no es age").expect("escribir");
    let (mut sesion, antes, despues) = lanzar_magi_en_pty(&banco, true, &["llavero", "borrar"]);
    let codigo = sesion.esperar_salida(Duration::from_secs(30));
    let visto = sesion.visto();
    assert_eq!(codigo, Some(1), "{visto:?}");
    assert!(visto.contains("dañado"), "{visto:?}");
    assert!(visto.contains("abre MAGI"), "{visto:?}");
    assert!(!visto.contains("escribe borrar"), "preguntó: {visto:?}");
    assert_eq!(modo_de(&antes), modo_de(&despues));
    assert_eq!(
        banco.bytes_llavero().as_deref(),
        Some(&b"esto no es age"[..])
    );
    assert!(banco.registro().is_empty());
}

/// Como `lanzar_magi_en_pty`, pero deja el terminal en el modo `modo_tty`
/// (argumentos de `stty`), avisa con el fichero `listo` y espera un segundo
/// antes de arrancar `magi`: da tiempo a que la prueba teclee «por adelantado»
/// (lo que ya está en la cola del terminal cuando `magi` empieza a preguntar).
fn lanzar_magi_en_pty_con_adelanto(
    banco: &Banco,
    argumentos: &[&str],
    modo_tty: &str,
) -> (SesionPty, PathBuf, PathBuf, PathBuf) {
    let antes = banco.tmp.join("modo-antes");
    let despues = banco.tmp.join("modo-despues");
    let listo = banco.tmp.join("listo");
    let mut orden = format!(
        "stty rows 24 cols 80; stty {modo_tty}; stty -g > '{}'; : > '{}'; sleep 1; '{}'",
        antes.display(),
        listo.display(),
        env!("CARGO_BIN_EXE_magi")
    );
    for argumento in argumentos {
        orden.push_str(&format!(" '{argumento}'"));
    }
    orden.push_str(&format!(
        "; c=$?; stty -g > '{}'; exit $c",
        despues.display()
    ));
    let mut script = Command::new("script");
    script.env_clear().envs(banco.entorno(true));
    (SesionPty::lanzar(script, &orden), antes, despues, listo)
}

/// Espera a que exista el fichero `listo` del banco.
fn esperar_fichero(ruta: &Path) {
    let limite = Instant::now() + PLAZO_PROMPT;
    while !ruta.exists() {
        assert!(Instant::now() < limite, "no apareció {}", ruta.display());
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Lo tecleado antes de que aparezca la pregunta de confirmación no la
/// contesta: `magi llavero borrar` vacía la entrada pendiente antes de
/// preguntar (antes, pegar dos líneas confirmaba sin haber leído el aviso).
#[test]
fn borrar_en_un_terminal_no_toma_como_confirmacion_lo_tecleado_por_adelantado() {
    let prueba = "borrar_en_un_terminal_no_toma_como_confirmacion";
    if !hay_modo_termux(prueba) || !hay_script(prueba) {
        return;
    }
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let antes_fichero = banco.bytes_llavero();
    let (mut sesion, antes, despues, listo) =
        lanzar_magi_en_pty_con_adelanto(&banco, &["llavero", "borrar"], "-echo");
    esperar_fichero(&listo);
    // La persona ya había pegado la palabra antes de ver el aviso.
    sesion.escribir(b"borrar\n");
    sesion.esperar("escribe borrar", PLAZO_PROMPT);
    assert_eq!(
        sesion.esperar_salida(Duration::from_millis(1500)),
        None,
        "tomó lo tecleado por adelantado como confirmación: {:?}",
        sesion.visto()
    );
    assert_eq!(banco.bytes_llavero(), antes_fichero, "no se ha borrado");
    // Ya con el aviso a la vista, otra palabra no borra.
    sesion.escribir(b"no\n");
    let codigo = sesion.esperar_salida(Duration::from_secs(30));
    let visto = sesion.visto();
    assert_eq!(codigo, Some(1), "{visto:?}");
    assert!(visto.contains("no se ha borrado nada"), "{visto:?}");
    assert_eq!(modo_de(&antes), modo_de(&despues));
    assert_eq!(banco.bytes_llavero(), antes_fichero);
    assert!(banco.registro().is_empty());
}

/// Lo mismo con la frase: lo tecleado antes de la pregunta no la contesta (ni
/// siquiera cuando el terminal ya estaba en modo crudo y `crossterm` pudo
/// leerlo de golpe).
#[test]
fn cambiar_en_un_terminal_no_toma_como_frase_lo_tecleado_por_adelantado() {
    let prueba = "cambiar_en_un_terminal_no_toma_como_frase";
    if !hay_modo_termux(prueba) || !hay_script(prueba) {
        return;
    }
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let antes_fichero = banco.bytes_llavero();
    let (mut sesion, antes, despues, listo) =
        lanzar_magi_en_pty_con_adelanto(&banco, &["llavero", "cambiar"], "raw -echo");
    esperar_fichero(&listo);
    // Dos líneas seguidas, como un pegado: la actual y la nueva.
    sesion.escribir(format!("{FRASE_VIEJA}\r{FRASE_NUEVA}\r").as_bytes());
    sesion.esperar("Frase maestra actual", PLAZO_PROMPT);
    std::thread::sleep(Duration::from_millis(1500));
    assert!(
        !sesion.visto().contains("Frase maestra nueva"),
        "contestó la pregunta con lo tecleado por adelantado: {:?}",
        sesion.visto()
    );
    // Ctrl+C cancela: nada cambia.
    sesion.escribir(&[0x03]);
    let codigo = sesion.esperar_salida(Duration::from_secs(30));
    let visto = sesion.visto();
    assert_eq!(codigo, Some(1), "{visto:?}");
    assert!(visto.contains("Cancelado"), "{visto:?}");
    assert_eq!(modo_de(&antes), modo_de(&despues));
    assert_eq!(banco.bytes_llavero(), antes_fichero);
    assert!(banco.registro().is_empty());
}

/// Un pegado de varias líneas llega de golpe: crossterm lee todas y las deja
/// en su cola interna. Solo la primera contesta la pregunta que se ve; las
/// demás se descartan antes de la siguiente (el vaciado de la cola del terminal
/// no basta: ya no están ahí).
#[test]
fn cambiar_en_un_terminal_no_toma_como_respuesta_lo_pegado_tras_la_primera_linea() {
    let prueba = "cambiar_en_un_terminal_no_toma_como_respuesta_lo_pegado";
    if !hay_modo_termux(prueba) || !hay_script(prueba) {
        return;
    }
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let antes_fichero = banco.bytes_llavero();
    let (mut sesion, antes, despues) = lanzar_magi_en_pty(&banco, true, &["llavero", "cambiar"]);
    sesion.esperar("Frase maestra actual", PLAZO_PROMPT);
    // Las tres frases de golpe, en una sola escritura.
    sesion.escribir(format!("{FRASE_VIEJA}\r{FRASE_NUEVA}\r{FRASE_NUEVA}\r").as_bytes());
    sesion.esperar("Frase maestra nueva", PLAZO_PROMPT);
    std::thread::sleep(Duration::from_millis(1500));
    assert!(
        !sesion.visto().contains("Repite la frase nueva"),
        "contestó la segunda pregunta con lo pegado: {:?}",
        sesion.visto()
    );
    sesion.escribir(&[0x03]);
    let codigo = sesion.esperar_salida(Duration::from_secs(30));
    let visto = sesion.visto();
    assert_eq!(codigo, Some(1), "{visto:?}");
    assert!(visto.contains("Cancelado"), "{visto:?}");
    assert_eq!(modo_de(&antes), modo_de(&despues));
    assert_eq!(banco.bytes_llavero(), antes_fichero);
    assert!(banco.registro().is_empty());
}

/// Mata el proceso al soltarse: una prueba que falla no deja un `magi`
/// girando.
struct MatarAlSalir(i32);

impl Drop for MatarAlSalir {
    fn drop(&mut self) {
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(self.0),
            nix::sys::signal::Signal::SIGKILL,
        );
    }
}

/// ¿Sigue vivo el proceso? Un zombi ya no cuenta.
fn proceso_vivo(pid: i32) -> bool {
    let Ok(estado) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    // «pid (nombre) E …»: el estado va tras el último paréntesis.
    estado
        .rsplit_once(')')
        .and_then(|(_, resto)| resto.trim_start().chars().next())
        .is_some_and(|letra| letra != 'Z' && letra != 'X')
}

/// Si el terminal desaparece (se cierra el maestro del pseudoterminal) con
/// SIGHUP ignorado, la lectura de la frase acaba con error en vez de girar al
/// 99 % de CPU para siempre (`crossterm` no devuelve error ante el fin de
/// fichero del terminal: se queda dando vueltas dentro de su lectura).
#[test]
fn si_el_terminal_desaparece_la_lectura_de_la_frase_acaba_en_vez_de_girar() {
    let prueba = "si_el_terminal_desaparece_la_lectura_de_la_frase";
    if !hay_modo_termux(prueba) || !hay_script(prueba) {
        return;
    }
    let banco = Banco::nuevo();
    banco.crear(FRASE_VIEJA);
    let ficha_pid = banco.tmp.join("pid-magi");
    // `trap '' HUP` + `exec`: `magi` hereda SIGHUP ignorado y conserva el pid
    // que apunta el shell.
    let orden = format!(
        "trap '' HUP; echo $$ > '{}'; exec '{}' llavero cambiar",
        ficha_pid.display(),
        env!("CARGO_BIN_EXE_magi")
    );
    let mut script = Command::new("script");
    script.env_clear().envs(banco.entorno(true));
    let mut sesion = SesionPty::lanzar(script, &orden);
    sesion.esperar("Frase maestra actual", PLAZO_PROMPT);
    let pid: i32 = std::fs::read_to_string(&ficha_pid)
        .expect("pid de magi")
        .trim()
        .parse()
        .expect("pid numérico");
    let _guarda = MatarAlSalir(pid);
    assert!(proceso_vivo(pid), "magi sigue esperando la frase");

    // Desaparece el maestro del terminal (`script` muere): el terminal de
    // `magi` cuelga y su lectura da fin de fichero.
    sesion.hijo.kill().expect("matar script");
    let _ = sesion.hijo.wait();

    let limite = Instant::now() + Duration::from_secs(15);
    while proceso_vivo(pid) && Instant::now() < limite {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        !proceso_vivo(pid),
        "magi sigue vivo (girando) tras perder el terminal"
    );
    // Nada cambió en el llavero.
    assert!(banco.registro().is_empty());
}
