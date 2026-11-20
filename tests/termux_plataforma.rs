//! La rama de Termux de `config` y de la deliberación (Fase 17, Sprint 1):
//! con `MAGI_PLATAFORMA=termux`, que solo vale en compilaciones de depuración,
//! este proceso se porta como un MAGI de Termux aunque corra en Linux.
//!
//! `es_termux()` se decide una vez por proceso, por eso estas pruebas viven en
//! su propio binario: todas empiezan por `Termux::con`, que pone la variable
//! antes de que nadie pregunte. La rama de Linux se prueba en `termux_ordenes`.
//!
//! Todo el fichero es solo de depuración (`debug_assertions`), que es donde
//! `MAGI_PLATAFORMA=termux` vale: en `cargo test --release` la variable no
//! activa nada y no hay nada que probar. Se deja fuera de la compilación en
//! vez de hacer que cada prueba salga sin comprobar, para que no cuente como
//! pasada sin haber corrido.
#![cfg(debug_assertions)]

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use magi::deliberacion::{Proveedores, ProveedoresEscritorio};
use magi::termux::{es_termux, EntornoTermux};

static ENTORNO: Mutex<()> = Mutex::new(());

/// Pone `MAGI_PLATAFORMA=termux` y las variables de Termux que se pidan, con
/// el entorno bajo cerrojo; al soltarlo restaura lo que había.
struct Termux {
    previos: Vec<(&'static str, Option<OsString>)>,
    _cerrojo: MutexGuard<'static, ()>,
}

impl Termux {
    fn con(cambios: &[(&'static str, Option<&str>)]) -> Self {
        let cerrojo = ENTORNO.lock().unwrap_or_else(|e| e.into_inner());
        let mut previos = Vec::new();
        let todos =
            std::iter::once(("MAGI_PLATAFORMA", Some("termux"))).chain(cambios.iter().copied());
        for (nombre, valor) in todos {
            previos.push((nombre, std::env::var_os(nombre)));
            match valor {
                Some(valor) => std::env::set_var(nombre, valor),
                None => std::env::remove_var(nombre),
            }
        }
        let guarda = Self {
            previos,
            _cerrojo: cerrojo,
        };
        // Aquí la variable vale (compilación de depuración): si el proceso no
        // se porta como Termux es que alguien preguntó antes de ponerla, y la
        // prueba no comprobaría nada. Es un fallo, no una omisión.
        assert!(
            es_termux(),
            "es_termux() ya se decidió sin MAGI_PLATAFORMA=termux en este proceso"
        );
        guarda
    }
}

impl Drop for Termux {
    fn drop(&mut self) {
        for (nombre, valor) in self.previos.drain(..).rev() {
            match valor {
                Some(valor) => std::env::set_var(nombre, valor),
                None => std::env::remove_var(nombre),
            }
        }
    }
}

#[test]
fn en_termux_la_plataforma_es_android_y_el_equipo_se_llama_termux() {
    let _termux = Termux::con(&[]);
    assert_eq!(magi::config::plataforma_actual(), "android");
    // En Linux no hay propiedades de Android: sin modelo, «termux» a secas
    // (en un móvil, `termux-<modelo>`); nunca el `localhost` de Termux.
    #[cfg(not(target_os = "android"))]
    assert_eq!(magi::config::nombre_actual(), "termux");
    assert!(!magi::config::nombre_actual().is_empty());
}

/// §3.2 y §7.8: el usuario de las deliberaciones en Termux es el nombre del
/// equipo (`termux-<modelo>`), no el del uid de Android (`u0_a123`).
#[test]
fn en_termux_el_usuario_de_las_deliberaciones_es_el_nombre_del_equipo() {
    let _termux = Termux::con(&[]);
    let usuario = magi::deliberacion::usuario_del_sistema();
    assert_eq!(usuario, magi::config::nombre_actual());
    // En Linux no hay propiedades de Android: sin modelo, «termux» a secas.
    #[cfg(not(target_os = "android"))]
    assert_eq!(usuario, "termux");
    // Nunca el del uid del proceso, que aquí es el del usuario que corre la
    // prueba (en un móvil sería `u0_aNNN`).
    let del_uid = nix::unistd::User::from_uid(nix::unistd::Uid::current())
        .ok()
        .flatten()
        .map(|usuario| usuario.name);
    assert_ne!(Some(usuario.clone()), del_uid);
    assert!(usuario.starts_with("termux"), "{usuario}");
    // Con un modelo, `termux-<modelo>` con la regla de la etiqueta.
    let propiedades = magi::termux::Propiedades::fijas(&[("ro.product.model", "ABC-X10")]);
    assert_eq!(
        magi::deliberacion::usuario_de_termux(&propiedades),
        "termux-abc-x10"
    );
}

/// En Termux la copia de la clave recién generada no es de la tarea de
/// generación (no hay `wl-copy` ni `pbcopy`): la hace la `App` por la única vía
/// (`termux-clipboard-set` u OSC 52). La tarea ni la intenta ni dice que falló.
#[tokio::test]
async fn en_termux_la_tarea_de_generacion_no_copia_la_publica() {
    let temporal = tempfile::tempdir().unwrap();
    // `es_termux()` se decide una vez por proceso: basta con haber puesto la
    // variable antes de la primera pregunta, y la guarda del entorno no cruza
    // el `await`.
    drop(Termux::con(&[]));
    let peticion = magi::identidades::PeticionGeneracion {
        dir_ssh: temporal.path().to_path_buf(),
        nombre: "id_termux".to_string(),
        tipo: magi::identidades::TipoClave::Ed25519,
        comentario: "usuario@ejemplo.org".to_string(),
        frase: zeroize::Zeroizing::new(String::new()),
        anadir_agente: false,
        copiar_publica: true,
    };
    let clave = magi::identidades::generar_con(peticion, |_| -> Result<(), String> {
        panic!("en Termux no copia la tarea")
    })
    .await
    .expect("clave generada");
    assert!(clave.solicito_copia, "el diálogo pidió copiarla");
    assert!(!clave.copiada);
    assert_eq!(clave.fallo_copia, None, "no lo intentó: no falló");
    assert!(clave.publica.starts_with("ssh-ed25519 "));
}

#[test]
fn en_termux_descubrir_rutas_sale_del_entorno_y_no_del_uid() {
    let temporal = tempfile::tempdir().unwrap();
    let home = temporal.path().join("home");
    let prefix = temporal.path().join("usr");
    let texto = |ruta: &Path| ruta.to_string_lossy().into_owned();
    let _termux = Termux::con(&[
        ("HOME", Some(&texto(&home))),
        ("PREFIX", Some(&texto(&prefix))),
        ("XDG_DATA_HOME", None),
        ("XDG_CONFIG_HOME", None),
        ("XDG_STATE_HOME", None),
        ("XDG_RUNTIME_DIR", None),
    ]);
    let rutas = magi::config::descubrir_rutas().expect("rutas de Termux");
    assert_eq!(rutas.hogar, home);
    assert_eq!(rutas.datos, home.join(".local/share/magi"));
    assert_eq!(rutas.config, home.join(".config/magi"));
    assert_eq!(rutas.estado, home.join(".local/state/magi"));
    let esperadas = magi::termux::rutas::calcular(&EntornoTermux::del_proceso()).unwrap();
    assert_eq!(rutas.runtime, esperadas.runtime);
    assert!(
        rutas.runtime.to_string_lossy().contains("/magi-"),
        "sin XDG_RUNTIME_DIR, el socket va en magi-<uid>: {}",
        rutas.runtime.display()
    );
}

#[test]
fn en_termux_sin_prefix_las_rutas_fallan_con_una_instruccion() {
    let _termux = Termux::con(&[("HOME", Some("/home/usuario")), ("PREFIX", None)]);
    let error = magi::config::descubrir_rutas().unwrap_err();
    let texto = format!("{error:#}");
    assert!(
        texto.contains("PREFIX") && texto.contains("Termux"),
        "{texto}"
    );
}

#[tokio::test]
async fn en_termux_el_comando_local_de_casper_corre_con_el_sh_del_prefijo() {
    let temporal = tempfile::tempdir().unwrap();
    let home = temporal.path().join("home");
    let prefix = temporal.path().join("usr");
    std::fs::create_dir_all(prefix.join("bin")).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    // El `$PREFIX/bin/sh` de este Termux de mentira: un enlace al sh del
    // sistema (dentro de `sh -c`, `$0` dice con qué ruta se lanzó).
    let sh = prefix.join("bin/sh");
    std::os::unix::fs::symlink("/bin/sh", &sh).unwrap();
    let texto = |ruta: &Path| ruta.to_string_lossy().into_owned();
    let proveedores = {
        // El proveedor fija la shell al construirse, como el diálogo.
        let _termux = Termux::con(&[
            ("HOME", Some(&texto(&home))),
            ("PREFIX", Some(&texto(&prefix))),
        ]);
        let (cliente, _recibido) = magi::cliente::Cliente::de_prueba();
        ProveedoresEscritorio::nuevo(
            cliente,
            HashMap::new(),
            home.clone(),
            tokio::time::Instant::now() + Duration::from_secs(10),
        )
    };
    let marca = temporal.path().join("shell-usada");
    let comando = format!("printf '%s' \"$0\" > '{}'", marca.display());
    let resultado = proveedores.tests(1, comando).await;
    assert_eq!(resultado.unwrap(), (true, "verde".to_string()));
    assert_eq!(std::fs::read_to_string(&marca).unwrap(), texto(&sh));
}

#[tokio::test]
async fn en_termux_sin_prefix_el_comando_local_rechaza_con_la_instruccion() {
    let temporal = tempfile::tempdir().unwrap();
    let proveedores = {
        let _termux = Termux::con(&[("HOME", Some("/home/usuario")), ("PREFIX", None)]);
        let (cliente, _recibido) = magi::cliente::Cliente::de_prueba();
        ProveedoresEscritorio::nuevo(
            cliente,
            HashMap::new(),
            temporal.path().to_path_buf(),
            tokio::time::Instant::now() + Duration::from_secs(10),
        )
    };
    let (aprueba, motivo) = proveedores
        .tests(1, "true".to_string())
        .await
        .expect("el comando local se puede comprobar");
    assert!(!aprueba);
    assert!(motivo.contains("PREFIX"), "{motivo}");
}
