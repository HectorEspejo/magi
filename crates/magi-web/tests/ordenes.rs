//! Pruebas de `ordenes::ejecutar_con_plazo` contra órdenes estándar (sin red).

use std::time::{Duration, Instant};

use magi_web::error::ErrorWeb;
use magi_web::ordenes::{ejecutar_con_plazo, PLAZO_RED, TOPE_SALIDA};

const HOLGADO: Duration = Duration::from_secs(20);

fn motivo(error: ErrorWeb) -> (String, String) {
    match error {
        ErrorWeb::Orden { orden, motivo } => (orden, motivo),
        otro => panic!("se esperaba ErrorWeb::Orden y salió: {otro}"),
    }
}

/// ¿Sigue vivo el proceso `pid`? (`kill -0` no manda nada: solo comprueba).
fn sigue_vivo(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[test]
fn una_orden_que_termina_devuelve_su_salida_y_su_codigo() {
    let r = ejecutar_con_plazo("echo", &["hola"], &[], HOLGADO, TOPE_SALIDA).unwrap();
    assert!(r.exito());
    assert_eq!(r.codigo, Some(0));
    assert_eq!(r.salida, b"hola\n");
    assert!(r.error.is_empty());
}

#[test]
fn un_codigo_distinto_de_cero_no_es_un_error_de_la_orden() {
    let r = ejecutar_con_plazo(
        "sh",
        &["-c", "echo fuera; echo dentro >&2; exit 3"],
        &[],
        HOLGADO,
        TOPE_SALIDA,
    )
    .unwrap();
    assert!(!r.exito());
    assert_eq!(r.codigo, Some(3));
    assert_eq!(r.salida, b"fuera\n");
    assert_eq!(r.error, b"dentro\n");
}

#[test]
fn no_hay_shell_los_argumentos_llegan_tal_cual() {
    // Con shell, `;`, `$()` y las comillas harían otra cosa.
    let raros = "; echo inyectado $(echo no) `echo no` 'a' \"b\" * ? > /dev/null";
    let r = ejecutar_con_plazo("echo", &[raros], &[], HOLGADO, TOPE_SALIDA).unwrap();
    assert_eq!(String::from_utf8_lossy(&r.salida), format!("{raros}\n"));

    // El programa tampoco pasa por un shell: un nombre con espacios no se trocea.
    let e = ejecutar_con_plazo("echo hola", &[], &[], HOLGADO, TOPE_SALIDA).unwrap_err();
    let (orden, _) = motivo(e);
    assert_eq!(orden, "echo hola");
}

#[test]
fn el_entorno_se_anade_al_heredado() {
    let r = ejecutar_con_plazo(
        "sh",
        &["-c", "printf '%s|%s' \"$MAGI_WEB_PRUEBA\" \"$PATH\""],
        &[("MAGI_WEB_PRUEBA", "valor de prueba")],
        HOLGADO,
        TOPE_SALIDA,
    )
    .unwrap();
    let salida = String::from_utf8_lossy(&r.salida).to_string();
    let (propio, path) = salida.split_once('|').unwrap();
    assert_eq!(propio, "valor de prueba");
    assert!(!path.is_empty(), "el PATH heredado debe seguir ahí");
}

#[test]
fn el_plazo_mata_la_orden_y_el_tope_de_salida_la_corta() {
    let carpeta = tempfile::tempdir().unwrap();
    let fichero_pid = carpeta.path().join("pid");

    // 1. Plazo: `exec` deja `sleep` con el mismo pid que el `sh` que lo lanzó.
    let inicio = Instant::now();
    let e = ejecutar_con_plazo(
        "sh",
        &[
            "-c",
            "echo $$ > \"$1\"; exec sleep 60",
            "sh",
            fichero_pid.to_str().unwrap(),
        ],
        &[],
        // Holgado para que `sh` llegue a escribir su pid aunque la máquina vaya
        // cargada; lo que cuenta es que no pase de los 10 s de abajo.
        Duration::from_millis(2000),
        TOPE_SALIDA,
    )
    .unwrap_err();
    assert!(
        inicio.elapsed() < Duration::from_secs(10),
        "debía cortarse en el plazo, tardó {:?}",
        inicio.elapsed()
    );
    let (orden, texto) = motivo(e);
    assert_eq!(orden, "sh");
    assert!(texto.contains("plazo"), "el motivo dice el plazo: {texto}");
    let pid = std::fs::read_to_string(&fichero_pid).unwrap();
    let pid = pid.trim();
    assert!(!pid.is_empty());
    assert!(!sigue_vivo(pid), "el proceso {pid} debía estar muerto");

    // 2. Tope de la salida estándar: `yes` escribe sin parar.
    let inicio = Instant::now();
    let e = ejecutar_con_plazo("yes", &[], &[], HOLGADO, 1000).unwrap_err();
    assert!(
        inicio.elapsed() < Duration::from_secs(10),
        "debía cortarse en el tope, tardó {:?}",
        inicio.elapsed()
    );
    let (orden, texto) = motivo(e);
    assert_eq!(orden, "yes");
    assert!(texto.contains("tope"), "el motivo dice el tope: {texto}");
    assert!(texto.contains("1000"));

    // 3. Tope de la salida de error.
    let e = ejecutar_con_plazo("sh", &["-c", "yes >&2"], &[], HOLGADO, 1000).unwrap_err();
    let (_, texto) = motivo(e);
    assert!(texto.contains("tope"), "el motivo dice el tope: {texto}");
}

#[test]
fn una_salida_que_cabe_justo_en_el_tope_no_falla() {
    let r = ejecutar_con_plazo("head", &["-c", "5000", "/dev/zero"], &[], HOLGADO, 5000).unwrap();
    assert_eq!(r.salida.len(), 5000);
    let e =
        ejecutar_con_plazo("head", &["-c", "5001", "/dev/zero"], &[], HOLGADO, 5000).unwrap_err();
    assert!(motivo(e).1.contains("tope"));
}

#[test]
fn una_salida_grande_dentro_del_tope_no_bloquea_las_tuberias() {
    // Más que el búfer de una tubería (64 KiB) en las dos salidas a la vez.
    let r = ejecutar_con_plazo(
        "sh",
        &[
            "-c",
            "head -c 300000 /dev/zero; head -c 200000 /dev/zero >&2",
        ],
        &[],
        HOLGADO,
        TOPE_SALIDA,
    )
    .unwrap();
    assert_eq!(r.salida.len(), 300_000);
    assert_eq!(r.error.len(), 200_000);
}

#[test]
fn el_tope_cero_corta_cualquier_salida() {
    let e = ejecutar_con_plazo("echo", &["x"], &[], HOLGADO, 0).unwrap_err();
    assert!(motivo(e).1.contains("tope"));
    // Sin salida no hay nada que cortar.
    let r = ejecutar_con_plazo("true", &[], &[], HOLGADO, 0).unwrap();
    assert!(r.exito());
}

#[test]
fn un_programa_que_no_existe_falla_sin_datos_del_entorno_ni_argumentos() {
    let e = ejecutar_con_plazo(
        "magi-web-programa-que-no-existe",
        &["argumento-secreto-1234"],
        &[("GH_TOKEN", "ghp_token_secreto_5678")],
        HOLGADO,
        TOPE_SALIDA,
    )
    .unwrap_err();
    let texto = e.to_string();
    assert!(texto.contains("magi-web-programa-que-no-existe"));
    assert!(!texto.contains("argumento-secreto-1234"), "{texto}");
    assert!(!texto.contains("ghp_token_secreto_5678"), "{texto}");
    assert!(!texto.contains("GH_TOKEN"), "{texto}");
}

#[test]
fn los_mensajes_de_plazo_y_tope_no_llevan_argumentos_ni_entorno() {
    let args = ["-c", "sleep 60 # argumento-secreto-1234"];
    let e = ejecutar_con_plazo(
        "sh",
        &args,
        &[("GH_TOKEN", "ghp_token_secreto_5678")],
        Duration::from_millis(200),
        TOPE_SALIDA,
    )
    .unwrap_err();
    let texto = e.to_string();
    assert!(texto.contains("plazo"));
    assert!(!texto.contains("argumento-secreto-1234"), "{texto}");
    assert!(!texto.contains("ghp_token_secreto_5678"), "{texto}");
}

#[test]
fn un_descendiente_con_las_tuberias_abiertas_no_alarga_el_plazo_mas_de_una_vez() {
    // `sh` termina al instante, pero su `sleep` (en segundo plano) conserva la salida
    // estándar y la de error abiertas: las dos tuberías comparten el límite y la
    // espera total no pasa de un plazo, en vez de uno por tubería.
    let plazo = Duration::from_millis(1500);
    let inicio = Instant::now();
    let e =
        ejecutar_con_plazo("sh", &["-c", "sleep 4 & exit 0"], &[], plazo, TOPE_SALIDA).unwrap_err();
    let tardo = inicio.elapsed();
    let (orden, texto) = motivo(e);
    assert_eq!(orden, "sh");
    assert!(texto.contains("tubería"), "{texto}");
    assert!(
        tardo < plazo * 2 - Duration::from_millis(200),
        "esperó más de un plazo por las tuberías: {tardo:?}"
    );
}

#[test]
fn una_orden_matada_por_una_senal_no_tiene_codigo() {
    let r = ejecutar_con_plazo("sh", &["-c", "kill -9 $$"], &[], HOLGADO, TOPE_SALIDA).unwrap();
    assert_eq!(r.codigo, None);
    assert!(!r.exito());
}

#[test]
fn la_orden_no_hereda_la_entrada_estandar() {
    // `cat` sin argumentos lee de la entrada: con stdin cerrado termina al instante.
    let inicio = Instant::now();
    let r = ejecutar_con_plazo("cat", &[], &[], HOLGADO, TOPE_SALIDA).unwrap();
    assert!(r.exito());
    assert!(r.salida.is_empty());
    assert!(inicio.elapsed() < Duration::from_secs(10));
}

#[test]
fn las_constantes_por_omision_son_las_del_contrato() {
    assert_eq!(PLAZO_RED, Duration::from_secs(60));
    assert_eq!(TOPE_SALIDA, 10 * 1024 * 1024);
}
