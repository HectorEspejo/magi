//! Pruebas de `servicios.sh` ejecutándolo de verdad con `sh` y binarios falsos
//! en el `PATH`: argumentos, privilegios, contraseña por la entrada estándar,
//! comprobaciones y espera al estado final. Sin red y sin systemd real.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[path = "fijos/servicios/binarios.rs"]
mod binarios;

use binarios::BinariosFalsos;

const SCRIPT: &str = magi_nucleo::servicios::orden::SCRIPT;

struct Salida {
    stdout: String,
    stderr: String,
    codigo: i32,
}

impl Salida {
    fn todo(&self) -> String {
        format!("{}\n{}", self.stdout, self.stderr)
    }
}

/// Driver de `servicios.sh` para estas pruebas: los binarios falsos
/// compartidos de `fijos/servicios/` van en el `PATH` y su estado, en un
/// temporal (lo instala y lo prepara el helper `binarios`).
struct Falsos {
    binarios: BinariosFalsos,
}

impl Falsos {
    fn nuevo() -> Self {
        let binarios = BinariosFalsos::nuevos();
        binarios.preparar_estado();
        Self { binarios }
    }

    fn escribir(&self, nombre: &str, contenido: &str) {
        self.binarios.escribir(nombre, contenido);
    }

    fn llamadas(&self) -> String {
        self.binarios.llamadas()
    }

    fn path(&self) -> String {
        format!(
            "{}:{}",
            self.binarios.ruta().display(),
            std::env::var("PATH").unwrap_or_default()
        )
    }

    fn comando(&self) -> Command {
        let mut comando = Command::new("sh");
        comando
            .env("PATH", self.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        comando
    }

    fn ruta_estado(&self) -> &Path {
        self.binarios.estado()
    }

    fn ejecutar_crudo(&self, argumentos: &[&str], clave: &str) -> Salida {
        let mut comando = self.comando();
        comando.arg("-c").arg(SCRIPT).arg("magi-servicios");
        for argumento in argumentos {
            comando.arg(argumento);
        }
        let mut hijo = comando.spawn().expect("arrancar script");
        Self::meter_clave(&mut hijo, clave);
        let salida = hijo.wait_with_output().expect("esperar script");
        Salida {
            stdout: String::from_utf8_lossy(&salida.stdout).to_string(),
            stderr: String::from_utf8_lossy(&salida.stderr).to_string(),
            codigo: salida.status.code().unwrap_or(-1),
        }
    }

    fn ejecutar(
        &self,
        suborden: &str,
        privilegio: &str,
        argumentos: &[&str],
        clave: &str,
    ) -> Salida {
        let mut todos = vec![suborden, privilegio];
        todos.extend_from_slice(argumentos);
        self.ejecutar_crudo(&todos, clave)
    }

    fn existe_estado(&self, nombre: &str) -> bool {
        self.ruta_estado().join(nombre).exists()
    }

    /// Espera a que el falso señale que ya tiene instalado su trap de TERM.
    fn esperar_estado(&self, nombre: &str) {
        let limite = Instant::now() + Duration::from_secs(5);
        while !self.existe_estado(nombre) {
            if Instant::now() > limite {
                panic!("el falso no dejó el estado {nombre}");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn meter_clave(hijo: &mut Child, clave: &str) {
        let Some(stdin) = hijo.stdin.as_mut() else {
            return;
        };
        // Con argumentos inválidos el script sale con 64 antes de leer la
        // entrada estándar: el tubo roto es normal, no un fallo de la prueba.
        match stdin.write_all(format!("{clave}\n").as_bytes()) {
            Ok(()) => {
                let _ = stdin.flush();
            }
            Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => {}
            Err(error) => panic!("escribir clave: {error}"),
        }
    }

    /// Lanza el script dejando la entrada estándar abierta (para los logs).
    fn lanzar(
        &self,
        suborden: &str,
        privilegio: &str,
        argumentos: &[&str],
        clave: &str,
    ) -> (Child, BufReader<std::process::ChildStdout>) {
        let mut comando = self.comando();
        comando.arg("-c").arg(SCRIPT).arg("magi-servicios");
        for argumento in [suborden, privilegio].iter().chain(argumentos.iter()) {
            comando.arg(argumento);
        }
        let mut hijo = comando.spawn().expect("arrancar script");
        Self::meter_clave(&mut hijo, clave);
        let lector = BufReader::new(hijo.stdout.take().expect("stdout"));
        (hijo, lector)
    }
}

fn lineas_magi(salida: &str, prefijo: &str) -> Vec<String> {
    salida
        .lines()
        .filter_map(|linea| linea.strip_prefix(prefijo))
        .map(|resto| resto.trim().to_string())
        .collect()
}

fn valor_magi(salida: &str, clave: &str) -> Option<String> {
    salida
        .lines()
        .find_map(|linea| linea.strip_prefix(clave))
        .map(|resto| resto.trim().to_string())
}

fn esperar_salida(mut hijo: Child, plazo: Duration) -> i32 {
    let limite = Instant::now() + plazo;
    loop {
        match hijo.try_wait() {
            Ok(Some(estado)) => return estado.code().unwrap_or(-1),
            Ok(None) => {
                if Instant::now() > limite {
                    let _ = hijo.kill();
                    throw("el script no terminó al cerrar la entrada estándar");
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => panic!("esperando el script: {error}"),
        }
    }
}

fn throw(mensaje: &str) -> ! {
    panic!("{mensaje}")
}

// ------------------------------------------------------------------ argumentos
#[test]
fn argumentos_invalidos_salen_con_64() {
    let falsos = Falsos::nuevo();
    let sin_argumentos = falsos.ejecutar_crudo(&[], "");
    assert_eq!(sin_argumentos.codigo, 64);
    assert!(sin_argumentos.stderr.contains("ERROR argumentos"));

    let privilegio_malo = falsos.ejecutar("privilegio", "poderoso", &[], "");
    assert_eq!(privilegio_malo.codigo, 64);
    assert!(privilegio_malo.stderr.contains("ERROR privilegio"));

    for (suborden, argumentos) in [
        ("desconocido", vec![]),
        ("listar", vec!["lxc"]),
        ("consumo", vec!["lxc"]),
        ("detalle", vec!["unidad"]),
        ("logs", vec!["unidad", "app.service", "muchas"]),
        ("logs", vec!["contenedor", "no vale", "10"]),
        (
            "accion",
            vec!["unidad", "app.service", "volar", "30", "ninguna"],
        ),
        (
            "accion",
            vec!["unidad", "app.service", "reiniciar", "0", "ninguna"],
        ),
        (
            "accion",
            vec!["unidad", "app.service", "reiniciar", "30", "quizá"],
        ),
        (
            "accion",
            vec!["proyecto", "web", "recargar", "30", "ninguna"],
        ),
    ] {
        let salida = falsos.ejecutar(suborden, "root", &argumentos, "");
        assert_eq!(
            salida.codigo,
            64,
            "{suborden} {argumentos:?}: {}",
            salida.todo()
        );
    }
    assert!(!falsos.llamadas().contains("systemctl restart"));
}

#[test]
fn un_objetivo_con_punto_y_coma_se_rechaza_antes_de_tocar_el_host() {
    let falsos = Falsos::nuevo();
    let salida = falsos.ejecutar(
        "accion",
        "root",
        &[
            "unidad",
            "nginx.service; rm -rf /",
            "reiniciar",
            "30",
            "ninguna",
        ],
        "",
    );
    assert_eq!(salida.codigo, 64, "{}", salida.todo());
    assert!(
        salida.stderr.contains("ERROR objetivo"),
        "{}",
        salida.stderr
    );
    let llamadas = falsos.llamadas();
    assert!(
        !llamadas.contains("systemctl"),
        "no se debió llamar a systemctl: {llamadas}"
    );
}

// ------------------------------------------------------------------ privilegios
#[test]
fn privilegio_detecta_los_tres_modos_de_sudo() {
    let falsos = Falsos::nuevo();
    for (modo, esperado) in [
        ("sin_clave", "sudo_sin_clave"),
        ("con_clave", "sudo_con_clave"),
        ("sin_sudo", "sin_sudo"),
    ] {
        falsos.escribir("sudo_modo", modo);
        let salida = falsos.ejecutar("privilegio", "sin_sudo", &[], "");
        assert_eq!(salida.codigo, 0, "{modo}: {}", salida.todo());
        assert_eq!(
            valor_magi(&salida.stdout, "MAGI_PRIV=").as_deref(),
            Some(esperado)
        );
        assert!(valor_magi(&salida.stdout, "MAGI_UID=").is_some());
        assert!(salida.stdout.contains("MAGI_FIN"));
    }
}

#[test]
fn sudo_comprueba_la_contrasena_por_la_entrada_estandar() {
    let falsos = Falsos::nuevo();
    falsos.escribir("sudo_modo", "con_clave");
    let buena = falsos.ejecutar("sudo", "sudo_con_clave", &[], "secreta");
    assert_eq!(
        valor_magi(&buena.stdout, "MAGI_SUDO=").as_deref(),
        Some("ok")
    );
    let mala = falsos.ejecutar("sudo", "sudo_con_clave", &[], "otra");
    assert_eq!(
        valor_magi(&mala.stdout, "MAGI_SUDO=").as_deref(),
        Some("rechazada")
    );
    falsos.escribir("sudo_modo", "sin_sudo");
    let sin_sudo = falsos.ejecutar("sudo", "sudo_con_clave", &[], "secreta");
    assert_eq!(
        valor_magi(&sin_sudo.stdout, "MAGI_SUDO=").as_deref(),
        Some("sin_sudo")
    );
    // La contraseña no puede aparecer en la salida ni en el registro de llamadas.
    assert!(!buena.todo().contains("secreta"));
    assert!(!falsos.llamadas().contains("secreta"));
}

// ------------------------------------------------------------------ listar
#[test]
fn listar_lee_systemd_y_dice_cuando_no_hay_motor() {
    let falsos = Falsos::nuevo();
    let salida = falsos.ejecutar("listar", "sin_sudo", &["-"], "");
    assert_eq!(salida.codigo, 0, "{}", salida.todo());
    assert_eq!(
        valor_magi(&salida.stdout, "MAGI_SYSTEMD=").as_deref(),
        Some("252")
    );
    assert_eq!(
        valor_magi(&salida.stdout, "MAGI_MOTOR=").as_deref(),
        Some("no")
    );
    assert!(salida.stdout.contains("MAGI_BLOQUE service"));
    assert!(salida.stdout.contains("Id=app.service"));
    assert!(salida.stdout.contains("MAGI_FIN_BLOQUE"));
    assert!(valor_magi(&salida.stdout, "MAGI_DIARIO=").is_some());
    assert!(valor_magi(&salida.stdout, "MAGI_RELOJ=").is_some());
    assert!(salida.stdout.contains("MAGI_FIN"));
}

#[test]
fn listar_con_docker_sin_permiso_lo_dice() {
    let falsos = Falsos::nuevo();
    falsos.escribir("docker_sin_permiso", "");
    let salida = falsos.ejecutar("listar", "sin_sudo", &["docker"], "");
    assert_eq!(salida.codigo, 0);
    assert_eq!(
        valor_magi(&salida.stdout, "MAGI_MOTOR=").as_deref(),
        Some("docker sin_permiso")
    );
    assert!(lineas_magi(&salida.stdout, "MAGI_CONTENEDOR ").is_empty());
}

#[test]
fn listar_reintenta_con_sudo_solo_lo_que_exige_permiso() {
    let falsos = Falsos::nuevo();
    falsos.escribir("sudo_modo", "sin_clave");
    falsos.escribir("docker_sin_permiso", "");
    let salida = falsos.ejecutar("listar", "sudo_sin_clave", &["docker"], "");
    assert_eq!(salida.codigo, 0, "{}", salida.todo());
    assert_eq!(
        valor_magi(&salida.stdout, "MAGI_MOTOR=").as_deref(),
        Some("docker 27.3.1")
    );
    let llamadas = falsos.llamadas();
    assert!(
        llamadas.contains("sudo -n -- docker ps -a -q"),
        "{llamadas}"
    );
    assert!(!llamadas.contains("sh -c"));
    assert!(salida.stdout.contains("Id=app.service"));
}

#[test]
fn listar_con_podman_nunca_usa_sudo() {
    let falsos = Falsos::nuevo();
    falsos.escribir("sudo_modo", "con_clave");
    let salida = falsos.ejecutar("listar", "sin_sudo", &["podman"], "");
    assert_eq!(salida.codigo, 0);
    assert_eq!(
        valor_magi(&salida.stdout, "MAGI_MOTOR=").as_deref(),
        Some("podman 5.2.3")
    );
    assert!(!falsos.llamadas().contains("sudo"), "{}", falsos.llamadas());
}

// ------------------------------------------------------------------ detalle
#[test]
fn detalle_lee_propiedades_dependencias_y_texto() {
    let falsos = Falsos::nuevo();
    falsos.escribir(
        "deps/nginx.service",
        "nginx.service\n● ├─network.target\n● ├─system.slice\n\
         ● │ ├─hondo.target\n● └─systemd-journald.socket\n",
    );
    falsos.escribir(
        "deps_reverse/nginx.service",
        "nginx.service\n● └─multi-user.target\n",
    );
    falsos.escribir(
        "cat/nginx.service",
        "# /lib/systemd/system/nginx.service\n[Service]\nExecStart=/usr/sbin/nginx\n",
    );
    let salida = falsos.ejecutar("detalle", "root", &["unidad", "nginx.service"], "");
    assert_eq!(salida.codigo, 0, "{}", salida.todo());
    assert!(salida.stdout.contains("MAGI_PROP ActiveState=active"));
    let necesita = lineas_magi(&salida.stdout, "MAGI_DEP necesita ");
    assert!(
        necesita.contains(&"network.target".to_string()),
        "{necesita:?}"
    );
    assert!(necesita.contains(&"system.slice".to_string()));
    assert!(necesita.contains(&"systemd-journald.socket".to_string()));
    assert!(
        !necesita.contains(&"hondo.target".to_string()),
        "el nivel 2 no sale"
    );
    assert_eq!(
        lineas_magi(&salida.stdout, "MAGI_DEP necesitada "),
        vec!["multi-user.target".to_string()]
    );
    assert!(salida.stdout.contains("MAGI_TEXTO"));
    assert!(salida.stdout.contains("ExecStart=/usr/sbin/nginx"));
    assert!(salida.stdout.contains("MAGI_FIN"));
}

#[test]
fn detalle_de_un_contenedor_usa_el_inspect() {
    let falsos = Falsos::nuevo();
    let salida = falsos.ejecutar("detalle", "root", &["contenedor", "web-app-1"], "");
    assert_eq!(salida.codigo, 0, "{}", salida.todo());
    assert!(salida.stdout.contains("MAGI_TEXTO"));
    assert!(salida.stdout.contains("Id"));
    let llamadas = falsos.llamadas();
    assert!(llamadas.contains("docker inspect web-app-1"), "{llamadas}");
}

// ------------------------------------------------------------------ consumo
#[test]
fn consumo_lee_las_estadisticas_de_cada_motor() {
    let falsos = Falsos::nuevo();
    falsos.escribir(
        "docker_stats",
        "{\"ID\":\"a1b2c3d4e5f6\",\"CPUPerc\":\"0.12%\",\"MemUsage\":\"12.3MiB / 7.6GiB\"}\n",
    );
    let salida = falsos.ejecutar("consumo", "root", &["docker"], "");
    assert_eq!(salida.codigo, 0, "{}", salida.todo());
    assert_eq!(lineas_magi(&salida.stdout, "MAGI_CONSUMO ").len(), 1);
    let podman = falsos.ejecutar("consumo", "root", &["podman"], "");
    assert_eq!(podman.codigo, 0);
    assert!(podman.stdout.contains("MAGI_FIN"));
}

#[test]
fn consumo_sin_permiso_sale_con_69() {
    let falsos = Falsos::nuevo();
    falsos.escribir("docker_sin_permiso", "");
    let salida = falsos.ejecutar("consumo", "sin_sudo", &["docker"], "");
    assert_eq!(salida.codigo, 69, "{}", salida.todo());
    assert!(salida.stderr.contains("ERROR sin_permiso"));
}

// ------------------------------------------------------------------ acciones
#[test]
fn accion_reinicia_con_sudo_orden_a_orden() {
    let falsos = Falsos::nuevo();
    falsos.escribir("accion_estado", "active");
    let salida = falsos.ejecutar(
        "accion",
        "sudo_sin_clave",
        &["unidad", "app.service", "reiniciar", "30", "ninguna"],
        "",
    );
    assert_eq!(salida.codigo, 0, "{}", salida.todo());
    assert!(salida.stdout.contains("MAGI_FASE=ejecutando"));
    assert!(salida.stdout.contains("MAGI_FASE=esperando"));
    assert!(
        salida.stdout.contains("MAGI_RESULTADO=hecho active"),
        "{}",
        salida.stdout
    );
    let llamadas = falsos.llamadas();
    assert!(
        llamadas.contains("sudo -n -- systemctl restart app.service"),
        "{llamadas}"
    );
    assert!(
        llamadas.contains("systemctl restart app.service"),
        "{llamadas}"
    );
    assert!(
        !llamadas.contains("sh -c"),
        "sudo orden a orden: {llamadas}"
    );
}

#[test]
fn accion_con_contrasena_por_la_entrada_estandar() {
    let falsos = Falsos::nuevo();
    falsos.escribir("sudo_modo", "con_clave");
    falsos.escribir("accion_estado", "active");
    let salida = falsos.ejecutar(
        "accion",
        "sudo_con_clave",
        &["unidad", "app.service", "reiniciar", "30", "ninguna"],
        "secreta",
    );
    assert_eq!(salida.codigo, 0, "{}", salida.todo());
    assert!(salida.stdout.contains("MAGI_RESULTADO=hecho active"));
    let llamadas = falsos.llamadas();
    assert!(llamadas.contains("sudo -S"), "{llamadas}");
    assert!(
        llamadas.contains("systemctl restart app.service"),
        "{llamadas}"
    );
    assert!(!llamadas.contains("secreta"));
    assert!(!salida.todo().contains("secreta"));
}

#[test]
fn accion_con_contrasena_mala_no_reinicia() {
    let falsos = Falsos::nuevo();
    falsos.escribir("sudo_modo", "con_clave");
    falsos.escribir("accion_estado", "active");
    let salida = falsos.ejecutar(
        "accion",
        "sudo_con_clave",
        &["unidad", "app.service", "reiniciar", "30", "ninguna"],
        "mala",
    );
    assert_eq!(salida.codigo, 0, "{}", salida.todo());
    assert!(
        salida.stdout.contains("MAGI_RESULTADO=fallido"),
        "{}",
        salida.stdout
    );
    assert!(
        !falsos
            .llamadas()
            .lines()
            .any(|linea| linea.starts_with("systemctl restart")),
        "{}",
        falsos.llamadas()
    );
}

#[test]
fn accion_sin_sudo_sale_con_77() {
    let falsos = Falsos::nuevo();
    falsos.escribir("sudo_modo", "sin_sudo");
    let salida = falsos.ejecutar(
        "accion",
        "sin_sudo",
        &["unidad", "app.service", "reiniciar", "30", "ninguna"],
        "",
    );
    assert_eq!(salida.codigo, 77, "{}", salida.todo());
    assert!(salida.stderr.contains("ERROR sin_privilegio"));
    assert!(!falsos.llamadas().contains("systemctl restart"));
}

#[test]
fn la_comprobacion_de_nginx_falla_y_no_recarga() {
    let falsos = Falsos::nuevo();
    falsos.escribir("sudo_modo", "sin_clave");
    falsos.escribir("nginx_codigo", "1");
    falsos.escribir(
        "nginx_salida",
        "nginx: [emerg] unexpected end of file in /etc/nginx/nginx.conf:42\n",
    );
    let salida = falsos.ejecutar(
        "accion",
        "sudo_sin_clave",
        &["unidad", "nginx.service", "recargar", "30", "conocida"],
        "",
    );
    assert_eq!(salida.codigo, 0, "{}", salida.todo());
    assert!(
        salida.stdout.contains("MAGI_COMPROBACION=fallo 1"),
        "{}",
        salida.stdout
    );
    assert!(salida
        .stdout
        .contains("MAGI_RESULTADO=rechazada_configuracion"));
    assert!(lineas_magi(&salida.stdout, "MAGI_LINEA ")
        .iter()
        .any(|linea| linea.contains("unexpected end of file")));
    let llamadas = falsos.llamadas();
    assert!(llamadas.contains("nginx -t"), "{llamadas}");
    assert!(!llamadas.contains("systemctl reload"), "{llamadas}");
}

#[test]
fn la_comprobacion_ok_va_justo_antes_de_recargar() {
    let falsos = Falsos::nuevo();
    falsos.escribir("sudo_modo", "sin_clave");
    falsos.escribir("accion_estado", "active");
    let salida = falsos.ejecutar(
        "accion",
        "sudo_sin_clave",
        &["unidad", "nginx.service", "recargar", "30", "conocida"],
        "",
    );
    assert_eq!(salida.codigo, 0, "{}", salida.todo());
    assert!(salida.stdout.contains("MAGI_COMPROBACION=ok"));
    assert!(salida.stdout.contains("MAGI_RESULTADO=hecho active"));
    let llamadas = falsos.llamadas();
    let comprobacion = llamadas.find("nginx -t").expect("nginx -t");
    let recarga = llamadas
        .find("systemctl reload nginx.service")
        .expect("reload");
    assert!(comprobacion < recarga, "{llamadas}");
}

#[test]
fn accion_fallida_trae_mensaje_y_ultimas_lineas() {
    let falsos = Falsos::nuevo();
    falsos.escribir("accion_codigo", "1");
    falsos.escribir(
        "journal_short/app.service",
        "21:52:10 app[4121]: KeyError: DATABASE_URL\n",
    );
    let salida = falsos.ejecutar(
        "accion",
        "root",
        &["unidad", "app.service", "reiniciar", "30", "ninguna"],
        "",
    );
    assert_eq!(salida.codigo, 0, "{}", salida.todo());
    assert!(
        salida.stdout.contains("MAGI_RESULTADO=fallido"),
        "{}",
        salida.stdout
    );
    assert!(salida.stdout.contains("MAGI_MENSAJE"));
    assert!(lineas_magi(&salida.stdout, "MAGI_LINEA ")
        .iter()
        .any(|linea| linea.contains("KeyError")));
}

#[test]
fn accion_que_no_vuelve_es_sin_confirmar() {
    let falsos = Falsos::nuevo();
    falsos.escribir("accion_dormir", "3");
    let salida = falsos.ejecutar(
        "accion",
        "root",
        &["unidad", "app.service", "reiniciar", "1", "ninguna"],
        "",
    );
    assert_eq!(salida.codigo, 0, "{}", salida.todo());
    assert!(
        salida.stdout.contains("MAGI_RESULTADO=sin_confirmar"),
        "{}",
        salida.stdout
    );
}

#[test]
fn accion_sobre_un_contenedor() {
    let falsos = Falsos::nuevo();
    falsos.escribir("docker_accion_estado", "running");
    let salida = falsos.ejecutar(
        "accion",
        "root",
        &["contenedor", "web-app-1", "reiniciar", "30", "ninguna"],
        "",
    );
    assert_eq!(salida.codigo, 0, "{}", salida.todo());
    assert!(
        salida.stdout.contains("MAGI_RESULTADO=hecho running"),
        "{}",
        salida.stdout
    );
    assert!(falsos.llamadas().contains("docker restart web-app-1"));
}

#[test]
fn accion_sobre_un_proyecto_compose() {
    let falsos = Falsos::nuevo();
    falsos.escribir("compose_accion_estado", "running");
    let salida = falsos.ejecutar(
        "accion",
        "root",
        &["proyecto", "web", "reiniciar", "30", "conocida"],
        "",
    );
    assert_eq!(salida.codigo, 0, "{}", salida.todo());
    assert!(
        salida.stdout.contains("MAGI_COMPROBACION=ok"),
        "{}",
        salida.stdout
    );
    assert!(
        salida.stdout.contains("MAGI_RESULTADO=hecho"),
        "{}",
        salida.stdout
    );
    let llamadas = falsos.llamadas();
    assert!(
        llamadas.contains("compose -p web -f /srv/web/docker-compose.yml config -q"),
        "{llamadas}"
    );
    assert!(
        llamadas.contains("compose -p web -f /srv/web/docker-compose.yml restart"),
        "{llamadas}"
    );
}

#[test]
fn la_comprobacion_de_un_proyecto_tambien_para_el_reinicio() {
    let falsos = Falsos::nuevo();
    falsos.escribir("compose_config_codigo", "1");
    falsos.escribir(
        "compose_config_salida",
        "compose.yaml: línea 12: variable sin definir\n",
    );
    let salida = falsos.ejecutar(
        "accion",
        "root",
        &["proyecto", "web", "reiniciar", "30", "conocida"],
        "",
    );
    assert_eq!(salida.codigo, 0, "{}", salida.todo());
    assert!(salida.stdout.contains("MAGI_COMPROBACION=fallo 1"));
    assert!(salida
        .stdout
        .contains("MAGI_RESULTADO=rechazada_configuracion"));
    assert!(!falsos.llamadas().contains(" restart"));
}

// ------------------------------------------------------------------ logs
fn leer_linea(lector: &mut BufReader<std::process::ChildStdout>) -> String {
    let mut linea = String::new();
    lector.read_line(&mut linea).expect("leer línea");
    linea
}

#[test]
fn los_logs_de_una_unidad_se_cortan_al_cerrar_la_entrada() {
    let falsos = Falsos::nuevo();
    falsos.escribir(
        "journal/app.service",
        "{\"__REALTIME_TIMESTAMP\":\"1794139824180943\",\"PRIORITY\":\"6\",\"MESSAGE\":\"hola\"}\n\
         {\"__REALTIME_TIMESTAMP\":\"1794139824180944\",\"PRIORITY\":\"3\",\"MESSAGE\":\"adios\"}\n",
    );
    let (mut hijo, mut lector) =
        falsos.lanzar("logs", "sin_sudo", &["unidad", "app.service", "2"], "");
    let primera = leer_linea(&mut lector);
    let segunda = leer_linea(&mut lector);
    assert!(primera.contains("MESSAGE"), "{primera}");
    assert!(segunda.contains("MESSAGE"), "{segunda}");
    drop(hijo.stdin.take());
    let codigo = esperar_salida(hijo, Duration::from_secs(5));
    assert_eq!(codigo, 0);
}

#[test]
fn los_logs_de_un_contenedor_llevan_su_flujo() {
    let falsos = Falsos::nuevo();
    falsos.escribir("docker_logs_out", "2026-11-08T12:00:00Z hola\n");
    falsos.escribir("docker_logs_err", "2026-11-08T12:00:01Z malo\n");
    let (mut hijo, mut lector) =
        falsos.lanzar("logs", "root", &["contenedor", "web-app-1", "1"], "");
    let mut lineas = vec![leer_linea(&mut lector), leer_linea(&mut lector)];
    lineas.sort();
    assert!(
        lineas
            .iter()
            .any(|linea| linea.starts_with("MAGI_LOG out ")),
        "{lineas:?}"
    );
    assert!(
        lineas
            .iter()
            .any(|linea| linea.starts_with("MAGI_LOG err ")),
        "{lineas:?}"
    );
    drop(hijo.stdin.take());
    let codigo = esperar_salida(hijo, Duration::from_secs(5));
    assert_eq!(codigo, 0);
}

#[test]
fn los_logs_sin_permiso_salen_con_69() {
    let falsos = Falsos::nuevo();
    falsos.escribir("journal_sin_permiso", "");
    let salida = falsos.ejecutar("logs", "sin_sudo", &["unidad", "app.service", "10"], "");
    assert_eq!(salida.codigo, 69, "{}", salida.todo());
    assert!(salida.stderr.contains("ERROR sin_permiso"));
}

// ------------------------------------------------- H2: propia de un proyecto
#[test]
fn la_comprobacion_propia_de_un_proyecto_usa_los_privilegios_del_motor() {
    let falsos = Falsos::nuevo();
    // Solo Podman utilizable: Docker da permiso denegado y el sudo pide
    // contraseña, que no se manda (como hace el cliente con Podman).
    falsos.escribir("docker_sin_permiso", "");
    falsos.escribir("sudo_modo", "con_clave");
    falsos.escribir("podman_etiquetas_proyecto", "/srv/web/docker-compose.yml\n");
    falsos.escribir("compose_accion_estado", "running");
    let salida = falsos.ejecutar(
        "accion",
        "sudo_con_clave",
        &["proyecto", "web", "reiniciar", "5", "propia", "true"],
        "",
    );
    assert_eq!(salida.codigo, 0, "{}", salida.todo());
    assert!(
        salida.stdout.contains("MAGI_COMPROBACION=ok"),
        "{}",
        salida.stdout
    );
    assert!(
        salida.stdout.contains("MAGI_RESULTADO=hecho"),
        "{}",
        salida.stdout
    );
    let llamadas = falsos.llamadas();
    assert!(
        !llamadas.contains("sudo -n -- sh -c") && !llamadas.contains("sudo -S -p '' -- sh -c"),
        "la comprobación de un proyecto no va con sudo si el motor va directo: {llamadas}"
    );
}

#[test]
fn una_comprobacion_propia_de_proyecto_que_falla_rechaza_la_accion() {
    let falsos = Falsos::nuevo();
    falsos.escribir("docker_sin_permiso", "");
    falsos.escribir("sudo_modo", "con_clave");
    falsos.escribir("podman_etiquetas_proyecto", "/srv/web/docker-compose.yml\n");
    let salida = falsos.ejecutar(
        "accion",
        "sudo_con_clave",
        &["proyecto", "web", "reiniciar", "5", "propia", "false"],
        "",
    );
    assert_eq!(salida.codigo, 0, "{}", salida.todo());
    assert!(
        salida.stdout.contains("MAGI_COMPROBACION=fallo"),
        "{}",
        salida.stdout
    );
    assert!(
        salida
            .stdout
            .contains("MAGI_RESULTADO=rechazada_configuracion"),
        "{}",
        salida.stdout
    );
    assert!(
        !falsos
            .llamadas()
            .lines()
            .any(|linea| linea.contains("compose") && linea.contains("restart")),
        "no se debía reiniciar el proyecto: {}",
        falsos.llamadas()
    );
}

// ------------------------------------------------- M5: matar el proceso real
fn una_linea_de_diario() -> &'static str {
    "{\"__REALTIME_TIMESTAMP\":\"1794139824180943\",\"PRIORITY\":\"6\",\"MESSAGE\":\"hola\"}\n"
}

#[test]
fn al_cerrar_los_logs_se_termina_el_journalctl_de_verdad() {
    let falsos = Falsos::nuevo();
    falsos.escribir("journal/app.service", una_linea_de_diario());
    let (mut hijo, mut lector) = falsos.lanzar("logs", "root", &["unidad", "app.service", "1"], "");
    let primera = leer_linea(&mut lector);
    assert!(primera.contains("MESSAGE"), "{primera}");
    falsos.esperar_estado("listo_journalctl");
    drop(hijo.stdin.take());
    let codigo = esperar_salida(hijo, Duration::from_secs(8));
    assert_eq!(codigo, 0);
    assert!(
        falsos.existe_estado("terminado_journalctl"),
        "el TERM no llegó al journalctl real"
    );
}

#[test]
fn un_journalctl_que_ignora_term_se_remata_con_kill() {
    let falsos = Falsos::nuevo();
    falsos.escribir("journal/app.service", una_linea_de_diario());
    falsos.escribir("journal_ignora_term", "");
    let (mut hijo, mut lector) = falsos.lanzar("logs", "root", &["unidad", "app.service", "1"], "");
    let _ = leer_linea(&mut lector);
    falsos.esperar_estado("listo_journalctl");
    drop(hijo.stdin.take());
    let codigo = esperar_salida(hijo, Duration::from_secs(8));
    assert_eq!(codigo, 0);
    let pid =
        fs::read_to_string(falsos.ruta_estado().join("pid_journalctl")).expect("pid del falso");
    let pid = pid.trim();
    let vivo = Command::new("sh")
        .arg("-c")
        .arg(format!("kill -0 {pid} 2>/dev/null"))
        .status()
        .expect("kill -0")
        .success();
    assert!(!vivo, "el journalctl que ignora TERM debía morir con KILL");
}

#[test]
fn al_cerrar_los_logs_se_termina_el_docker_logs_de_verdad() {
    let falsos = Falsos::nuevo();
    falsos.escribir("docker_logs_out", "2026-11-08T12:00:00Z hola\n");
    let (mut hijo, mut lector) =
        falsos.lanzar("logs", "root", &["contenedor", "web-app-1", "1"], "");
    let primera = leer_linea(&mut lector);
    assert!(primera.contains("MAGI_LOG out"), "{primera}");
    falsos.esperar_estado("listo_docker_logs");
    drop(hijo.stdin.take());
    let codigo = esperar_salida(hijo, Duration::from_secs(8));
    assert_eq!(codigo, 0);
    assert!(
        falsos.existe_estado("terminado_docker_logs"),
        "el TERM no llegó al docker logs real"
    );
}

// ------------------------------------------------- B8: líneas completas
#[test]
fn las_lineas_de_una_accion_no_parten_caracteres_ni_pierden_la_ultima() {
    let falsos = Falsos::nuevo();
    falsos.escribir("accion_codigo", "1");
    // La última línea no acaba en salto y hay caracteres multibyte.
    falsos.escribir(
        "journal_short/app.service",
        "aviso: configuración\ndos sin salto",
    );
    let salida = falsos.ejecutar(
        "accion",
        "root",
        &["unidad", "app.service", "reiniciar", "30", "ninguna"],
        "",
    );
    assert_eq!(salida.codigo, 0, "{}", salida.todo());
    assert!(
        salida.stdout.contains("MAGI_LINEA aviso: configuración"),
        "{}",
        salida.stdout
    );
    assert!(
        salida.stdout.contains("MAGI_LINEA dos sin salto"),
        "la última línea sin salto no puede perderse: {}",
        salida.stdout
    );
    assert!(
        !salida.stdout.contains('\u{FFFD}'),
        "no puede aparecer un carácter UTF-8 partido: {}",
        salida.stdout
    );
}
