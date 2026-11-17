//! Pruebas del llavero de fichero con frase maestra (Fase 17): crear y abrir,
//! frase incorrecta, ficheros dañados, factor de trabajo acotado, NFC,
//! escrituras concurrentes con `flock`, serie de fallos con reloj inyectado y
//! el resto de operaciones. Sin red ni llavero del sistema: cada prueba trabaja
//! en su carpeta temporal.
//!
//! El factor de trabajo de las pruebas es bajo (2^6) para que scrypt no pese;
//! una sola prueba usa el de producción (2^17).

use std::io::Write as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use magi_nucleo::llavero::{
    es_bloqueado, LlaveroEnMemoria, LlaveroHost, LlaveroSecreto, BLOQUEADO,
};
use magi_nucleo::llavero_fichero::{
    info_fichero, preparar_frase, ErrorLlavero, EstadoFichero, LlaveroFichero, Reloj, ESPERA_SERIE,
    FALLOS_POR_SERIE, LOG_N_CREAR, LOG_N_MAX, MAX_FRASE_BYTES, MIN_FRASE, NOMBRE_FICHERO,
    SIN_CREAR, TOPE_BYTES,
};

const FRASE: &str = "una frase maestra larga";
const OTRA_FRASE: &str = "otra frase muy distinta";
/// Factor de trabajo de las pruebas.
const LOG_N_PRUEBAS: u8 = 6;
/// Un secreto reconocible para comprobar que no se cuela en ningún texto.
const SECRETO: &str = "S3CR3T0-UNICO-4f9c";

fn ruta_en(dir: &Path) -> PathBuf {
    dir.join(NOMBRE_FICHERO)
}

fn llavero_en(dir: &Path) -> LlaveroFichero {
    LlaveroFichero::con_ajustes(ruta_en(dir), LOG_N_PRUEBAS, Arc::new(Instant::now))
}

/// Un llavero creado y abierto con `FRASE`.
fn llavero_creado(dir: &Path) -> LlaveroFichero {
    let llavero = llavero_en(dir);
    llavero.crear(FRASE, FRASE).unwrap();
    llavero
}

/// Reloj que avanza solo cuando la prueba lo manda (milisegundos).
fn reloj_falso() -> (Reloj, Arc<AtomicU64>) {
    let base = Instant::now();
    let milis = Arc::new(AtomicU64::new(0));
    let copia = milis.clone();
    let reloj: Reloj = Arc::new(move || base + Duration::from_millis(copia.load(Ordering::SeqCst)));
    (reloj, milis)
}

/// Un fichero age con una entrada scrypt y el texto en claro que se quiera:
/// para fabricar contenidos que MAGI nunca escribiría (JSON roto, otro formato).
fn cifrar_age(claro: &[u8], frase: &str, log_n: u8) -> Vec<u8> {
    let mut destinatario =
        age::scrypt::Recipient::new(age::secrecy::SecretString::from(frase.to_owned()));
    destinatario.set_work_factor(log_n);
    let cifrador =
        age::Encryptor::with_recipients(std::iter::once(&destinatario as &dyn age::Recipient))
            .unwrap();
    let mut salida = Vec::new();
    let mut escritor = cifrador.wrap_output(&mut salida).unwrap();
    escritor.write_all(claro).unwrap();
    escritor.finish().unwrap();
    salida
}

fn descifrar_age(datos: &[u8], frase: &str) -> Vec<u8> {
    use std::io::Read as _;
    let mut identidad =
        age::scrypt::Identity::new(age::secrecy::SecretString::from(frase.to_owned()));
    identidad.set_max_work_factor(LOG_N_MAX);
    let descifrador = age::Decryptor::new_buffered(datos).unwrap();
    let mut lector = descifrador
        .decrypt(std::iter::once(&identidad as &dyn age::Identity))
        .unwrap();
    let mut claro = Vec::new();
    lector.read_to_end(&mut claro).unwrap();
    claro
}

/// Cambia el factor de trabajo escrito en la entrada scrypt de la cabecera.
fn forjar_factor(datos: &[u8], nuevo: u32) -> Vec<u8> {
    let fin_linea1 = datos.iter().position(|b| *b == b'\n').unwrap() + 1;
    let fin_linea2 = fin_linea1
        + datos[fin_linea1..]
            .iter()
            .position(|b| *b == b'\n')
            .unwrap();
    let linea = std::str::from_utf8(&datos[fin_linea1..fin_linea2]).unwrap();
    assert!(linea.starts_with("-> scrypt "), "línea: {linea}");
    let (principio, _) = linea.rsplit_once(' ').unwrap();
    let mut salida = datos[..fin_linea1].to_vec();
    salida.extend_from_slice(format!("{principio} {nuevo}").as_bytes());
    salida.extend_from_slice(&datos[fin_linea2..]);
    salida
}

/// El factor de trabajo escrito en la cabecera.
fn factor_de(datos: &[u8]) -> u32 {
    let texto = String::from_utf8_lossy(&datos[..120.min(datos.len())]).into_owned();
    let linea = texto.lines().nth(1).unwrap();
    linea.rsplit_once(' ').unwrap().1.parse().unwrap()
}

/// Cambia un byte en la posición dada.
fn tocar(datos: &[u8], posicion: usize) -> Vec<u8> {
    let mut salida = datos.to_vec();
    salida[posicion] = if salida[posicion] == b'A' { b'B' } else { b'A' };
    salida
}

/// Posición del primer carácter del MAC de la cabecera (tras `--- `).
fn posicion_mac(datos: &[u8]) -> usize {
    let texto = String::from_utf8_lossy(datos).into_owned();
    texto.find("\n--- ").unwrap() + 5
}

/// Posición del primer carácter de la sal (tras `-> scrypt `).
fn posicion_sal(datos: &[u8]) -> usize {
    let texto = String::from_utf8_lossy(datos).into_owned();
    texto.find("-> scrypt ").unwrap() + "-> scrypt ".len()
}

fn es_danado(estado: &EstadoFichero) -> bool {
    matches!(estado, EstadoFichero::Danado(_))
}

fn motivo_danado(error: ErrorLlavero) -> String {
    match error {
        ErrorLlavero::Danado(motivo) => motivo,
        otro => panic!("se esperaba «dañado» y salió {otro:?}"),
    }
}

// ---------------------------------------------------------------------------
// Crear y abrir
// ---------------------------------------------------------------------------

#[test]
fn crear_deja_el_llavero_abierto_y_el_fichero_es_un_age_con_json() {
    let dir = tempfile::tempdir().unwrap();
    let llavero = llavero_en(dir.path());
    assert_eq!(llavero.estado(), EstadoFichero::Inexistente);
    llavero.crear(FRASE, FRASE).unwrap();
    assert_eq!(llavero.estado(), EstadoFichero::Abierto);

    let datos = std::fs::read(ruta_en(dir.path())).unwrap();
    assert!(datos.starts_with(b"age-encryption.org/v1\n-> scrypt "));
    assert_eq!(factor_de(&datos), u32::from(LOG_N_PRUEBAS));
    let claro = descifrar_age(&datos, FRASE);
    let json: serde_json::Value = serde_json::from_slice(&claro).unwrap();
    assert_eq!(json["formato"], 1);
    assert_eq!(json["entradas"], serde_json::json!({}));
}

#[test]
fn crear_con_un_fichero_existente_da_ya_existe_y_no_lo_toca() {
    let dir = tempfile::tempdir().unwrap();
    let llavero = llavero_creado(dir.path());
    llavero.guardar("cuenta", SECRETO).unwrap();
    let antes = std::fs::read(ruta_en(dir.path())).unwrap();
    let otro = llavero_en(dir.path());
    assert_eq!(
        otro.crear(OTRA_FRASE, OTRA_FRASE),
        Err(ErrorLlavero::YaExiste)
    );
    assert_eq!(std::fs::read(ruta_en(dir.path())).unwrap(), antes);
    assert_eq!(otro.estado(), EstadoFichero::Cerrado);
}

#[test]
fn crear_valida_la_frase_antes_de_tocar_nada() {
    let dir = tempfile::tempdir().unwrap();
    let llavero = llavero_en(dir.path());
    assert!(matches!(
        llavero.crear("corta", "corta"),
        Err(ErrorLlavero::FraseInvalida(_))
    ));
    assert!(matches!(
        llavero.crear(FRASE, OTRA_FRASE),
        Err(ErrorLlavero::FraseInvalida(_))
    ));
    assert_eq!(llavero.estado(), EstadoFichero::Inexistente);
    assert!(!ruta_en(dir.path()).exists());
}

#[test]
fn abrir_en_otro_objeto_pide_la_frase_y_recupera_lo_guardado() {
    let dir = tempfile::tempdir().unwrap();
    {
        let llavero = llavero_creado(dir.path());
        llavero.guardar("usuario@192.0.2.10", SECRETO).unwrap();
        llavero
            .guardar("sincronizacion", "AGE-SECRET-KEY-1EJEMPLO")
            .unwrap();
    }
    let llavero = llavero_en(dir.path());
    assert_eq!(llavero.estado(), EstadoFichero::Cerrado);
    let error = llavero.recuperar("usuario@192.0.2.10").unwrap_err();
    assert!(es_bloqueado(&error), "error: {error}");
    assert_eq!(error, BLOQUEADO);

    llavero.desbloquear(FRASE).unwrap();
    assert_eq!(llavero.estado(), EstadoFichero::Abierto);
    assert_eq!(
        llavero
            .recuperar("usuario@192.0.2.10")
            .unwrap()
            .as_deref()
            .map(String::as_str),
        Some(SECRETO)
    );
    assert_eq!(
        llavero
            .recuperar("sincronizacion")
            .unwrap()
            .as_deref()
            .map(String::as_str),
        Some("AGE-SECRET-KEY-1EJEMPLO")
    );
    assert!(llavero.recuperar("no-existe").unwrap().is_none());
}

#[test]
fn los_valores_dificiles_viajan_intactos_por_el_json() {
    let dir = tempfile::tempdir().unwrap();
    let llavero = llavero_creado(dir.path());
    let valores = [
        ("comillas", "di \"hola\" y \\adiós\\"),
        ("control", "linea1\nlinea2\t\r\u{1}\u{1f}"),
        ("unicode", "contraseña ñandú 日本語 🔑 e\u{301}"),
        ("vacio", ""),
        ("json", "{\"formato\":2,\"entradas\":{}}"),
    ];
    for (cuenta, valor) in valores {
        llavero.guardar(cuenta, valor).unwrap();
    }
    let otro = llavero_en(dir.path());
    otro.desbloquear(FRASE).unwrap();
    for (cuenta, valor) in valores {
        assert_eq!(
            otro.recuperar(cuenta)
                .unwrap()
                .as_deref()
                .map(String::as_str),
            Some(valor),
            "cuenta {cuenta}"
        );
    }
}

#[test]
fn olvidar_dice_si_existia_y_no_reescribe_si_no() {
    let dir = tempfile::tempdir().unwrap();
    let llavero = llavero_creado(dir.path());
    llavero.guardar("a", "1").unwrap();
    llavero.guardar("b", "2").unwrap();
    assert_eq!(llavero.olvidar("a"), Ok(true));
    assert_eq!(llavero.olvidar("a"), Ok(false));
    assert!(llavero.recuperar("a").unwrap().is_none());
    let antes = std::fs::read(ruta_en(dir.path())).unwrap();
    assert_eq!(llavero.olvidar("nada"), Ok(false));
    assert_eq!(
        std::fs::read(ruta_en(dir.path())).unwrap(),
        antes,
        "olvidar lo que no está no reescribe el fichero"
    );
    let otro = llavero_en(dir.path());
    otro.desbloquear(FRASE).unwrap();
    assert!(otro.recuperar("a").unwrap().is_none());
    assert!(otro.recuperar("b").unwrap().is_some());
}

#[test]
fn con_el_factor_de_produccion_se_crea_y_se_abre() {
    // El único test con 2^17 (~128 MiB): comprueba que el factor real se
    // escribe y se acepta. El tiempo sale con --nocapture.
    let dir = tempfile::tempdir().unwrap();
    let llavero = LlaveroFichero::nuevo(ruta_en(dir.path()));
    let inicio = Instant::now();
    llavero.crear(FRASE, FRASE).unwrap();
    let crear = inicio.elapsed();
    let datos = std::fs::read(ruta_en(dir.path())).unwrap();
    assert_eq!(factor_de(&datos), u32::from(LOG_N_CREAR));
    assert_eq!(LOG_N_CREAR, 17);

    let otro = LlaveroFichero::nuevo(ruta_en(dir.path()));
    let inicio = Instant::now();
    otro.desbloquear(FRASE).unwrap();
    let abrir = inicio.elapsed();
    println!("factor 2^{LOG_N_CREAR}: crear {crear:?}, abrir {abrir:?}");
    assert_eq!(otro.estado(), EstadoFichero::Abierto);
}

// ---------------------------------------------------------------------------
// Frase incorrecta
// ---------------------------------------------------------------------------

#[test]
fn con_otra_frase_da_frase_incorrecta_y_el_fichero_queda_byte_a_byte_igual() {
    let dir = tempfile::tempdir().unwrap();
    {
        let llavero = llavero_creado(dir.path());
        llavero.guardar("cuenta", SECRETO).unwrap();
    }
    let antes = std::fs::read(ruta_en(dir.path())).unwrap();
    let meta_antes = std::fs::metadata(ruta_en(dir.path())).unwrap();

    let llavero = llavero_en(dir.path());
    assert_eq!(
        llavero.desbloquear(OTRA_FRASE),
        Err(ErrorLlavero::FraseIncorrecta {
            intentos_seguidos: 1
        })
    );
    assert_eq!(llavero.estado(), EstadoFichero::Cerrado);
    assert_eq!(std::fs::read(ruta_en(dir.path())).unwrap(), antes);
    let meta_despues = std::fs::metadata(ruta_en(dir.path())).unwrap();
    assert_eq!(
        meta_antes.modified().unwrap(),
        meta_despues.modified().unwrap(),
        "ni se reescribió"
    );
    // Un fallo tampoco deja abierto el llavero.
    assert!(es_bloqueado(&llavero.recuperar("cuenta").unwrap_err()));
    // La buena sigue abriendo.
    llavero.desbloquear(FRASE).unwrap();
}

#[test]
fn la_frase_vacia_o_enorme_no_gasta_scrypt_ni_cuenta() {
    let dir = tempfile::tempdir().unwrap();
    llavero_creado(dir.path());
    let llavero = llavero_en(dir.path());
    assert!(matches!(
        llavero.desbloquear(""),
        Err(ErrorLlavero::FraseInvalida(_))
    ));
    assert!(matches!(
        llavero.desbloquear(&"x".repeat(MAX_FRASE_BYTES + 1)),
        Err(ErrorLlavero::FraseInvalida(_))
    ));
    assert_eq!(llavero.intentos_seguidos(), 0);
}

#[test]
fn desbloquear_sin_fichero_da_inexistente() {
    let dir = tempfile::tempdir().unwrap();
    let llavero = llavero_en(dir.path());
    assert_eq!(llavero.desbloquear(FRASE), Err(ErrorLlavero::Inexistente));
}

// ---------------------------------------------------------------------------
// Dañado
// ---------------------------------------------------------------------------

/// Escribe `datos` como `llavero.age` y devuelve un llavero sobre él.
fn con_fichero(dir: &Path, datos: &[u8]) -> LlaveroFichero {
    std::fs::write(ruta_en(dir), datos).unwrap();
    llavero_en(dir)
}

#[test]
fn la_basura_y_lo_truncado_son_danado_y_no_frase_incorrecta() {
    let dir = tempfile::tempdir().unwrap();
    let valido = {
        let llavero = llavero_creado(dir.path());
        llavero.guardar("cuenta", SECRETO).unwrap();
        std::fs::read(ruta_en(dir.path())).unwrap()
    };
    let casos: Vec<(&str, Vec<u8>)> = vec![
        ("vacío", Vec::new()),
        ("basura", b"esto no es un llavero de nadie".to_vec()),
        ("binario", (0u8..=255).collect()),
        ("cabecera cortada", valido[..30].to_vec()),
        (
            "cabecera sin terminar",
            valido[..90.min(valido.len())].to_vec(),
        ),
        ("payload sin etiqueta", valido[..valido.len() - 1].to_vec()),
        ("payload a medias", valido[..valido.len() - 20].to_vec()),
    ];
    for (nombre, datos) in casos {
        let llavero = con_fichero(dir.path(), &datos);
        let estado = llavero.estado();
        // Con la cabecera entera, el daño del payload solo se ve al abrir.
        if nombre.starts_with("payload") {
            assert_eq!(estado, EstadoFichero::Cerrado, "{nombre}");
        } else {
            assert!(es_danado(&estado), "{nombre}: {estado:?}");
        }
        let error = llavero.desbloquear(FRASE).unwrap_err();
        assert!(
            matches!(error, ErrorLlavero::Danado(_)),
            "{nombre}: {error:?}"
        );
        assert_eq!(
            llavero.intentos_seguidos(),
            0,
            "{nombre}: no cuenta como intento"
        );
        assert_eq!(
            std::fs::read(ruta_en(dir.path())).unwrap(),
            datos,
            "{nombre}: el fichero no cambia"
        );
    }
}

#[test]
fn contenido_corrupto_con_la_frase_correcta_es_danado() {
    let dir = tempfile::tempdir().unwrap();
    let valido = {
        let llavero = llavero_creado(dir.path());
        llavero.guardar("cuenta", SECRETO).unwrap();
        std::fs::read(ruta_en(dir.path())).unwrap()
    };

    // Un byte del flujo cifrado cambiado: la frase abre la cabecera y el
    // contenido no pasa su etiqueta.
    let mut flujo = valido.clone();
    let ultimo = flujo.len() - 5;
    flujo[ultimo] ^= 0x55;
    let llavero = con_fichero(dir.path(), &flujo);
    let motivo = motivo_danado(llavero.desbloquear(FRASE).unwrap_err());
    assert!(motivo.contains("dañado"), "motivo: {motivo}");

    // El MAC de la cabecera cambiado: la entrada scrypt descifra (la frase es
    // buena) y el MAC no cuadra.
    let llavero = con_fichero(dir.path(), &tocar(&valido, posicion_mac(&valido)));
    let motivo = motivo_danado(llavero.desbloquear(FRASE).unwrap_err());
    assert!(motivo.contains("MAC"), "motivo: {motivo}");
    assert_eq!(llavero.intentos_seguidos(), 0);
}

#[test]
fn una_sal_estropeada_se_ve_como_frase_incorrecta_y_el_fichero_no_cambia() {
    // La entrada scrypt no descifra, y desde fuera eso es lo mismo que una
    // frase equivocada: no hay forma de distinguirlas, y por eso el fichero
    // no se toca ni se aparta solo.
    let dir = tempfile::tempdir().unwrap();
    let valido = {
        llavero_creado(dir.path());
        std::fs::read(ruta_en(dir.path())).unwrap()
    };
    let estropeado = tocar(&valido, posicion_sal(&valido));
    let llavero = con_fichero(dir.path(), &estropeado);
    assert_eq!(llavero.estado(), EstadoFichero::Cerrado);
    assert_eq!(
        llavero.desbloquear(FRASE),
        Err(ErrorLlavero::FraseIncorrecta {
            intentos_seguidos: 1
        })
    );
    assert_eq!(std::fs::read(ruta_en(dir.path())).unwrap(), estropeado);
}

#[test]
fn json_roto_o_formato_distinto_es_danado_con_motivo() {
    let dir = tempfile::tempdir().unwrap();
    let casos: Vec<(&str, &[u8], &str)> = vec![
        ("no es JSON", b"esto no es json", "JSON"),
        (
            "JSON truncado",
            b"{\"formato\":1,\"entradas\":{\"a\":",
            "JSON",
        ),
        ("sin formato", b"{\"entradas\":{}}", "JSON"),
        ("sin entradas", b"{\"formato\":1}", "JSON"),
        (
            "formato texto",
            b"{\"formato\":\"1\",\"entradas\":{}}",
            "JSON",
        ),
        (
            "entrada numérica",
            b"{\"formato\":1,\"entradas\":{\"a\":5}}",
            "JSON",
        ),
        ("lista", b"[]", "JSON"),
        ("formato 2", b"{\"formato\":2,\"entradas\":{}}", "formato"),
        ("formato 0", b"{\"formato\":0,\"entradas\":{}}", "formato"),
        (
            "formato 2 con otra estructura",
            b"{\"formato\":2,\"cuentas\":[1,2]}",
            "formato",
        ),
    ];
    for (nombre, claro, palabra) in casos {
        let llavero = con_fichero(dir.path(), &cifrar_age(claro, FRASE, LOG_N_PRUEBAS));
        // La cabecera está bien: sin la frase se ve cerrado.
        assert_eq!(llavero.estado(), EstadoFichero::Cerrado, "{nombre}");
        let motivo = motivo_danado(llavero.desbloquear(FRASE).unwrap_err());
        assert!(motivo.contains(palabra), "{nombre}: {motivo}");
        // El motivo no cita el contenido.
        assert!(!motivo.contains("entradas"), "{nombre}: {motivo}");
        // Una vez visto el daño, el estado lo dice (la interfaz ofrece apartarlo).
        assert_eq!(llavero.estado(), EstadoFichero::Danado(motivo), "{nombre}");
    }
}

#[test]
fn un_fichero_por_encima_del_tope_es_danado_sin_leerlo_entero() {
    let dir = tempfile::tempdir().unwrap();
    let mut grande = vec![b'x'; TOPE_BYTES + 1];
    grande[..22].copy_from_slice(b"age-encryption.org/v1\n");
    let llavero = con_fichero(dir.path(), &grande);
    let estado = llavero.estado();
    assert!(es_danado(&estado), "estado: {estado:?}");
    let motivo = motivo_danado(llavero.desbloquear(FRASE).unwrap_err());
    assert!(motivo.contains("1 MiB"), "motivo: {motivo}");

    // Justo en el tope no es «grande»: es basura, pero por otra razón.
    let exacto = vec![b'x'; TOPE_BYTES];
    let llavero = con_fichero(dir.path(), &exacto);
    let motivo = motivo_danado(llavero.desbloquear(FRASE).unwrap_err());
    assert!(!motivo.contains("1 MiB"), "motivo: {motivo}");
}

#[test]
fn un_age_que_no_es_de_frase_es_danado() {
    // Cifrado a una clave X25519 en vez de a una frase.
    let identidad = age::x25519::Identity::generate();
    let datos = age::encrypt(&identidad.to_public(), b"{\"formato\":1,\"entradas\":{}}").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let llavero = con_fichero(dir.path(), &datos);
    let estado = llavero.estado();
    assert!(es_danado(&estado), "estado: {estado:?}");
    assert!(matches!(
        llavero.desbloquear(FRASE),
        Err(ErrorLlavero::Danado(_))
    ));
}

#[test]
fn un_directorio_en_lugar_del_fichero_es_danado() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(ruta_en(dir.path())).unwrap();
    let llavero = llavero_en(dir.path());
    assert!(es_danado(&llavero.estado()));
    assert!(matches!(
        llavero.desbloquear(FRASE),
        Err(ErrorLlavero::Danado(_))
    ));
    assert!(matches!(
        info_fichero(&ruta_en(dir.path())),
        Err(ErrorLlavero::Danado(_))
    ));
}

// ---------------------------------------------------------------------------
// Factor de trabajo
// ---------------------------------------------------------------------------

#[test]
fn un_factor_forjado_a_25_es_danado_enseguida_y_sin_derivar_la_clave() {
    let dir = tempfile::tempdir().unwrap();
    let valido = {
        llavero_creado(dir.path());
        std::fs::read(ruta_en(dir.path())).unwrap()
    };
    assert_eq!(factor_de(&valido), u32::from(LOG_N_PRUEBAS));
    for forjado in [25u32, 21, 64, 255] {
        let datos = forjar_factor(&valido, forjado);
        assert_eq!(factor_de(&datos), forjado);
        let llavero = con_fichero(dir.path(), &datos);

        let inicio = Instant::now();
        let estado = llavero.estado();
        let motivo = match &estado {
            EstadoFichero::Danado(motivo) => motivo.clone(),
            otro => panic!("factor {forjado}: {otro:?}"),
        };
        assert!(motivo.contains("factor"), "motivo: {motivo}");
        let error = llavero.desbloquear(FRASE).unwrap_err();
        assert!(
            motivo_danado(error).contains("factor"),
            "factor {forjado}: el motivo habla del factor"
        );
        // Derivar 2^25 tardaría minutos y gastaría 32 GiB: aquí no pasa ni
        // por scrypt.
        assert!(
            inicio.elapsed() < Duration::from_secs(2),
            "factor {forjado}: tardó {:?}",
            inicio.elapsed()
        );
        assert_eq!(llavero.intentos_seguidos(), 0, "no cuenta como intento");
        assert_eq!(std::fs::read(ruta_en(dir.path())).unwrap(), datos);
        // Tampoco se deja cambiar la frase con él ni guardar nada.
        assert!(matches!(
            llavero.cambiar_frase(FRASE, OTRA_FRASE, OTRA_FRASE),
            Err(ErrorLlavero::Danado(_))
        ));
    }
}

#[test]
fn la_identidad_de_age_con_su_tope_rechaza_el_exceso_antes_de_derivar() {
    // Lo que MAGI da por supuesto de age 0.11 (y que esta prueba vigila si se
    // sube la versión): con `set_max_work_factor`, una entrada scrypt que pide
    // más trabajo da `ExcessiveWork` sin derivar nada. MAGI además mira la
    // cabecera antes (`validar_cabecera`), así que esto es la segunda defensa.
    let dir = tempfile::tempdir().unwrap();
    let valido = {
        llavero_creado(dir.path());
        std::fs::read(ruta_en(dir.path())).unwrap()
    };
    let forjado = forjar_factor(&valido, 25);
    let mut identidad =
        age::scrypt::Identity::new(age::secrecy::SecretString::from(FRASE.to_owned()));
    identidad.set_max_work_factor(LOG_N_MAX);
    let inicio = Instant::now();
    let descifrador = age::Decryptor::new_buffered(&forjado[..]).unwrap();
    assert!(descifrador.is_scrypt());
    let resultado = descifrador.decrypt(std::iter::once(&identidad as &dyn age::Identity));
    assert!(
        matches!(
            resultado,
            Err(age::DecryptError::ExcessiveWork { required: 25, .. })
        ),
        "age debe rechazar el factor 25"
    );
    assert!(
        inicio.elapsed() < Duration::from_secs(2),
        "sin derivar la clave"
    );
}

#[test]
fn el_factor_maximo_se_acepta_en_la_cabecera_sin_gastar_scrypt() {
    // 2^20 (1 GiB) es lo más que se abre. `estado` solo mira la cabecera: si
    // derivara la clave tardaría varios segundos y 1 GiB, incluso optimizado.
    let dir = tempfile::tempdir().unwrap();
    let valido = {
        llavero_creado(dir.path());
        std::fs::read(ruta_en(dir.path())).unwrap()
    };
    let datos = forjar_factor(&valido, u32::from(LOG_N_MAX));
    let llavero = con_fichero(dir.path(), &datos);
    let inicio = Instant::now();
    assert_eq!(llavero.estado(), EstadoFichero::Cerrado);
    assert!(inicio.elapsed() < Duration::from_secs(1));
}

#[test]
fn con_ajustes_acota_el_factor_de_trabajo() {
    let dir = tempfile::tempdir().unwrap();
    // Un factor de 0 sube a 4 (el mínimo): la cabecera lo dice. El tope de 20
    // no se prueba creando: derivar 2^20 son 1 GiB.
    let bajo = LlaveroFichero::con_ajustes(ruta_en(dir.path()), 0, Arc::new(Instant::now));
    bajo.crear(FRASE, FRASE).unwrap();
    assert_eq!(factor_de(&std::fs::read(ruta_en(dir.path())).unwrap()), 4);
    bajo.borrar().unwrap();
}

// ---------------------------------------------------------------------------
// Frase maestra: reglas y NFC
// ---------------------------------------------------------------------------

#[test]
fn preparar_frase_aplica_las_reglas_y_no_cita_la_frase() {
    // Longitud: 12 caracteres sí, 11 no; se cuentan caracteres, no bytes.
    assert!(preparar_frase(&"a".repeat(MIN_FRASE), None).is_ok());
    let corta = "b".repeat(MIN_FRASE - 1);
    let error = preparar_frase(&corta, None).unwrap_err();
    assert!(matches!(error, ErrorLlavero::FraseInvalida(_)));
    assert!(error.to_string().contains("12"));
    assert!(!error.to_string().contains(&corta));
    assert!(preparar_frase(&"é".repeat(MIN_FRASE), None).is_ok());
    // Solo espacios.
    assert!(matches!(
        preparar_frase(&" ".repeat(40), None),
        Err(ErrorLlavero::FraseInvalida(_))
    ));
    assert!(matches!(
        preparar_frase("", None),
        Err(ErrorLlavero::FraseInvalida(_))
    ));
    // Espacios dentro valen y la frase no se recorta.
    let con_espacios = "  frase con espacios  ";
    assert_eq!(
        preparar_frase(con_espacios, None).unwrap().as_str(),
        con_espacios
    );
    // Largo máximo, en bytes.
    assert!(preparar_frase(&"c".repeat(MAX_FRASE_BYTES), None).is_ok());
    assert!(matches!(
        preparar_frase(&"c".repeat(MAX_FRASE_BYTES + 1), None),
        Err(ErrorLlavero::FraseInvalida(_))
    ));
    assert!(
        preparar_frase(&"ñ".repeat(MAX_FRASE_BYTES / 2 + 1), None).is_err(),
        "los bytes, no los caracteres"
    );
    // Repetida: tiene que coincidir.
    let error = preparar_frase(FRASE, Some(OTRA_FRASE)).unwrap_err();
    assert_eq!(
        error,
        ErrorLlavero::FraseInvalida("las frases no coinciden".to_string())
    );
    assert!(preparar_frase(FRASE, Some(FRASE)).is_ok());
}

#[test]
fn la_frase_compuesta_y_la_descompuesta_abren_igual() {
    let compuesta = "caf\u{e9} con leche y mucho az\u{fa}car";
    let descompuesta = "cafe\u{301} con leche y mucho azu\u{301}car";
    assert_ne!(compuesta, descompuesta);
    assert_eq!(
        preparar_frase(compuesta, Some(descompuesta))
            .unwrap()
            .as_str(),
        compuesta
    );
    assert_eq!(
        preparar_frase(descompuesta, None).unwrap().as_str(),
        compuesta,
        "se guarda en NFC"
    );

    // Creada con la compuesta, abierta con la descompuesta, y al revés.
    let dir = tempfile::tempdir().unwrap();
    let llavero = llavero_en(dir.path());
    llavero.crear(compuesta, descompuesta).unwrap();
    llavero.guardar("cuenta", SECRETO).unwrap();
    let otro = llavero_en(dir.path());
    otro.desbloquear(descompuesta).unwrap();
    assert!(otro.recuperar("cuenta").unwrap().is_some());
    otro.bloquear();
    otro.desbloquear(compuesta).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let llavero = llavero_en(dir.path());
    llavero.crear(descompuesta, descompuesta).unwrap();
    let otro = llavero_en(dir.path());
    otro.desbloquear(compuesta).unwrap();
    // Y una frase que solo se parece no abre.
    otro.bloquear();
    assert!(matches!(
        otro.desbloquear("cafe con leche y mucho azucar"),
        Err(ErrorLlavero::FraseIncorrecta { .. })
    ));
}

// ---------------------------------------------------------------------------
// Concurrencia y cerrojo
// ---------------------------------------------------------------------------

#[test]
fn dos_ventanas_que_guardan_a_la_vez_dejan_las_dos_claves() {
    let dir = tempfile::tempdir().unwrap();
    llavero_creado(dir.path());
    let ruta = Arc::new(dir.path().to_path_buf());
    let barrera = Arc::new(std::sync::Barrier::new(8));
    let hilos: Vec<_> = (0..8)
        .map(|i| {
            let ruta = ruta.clone();
            let barrera = barrera.clone();
            std::thread::spawn(move || {
                // Cada hilo es «una ventana»: su propio objeto sobre el mismo fichero.
                let ventana = llavero_en(&ruta);
                ventana.desbloquear(FRASE).unwrap();
                barrera.wait();
                for j in 0..3 {
                    ventana
                        .guardar_contrasena(
                            &format!("192.0.2.{i}"),
                            "usuario",
                            &format!("clave-{i}-{j}"),
                        )
                        .unwrap();
                }
            })
        })
        .collect();
    for hilo in hilos {
        hilo.join().unwrap();
    }
    let lector = llavero_en(dir.path());
    lector.desbloquear(FRASE).unwrap();
    for i in 0..8 {
        assert_eq!(
            lector
                .recuperar_contrasena(&format!("192.0.2.{i}"), "usuario")
                .unwrap()
                .as_deref()
                .map(String::as_str),
            Some(format!("clave-{i}-2").as_str()),
            "ventana {i}"
        );
    }
    // Y no quedan temporales junto al llavero.
    let mut nombres: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    nombres.sort();
    assert_eq!(nombres, vec!["llavero.age", "llavero.age.lock"]);
}

#[test]
fn dos_objetos_abiertos_se_releen_y_no_se_pisan() {
    let dir = tempfile::tempdir().unwrap();
    let a = llavero_creado(dir.path());
    let b = llavero_en(dir.path());
    b.desbloquear(FRASE).unwrap();

    a.guardar("de-a", "uno").unwrap();
    // B tiene la memoria vieja y escribe otra clave: relee bajo el cerrojo, así
    // que no borra la de A.
    b.guardar("de-b", "dos").unwrap();
    assert_eq!(b.estado(), EstadoFichero::Abierto);
    assert_eq!(
        b.recuperar("de-a").unwrap().as_deref().map(String::as_str),
        Some("uno"),
        "tras guardar, B ya ve lo de A"
    );
    // A también, en su siguiente escritura.
    a.guardar("de-a2", "tres").unwrap();
    for clave in ["de-a", "de-b", "de-a2"] {
        assert!(a.recuperar(clave).unwrap().is_some(), "A ve {clave}");
    }
    // Y olvidar también relee: A olvida una clave de B sin perder las demás.
    a.olvidar("de-b").unwrap();
    let c = llavero_en(dir.path());
    c.desbloquear(FRASE).unwrap();
    assert!(c.recuperar("de-b").unwrap().is_none());
    assert!(c.recuperar("de-a").unwrap().is_some());
    assert!(c.recuperar("de-a2").unwrap().is_some());
}

#[test]
fn guardar_espera_al_cerrojo_de_otra_ventana_y_luego_escribe() {
    use nix::fcntl::{Flock, FlockArg};
    let dir = tempfile::tempdir().unwrap();
    let llavero = Arc::new(llavero_creado(dir.path()));
    let cerrojo = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(dir.path().join("llavero.age.lock"))
        .unwrap();
    let cerrojo = Flock::lock(cerrojo, FlockArg::LockExclusiveNonblock)
        .map_err(|_| "no se pudo tomar el cerrojo")
        .unwrap();
    let inicio = Instant::now();
    let hilo = {
        let llavero = llavero.clone();
        std::thread::spawn(move || llavero.guardar("espera", "valor"))
    };
    std::thread::sleep(Duration::from_millis(400));
    assert!(!hilo.is_finished(), "guardar espera al cerrojo");
    // Mientras tanto, la interfaz sigue respondiendo: estado y recuperar no
    // esperan ni al cerrojo ni a scrypt.
    let sondeo = Instant::now();
    assert_eq!(llavero.estado(), EstadoFichero::Abierto);
    assert!(llavero.recuperar("espera").unwrap().is_none());
    assert!(sondeo.elapsed() < Duration::from_secs(2));
    drop(cerrojo);
    hilo.join().unwrap().unwrap();
    assert!(inicio.elapsed() >= Duration::from_millis(400));
    assert!(llavero.recuperar("espera").unwrap().is_some());
}

#[test]
fn con_el_cerrojo_ocupado_guardar_se_rinde_tras_el_plazo_y_no_escribe() {
    use nix::fcntl::{Flock, FlockArg};
    let dir = tempfile::tempdir().unwrap();
    let llavero = llavero_creado(dir.path());
    llavero.guardar("antes", "1").unwrap();
    let antes = std::fs::read(ruta_en(dir.path())).unwrap();
    let cerrojo = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(dir.path().join("llavero.age.lock"))
        .unwrap();
    let _cerrojo = Flock::lock(cerrojo, FlockArg::LockExclusiveNonblock)
        .map_err(|_| "no se pudo tomar el cerrojo")
        .unwrap();
    let inicio = Instant::now();
    let error = llavero.guardar("despues", "2").unwrap_err();
    let tardo = inicio.elapsed();
    assert!(error.contains("otra ventana"), "error: {error}");
    assert!(
        tardo >= Duration::from_secs(9) && tardo < Duration::from_secs(20),
        "plazo de ~10 s, no infinito: {tardo:?}"
    );
    assert_eq!(std::fs::read(ruta_en(dir.path())).unwrap(), antes);
    assert!(llavero.recuperar("despues").unwrap().is_none());
}

#[test]
fn bloquear_durante_un_guardar_lento_no_reabre_el_llavero() {
    use nix::fcntl::{Flock, FlockArg};
    let dir = tempfile::tempdir().unwrap();
    let llavero = Arc::new(llavero_creado(dir.path()));
    let cerrojo = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(dir.path().join("llavero.age.lock"))
        .unwrap();
    let cerrojo = Flock::lock(cerrojo, FlockArg::LockExclusiveNonblock)
        .map_err(|_| "no se pudo tomar el cerrojo")
        .unwrap();
    let hilo = {
        let llavero = llavero.clone();
        std::thread::spawn(move || llavero.guardar("a", "1"))
    };
    std::thread::sleep(Duration::from_millis(200));
    // El usuario bloquea mientras la escritura espera su turno.
    llavero.bloquear();
    drop(cerrojo);
    hilo.join().unwrap().unwrap();
    // La escritura que ya se había pedido se hizo, pero el llavero sigue cerrado.
    assert_eq!(llavero.estado(), EstadoFichero::Cerrado);
    assert!(es_bloqueado(&llavero.recuperar("a").unwrap_err()));
    llavero.desbloquear(FRASE).unwrap();
    assert!(llavero.recuperar("a").unwrap().is_some());
}

// ---------------------------------------------------------------------------
// Serie de fallos
// ---------------------------------------------------------------------------

#[test]
fn cinco_fallos_seguidos_imponen_30_s_de_espera_con_el_reloj_inyectado() {
    assert_eq!(FALLOS_POR_SERIE, 5);
    assert_eq!(ESPERA_SERIE, Duration::from_secs(30));
    let dir = tempfile::tempdir().unwrap();
    llavero_creado(dir.path());
    let (reloj, milis) = reloj_falso();
    let llavero = LlaveroFichero::con_ajustes(ruta_en(dir.path()), LOG_N_PRUEBAS, reloj);

    for intento in 1..=5u32 {
        assert_eq!(
            llavero.desbloquear(OTRA_FRASE),
            Err(ErrorLlavero::FraseIncorrecta {
                intentos_seguidos: intento
            })
        );
        assert_eq!(llavero.intentos_seguidos(), intento);
    }
    // El quinto fallo (múltiplo de 5) es el que anota el llamante: los cuatro
    // anteriores no lo eran.
    // Durante la espera, ni la frase buena prueba scrypt.
    let inicio = Instant::now();
    assert_eq!(
        llavero.desbloquear(FRASE),
        Err(ErrorLlavero::Espera { segundos: 30 })
    );
    assert!(inicio.elapsed() < Duration::from_secs(2));
    assert_eq!(llavero.estado(), EstadoFichero::Cerrado);
    milis.store(10_000, Ordering::SeqCst);
    assert_eq!(
        llavero.desbloquear(FRASE),
        Err(ErrorLlavero::Espera { segundos: 20 })
    );
    milis.store(29_500, Ordering::SeqCst);
    assert_eq!(
        llavero.desbloquear(FRASE),
        Err(ErrorLlavero::Espera { segundos: 1 }),
        "los segundos restantes se redondean hacia arriba"
    );
    assert_eq!(llavero.intentos_seguidos(), 5, "la espera no cuenta fallos");

    // Pasados los 30 s se puede volver a probar, y la serie sigue contando.
    milis.store(30_000, Ordering::SeqCst);
    assert_eq!(
        llavero.desbloquear(OTRA_FRASE),
        Err(ErrorLlavero::FraseIncorrecta {
            intentos_seguidos: 6
        })
    );
    for intento in 7..=10u32 {
        assert_eq!(
            llavero.desbloquear(OTRA_FRASE),
            Err(ErrorLlavero::FraseIncorrecta {
                intentos_seguidos: intento
            })
        );
    }
    // El décimo es otro múltiplo de 5: otra espera, desde «ahora».
    assert_eq!(
        llavero.desbloquear(FRASE),
        Err(ErrorLlavero::Espera { segundos: 30 })
    );
    milis.store(60_000, Ordering::SeqCst);
    llavero.desbloquear(FRASE).unwrap();
    assert_eq!(
        llavero.intentos_seguidos(),
        0,
        "un acierto pone la serie a cero"
    );
    assert_eq!(llavero.estado(), EstadoFichero::Abierto);
}

#[test]
fn un_acierto_pone_la_serie_a_cero() {
    let dir = tempfile::tempdir().unwrap();
    llavero_creado(dir.path());
    let llavero = llavero_en(dir.path());
    for intento in 1..=3u32 {
        assert_eq!(
            llavero.desbloquear(OTRA_FRASE),
            Err(ErrorLlavero::FraseIncorrecta {
                intentos_seguidos: intento
            })
        );
    }
    llavero.desbloquear(FRASE).unwrap();
    assert_eq!(llavero.intentos_seguidos(), 0);
    llavero.bloquear();
    assert_eq!(
        llavero.desbloquear(OTRA_FRASE),
        Err(ErrorLlavero::FraseIncorrecta {
            intentos_seguidos: 1
        })
    );
}

#[test]
fn bloquear_no_borra_la_serie_de_fallos() {
    let dir = tempfile::tempdir().unwrap();
    llavero_creado(dir.path());
    let llavero = llavero_en(dir.path());
    let _ = llavero.desbloquear(OTRA_FRASE);
    llavero.bloquear();
    assert_eq!(
        llavero.intentos_seguidos(),
        1,
        "bloquear no premia los fallos"
    );
}

// ---------------------------------------------------------------------------
// Permisos, bloquear, cambiar la frase, borrar y apartar
// ---------------------------------------------------------------------------

#[test]
fn el_fichero_y_su_cerrojo_son_600() {
    let dir = tempfile::tempdir().unwrap();
    let llavero = llavero_creado(dir.path());
    llavero.guardar("cuenta", SECRETO).unwrap();
    for nombre in ["llavero.age", "llavero.age.lock"] {
        let modo = std::fs::metadata(dir.path().join(nombre))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(modo, 0o600, "{nombre}");
    }
}

#[test]
fn bloquear_borra_la_memoria_pero_no_toca_el_fichero() {
    let dir = tempfile::tempdir().unwrap();
    let llavero = llavero_creado(dir.path());
    llavero.guardar("cuenta", SECRETO).unwrap();
    let antes = std::fs::read(ruta_en(dir.path())).unwrap();
    assert!(llavero.recuperar("cuenta").unwrap().is_some());

    llavero.bloquear();
    assert_eq!(llavero.estado(), EstadoFichero::Cerrado);
    assert_eq!(llavero.recuperar("cuenta"), Err(BLOQUEADO.to_string()));
    assert_eq!(std::fs::read(ruta_en(dir.path())).unwrap(), antes);
    // Cerrado, guardar y olvidar también dicen «bloqueado».
    assert_eq!(llavero.guardar("otra", "x"), Err(BLOQUEADO.to_string()));
    assert_eq!(llavero.olvidar("cuenta"), Err(BLOQUEADO.to_string()));
    assert_eq!(std::fs::read(ruta_en(dir.path())).unwrap(), antes);

    llavero.desbloquear(FRASE).unwrap();
    assert!(llavero.recuperar("cuenta").unwrap().is_some());
}

#[test]
fn sin_fichero_guardar_y_olvidar_piden_crearlo_y_recuperar_no_hay() {
    let dir = tempfile::tempdir().unwrap();
    let llavero = llavero_en(dir.path());
    assert_eq!(
        llavero.guardar("cuenta", SECRETO),
        Err(SIN_CREAR.to_string())
    );
    assert_eq!(llavero.olvidar("cuenta"), Err(SIN_CREAR.to_string()));
    assert!(llavero.recuperar("cuenta").unwrap().is_none());
    assert_eq!(
        llavero.guardar_contrasena("192.0.2.1", "usuario", "x"),
        Err(SIN_CREAR.to_string())
    );
    assert!(!ruta_en(dir.path()).exists(), "no se crea nada solo");
    assert!(!es_bloqueado(SIN_CREAR));
}

#[test]
fn si_alguien_borra_el_fichero_a_mano_es_como_si_no_existiera() {
    let dir = tempfile::tempdir().unwrap();
    let llavero = llavero_creado(dir.path());
    llavero.guardar("cuenta", SECRETO).unwrap();
    std::fs::remove_file(ruta_en(dir.path())).unwrap();
    assert_eq!(llavero.estado(), EstadoFichero::Inexistente);
    assert!(llavero.recuperar("cuenta").unwrap().is_none());
    assert_eq!(llavero.guardar("otra", "x"), Err(SIN_CREAR.to_string()));
    // La memoria se olvidó: un llavero nuevo en la misma ruta no hereda nada.
    llavero.crear(OTRA_FRASE, OTRA_FRASE).unwrap();
    assert!(llavero.recuperar("cuenta").unwrap().is_none());
}

#[test]
fn cambiar_la_frase_recifra_y_deja_el_llavero_abierto_con_la_nueva() {
    let dir = tempfile::tempdir().unwrap();
    let llavero = llavero_creado(dir.path());
    llavero.guardar("cuenta", SECRETO).unwrap();
    let antes = std::fs::read(ruta_en(dir.path())).unwrap();

    llavero
        .cambiar_frase(FRASE, OTRA_FRASE, OTRA_FRASE)
        .unwrap();
    assert_eq!(llavero.estado(), EstadoFichero::Abierto);
    let despues = std::fs::read(ruta_en(dir.path())).unwrap();
    assert_ne!(antes, despues);
    assert_eq!(
        descifrar_age(&despues, OTRA_FRASE),
        descifrar_age(&antes, FRASE),
        "mismo contenido, otra frase"
    );
    assert_eq!(
        llavero
            .recuperar("cuenta")
            .unwrap()
            .as_deref()
            .map(String::as_str),
        Some(SECRETO)
    );
    // Sigue pudiéndose guardar con la nueva.
    llavero.guardar("otra", "x").unwrap();

    let otro = llavero_en(dir.path());
    assert!(matches!(
        otro.desbloquear(FRASE),
        Err(ErrorLlavero::FraseIncorrecta { .. })
    ));
    otro.desbloquear(OTRA_FRASE).unwrap();
    assert!(otro.recuperar("cuenta").unwrap().is_some());
    assert!(otro.recuperar("otra").unwrap().is_some());
}

#[test]
fn cambiar_la_frase_abre_con_la_actual_aunque_este_cerrado_y_cuenta_los_fallos() {
    let dir = tempfile::tempdir().unwrap();
    {
        let llavero = llavero_creado(dir.path());
        llavero.guardar("cuenta", SECRETO).unwrap();
    }
    let (reloj, milis) = reloj_falso();
    let llavero = LlaveroFichero::con_ajustes(ruta_en(dir.path()), LOG_N_PRUEBAS, reloj);
    assert_eq!(llavero.estado(), EstadoFichero::Cerrado);

    let antes = std::fs::read(ruta_en(dir.path())).unwrap();
    for intento in 1..=4u32 {
        assert_eq!(
            llavero.cambiar_frase(
                OTRA_FRASE,
                "tercera frase del llavero",
                "tercera frase del llavero"
            ),
            Err(ErrorLlavero::FraseIncorrecta {
                intentos_seguidos: intento
            })
        );
    }
    assert_eq!(
        llavero.cambiar_frase(
            OTRA_FRASE,
            "tercera frase del llavero",
            "tercera frase del llavero"
        ),
        Err(ErrorLlavero::FraseIncorrecta {
            intentos_seguidos: 5
        })
    );
    // Es otra puerta para adivinar la frase: respeta la misma espera.
    assert_eq!(
        llavero.cambiar_frase(
            FRASE,
            "tercera frase del llavero",
            "tercera frase del llavero"
        ),
        Err(ErrorLlavero::Espera { segundos: 30 })
    );
    assert_eq!(
        llavero.desbloquear(FRASE),
        Err(ErrorLlavero::Espera { segundos: 30 })
    );
    assert_eq!(std::fs::read(ruta_en(dir.path())).unwrap(), antes);

    milis.store(31_000, Ordering::SeqCst);
    llavero
        .cambiar_frase(
            FRASE,
            "tercera frase del llavero",
            "tercera frase del llavero",
        )
        .unwrap();
    assert_eq!(llavero.estado(), EstadoFichero::Abierto);
    assert_eq!(llavero.intentos_seguidos(), 0);
    assert!(llavero.recuperar("cuenta").unwrap().is_some());
}

#[test]
fn cambiar_la_frase_valida_la_nueva_antes_de_gastar_nada() {
    let dir = tempfile::tempdir().unwrap();
    let llavero = llavero_creado(dir.path());
    let antes = std::fs::read(ruta_en(dir.path())).unwrap();
    assert!(matches!(
        llavero.cambiar_frase(FRASE, "corta", "corta"),
        Err(ErrorLlavero::FraseInvalida(_))
    ));
    assert!(matches!(
        llavero.cambiar_frase(FRASE, OTRA_FRASE, FRASE),
        Err(ErrorLlavero::FraseInvalida(_))
    ));
    assert_eq!(llavero.intentos_seguidos(), 0);
    assert_eq!(std::fs::read(ruta_en(dir.path())).unwrap(), antes);
    // Sin fichero, no hay nada que cambiar.
    llavero.borrar().unwrap();
    assert_eq!(
        llavero.cambiar_frase(FRASE, OTRA_FRASE, OTRA_FRASE),
        Err(ErrorLlavero::Inexistente)
    );
}

#[test]
fn si_otra_ventana_cambia_la_frase_guardar_avisa_se_cierra_y_no_escribe() {
    let dir = tempfile::tempdir().unwrap();
    let a = llavero_creado(dir.path());
    let b = llavero_en(dir.path());
    b.desbloquear(FRASE).unwrap();
    b.cambiar_frase(FRASE, OTRA_FRASE, OTRA_FRASE).unwrap();
    let escrito_por_b = std::fs::read(ruta_en(dir.path())).unwrap();

    let error = a.guardar("de-a", "x").unwrap_err();
    assert!(error.contains("frase"), "error claro: {error}");
    assert!(error.contains("otra ventana"), "error: {error}");
    assert_eq!(
        std::fs::read(ruta_en(dir.path())).unwrap(),
        escrito_por_b,
        "no se escribió nada"
    );
    // A ya no tiene una frase que sirva: pide la nueva.
    assert_eq!(a.estado(), EstadoFichero::Cerrado);
    assert_eq!(a.intentos_seguidos(), 0, "no es un intento del usuario");
    a.desbloquear(OTRA_FRASE).unwrap();
    a.guardar("de-a", "x").unwrap();
}

#[test]
fn borrar_quita_el_fichero_y_la_memoria_sin_pedir_la_frase() {
    let dir = tempfile::tempdir().unwrap();
    {
        let llavero = llavero_creado(dir.path());
        llavero.guardar("cuenta", SECRETO).unwrap();
    }
    // Un objeto cerrado, sin la frase, también puede borrar.
    let llavero = llavero_en(dir.path());
    assert_eq!(llavero.estado(), EstadoFichero::Cerrado);
    llavero.borrar().unwrap();
    assert!(!ruta_en(dir.path()).exists());
    assert_eq!(llavero.estado(), EstadoFichero::Inexistente);
    // Borrar lo que no hay no es un error.
    llavero.borrar().unwrap();

    // Abierto: la memoria desaparece con el fichero.
    let llavero = llavero_creado(dir.path());
    llavero.guardar("cuenta", SECRETO).unwrap();
    llavero.borrar().unwrap();
    assert_eq!(llavero.estado(), EstadoFichero::Inexistente);
    assert!(llavero.recuperar("cuenta").unwrap().is_none());
    // Y se puede volver a crear con otra frase.
    llavero.crear(OTRA_FRASE, OTRA_FRASE).unwrap();
    assert!(llavero.recuperar("cuenta").unwrap().is_none());
}

#[test]
fn apartar_un_danado_lo_renombra_con_fecha_y_con_sufijo_si_ya_existe() {
    let dir = tempfile::tempdir().unwrap();
    let fecha = chrono::Local::now().format("%Y-%m-%d").to_string();
    let llavero = llavero_en(dir.path());
    // Sin fichero no hay nada que apartar.
    assert_eq!(llavero.apartar_danado(), Err(ErrorLlavero::Inexistente));

    let mut copias = Vec::new();
    for vuelta in 0..3 {
        let basura = format!("basura número {vuelta}").into_bytes();
        std::fs::write(ruta_en(dir.path()), &basura).unwrap();
        assert!(es_danado(&llavero.estado()));
        let copia = llavero.apartar_danado().unwrap();
        assert_eq!(copia.parent(), Some(dir.path()), "en el mismo directorio");
        assert_eq!(
            std::fs::read(&copia).unwrap(),
            basura,
            "la copia es el original"
        );
        assert!(!ruta_en(dir.path()).exists());
        assert_eq!(llavero.estado(), EstadoFichero::Inexistente);
        copias.push(copia);
    }
    let nombres: Vec<String> = copias
        .iter()
        .map(|c| c.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        nombres,
        vec![
            format!("llavero.age.danado-{fecha}"),
            format!("llavero.age.danado-{fecha}-2"),
            format!("llavero.age.danado-{fecha}-3"),
        ]
    );
    // Nunca borra: las tres siguen ahí.
    for copia in copias {
        assert!(copia.exists());
    }
    // Y se puede empezar de cero.
    llavero.crear(FRASE, FRASE).unwrap();
    assert_eq!(llavero.estado(), EstadoFichero::Abierto);
}

#[test]
fn borrar_no_borra_un_danado_y_apartar_no_aparta_uno_sano() {
    let dir = tempfile::tempdir().unwrap();
    // Dañado: borrar se niega, no toca el fichero y dice que se aparta.
    let basura = b"esto no es un llavero".to_vec();
    std::fs::write(ruta_en(dir.path()), &basura).unwrap();
    let llavero = llavero_en(dir.path());
    assert_eq!(llavero.borrar(), Err(ErrorLlavero::NoSeBorraDanado));
    assert!(llavero
        .borrar()
        .unwrap_err()
        .to_string()
        .contains("se aparta"));
    assert_eq!(std::fs::read(ruta_en(dir.path())).unwrap(), basura);
    // Apartarlo sí, y entonces ya no hay nada que borrar.
    llavero.apartar_danado().unwrap();
    llavero.borrar().unwrap();

    // Sano: apartar se niega (otra ventana lo restauró mientras el diálogo
    // de «dañado» seguía abierto) y el fichero queda donde estaba.
    let llavero = llavero_creado(dir.path());
    llavero.guardar("cuenta", SECRETO).unwrap();
    let antes = std::fs::read(ruta_en(dir.path())).unwrap();
    assert_eq!(llavero.apartar_danado(), Err(ErrorLlavero::NoEstaDanado));
    let otro = llavero_en(dir.path());
    assert_eq!(otro.estado(), EstadoFichero::Cerrado);
    assert_eq!(otro.apartar_danado(), Err(ErrorLlavero::NoEstaDanado));
    assert_eq!(std::fs::read(ruta_en(dir.path())).unwrap(), antes);
    assert_eq!(llavero.estado(), EstadoFichero::Abierto);
}

#[test]
fn un_contenido_ilegible_visto_con_la_frase_buena_se_ve_danado_y_se_aparta() {
    let dir = tempfile::tempdir().unwrap();
    // Una cabecera buena con un JSON que MAGI nunca escribiría.
    let roto = cifrar_age(b"{esto no es json", FRASE, LOG_N_PRUEBAS);
    let llavero = con_fichero(dir.path(), &roto);
    // Antes de abrirlo solo se ve la cabecera: parece cerrado.
    assert_eq!(llavero.estado(), EstadoFichero::Cerrado);
    // Con la frase buena se descubre el daño y desde entonces se sabe.
    assert!(matches!(
        llavero.desbloquear(FRASE),
        Err(ErrorLlavero::Danado(_))
    ));
    assert!(es_danado(&llavero.estado()));
    assert_eq!(llavero.borrar(), Err(ErrorLlavero::NoSeBorraDanado));
    assert_eq!(std::fs::read(ruta_en(dir.path())).unwrap(), roto);
    // Apartarlo funciona aunque la cabecera sea buena.
    let copia = llavero.apartar_danado().unwrap();
    assert_eq!(std::fs::read(copia).unwrap(), roto);
    assert_eq!(llavero.estado(), EstadoFichero::Inexistente);

    // Si otra ventana lo sustituye por uno bueno, el aviso ya no vale.
    let llavero = con_fichero(dir.path(), &roto);
    assert!(llavero.desbloquear(FRASE).is_err());
    assert!(es_danado(&llavero.estado()));
    let bueno = llavero_en(dir.path());
    std::fs::remove_file(ruta_en(dir.path())).unwrap();
    bueno.crear(FRASE, FRASE).unwrap();
    assert_eq!(llavero.estado(), EstadoFichero::Cerrado);
    assert_eq!(llavero.apartar_danado(), Err(ErrorLlavero::NoEstaDanado));
}

/// Un llavero con el factor de producción (en depuración, ~0,4 s por scrypt
/// gracias al perfil de `Cargo.toml`) para tener un hueco en el que bloquear.
fn llavero_de_produccion(dir: &Path) -> LlaveroFichero {
    LlaveroFichero::nuevo(ruta_en(dir))
}

#[test]
fn bloquear_durante_un_desbloqueo_en_vuelo_lo_deja_cerrado() {
    let dir = tempfile::tempdir().unwrap();
    llavero_de_produccion(dir.path())
        .crear(FRASE, FRASE)
        .unwrap();
    let llavero = Arc::new(llavero_de_produccion(dir.path()));
    let hilo = {
        let llavero = llavero.clone();
        std::thread::spawn(move || llavero.desbloquear(FRASE))
    };
    std::thread::sleep(Duration::from_millis(100));
    llavero.bloquear();
    assert_eq!(hilo.join().unwrap(), Err(ErrorLlavero::Bloqueado));
    assert_eq!(llavero.estado(), EstadoFichero::Cerrado);
    assert!(es_bloqueado(&llavero.recuperar("a").unwrap_err()));
    // Y la frase sigue valiendo.
    llavero.desbloquear(FRASE).unwrap();
}

#[test]
fn bloquear_durante_un_cambio_de_frase_en_vuelo_no_reabre_con_la_nueva() {
    let dir = tempfile::tempdir().unwrap();
    llavero_de_produccion(dir.path())
        .crear(FRASE, FRASE)
        .unwrap();
    let llavero = Arc::new(llavero_de_produccion(dir.path()));
    llavero.desbloquear(FRASE).unwrap();
    let hilo = {
        let llavero = llavero.clone();
        std::thread::spawn(move || llavero.cambiar_frase(FRASE, OTRA_FRASE, OTRA_FRASE))
    };
    std::thread::sleep(Duration::from_millis(150));
    llavero.bloquear();
    assert_eq!(hilo.join().unwrap(), Err(ErrorLlavero::Bloqueado));
    assert_eq!(llavero.estado(), EstadoFichero::Cerrado);
    // El cambio que se pidió se hizo: la frase nueva abre y la vieja no.
    assert!(matches!(
        llavero.desbloquear(FRASE),
        Err(ErrorLlavero::FraseIncorrecta { .. })
    ));
    llavero.desbloquear(OTRA_FRASE).unwrap();
}

#[test]
fn bloquear_durante_una_creacion_en_vuelo_deja_el_fichero_y_lo_cerrado() {
    let dir = tempfile::tempdir().unwrap();
    let llavero = Arc::new(llavero_de_produccion(dir.path()));
    let hilo = {
        let llavero = llavero.clone();
        std::thread::spawn(move || llavero.crear(FRASE, FRASE))
    };
    std::thread::sleep(Duration::from_millis(100));
    llavero.bloquear();
    assert_eq!(hilo.join().unwrap(), Err(ErrorLlavero::Bloqueado));
    assert_eq!(llavero.estado(), EstadoFichero::Cerrado);
    llavero.desbloquear(FRASE).unwrap();
}

#[test]
fn info_fichero_da_ruta_tamano_y_fecha_sin_abrirlo() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(info_fichero(&ruta_en(dir.path())), Ok(None));
    llavero_creado(dir.path());
    let info = info_fichero(&ruta_en(dir.path())).unwrap().unwrap();
    assert_eq!(info.ruta, ruta_en(dir.path()));
    assert_eq!(
        info.bytes,
        std::fs::metadata(ruta_en(dir.path())).unwrap().len()
    );
    let ahora = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    assert!(
        (ahora - info.modificado).abs() < 60,
        "mtime en segundos Unix"
    );
}

#[test]
fn un_llavero_que_pasaria_del_tope_no_se_escribe() {
    let dir = tempfile::tempdir().unwrap();
    let llavero = llavero_creado(dir.path());
    llavero.guardar("pequeña", "1").unwrap();
    let antes = std::fs::read(ruta_en(dir.path())).unwrap();
    let enorme = "x".repeat(TOPE_BYTES);
    let error = llavero.guardar("enorme", &enorme).unwrap_err();
    assert!(error.contains("1 MiB"), "error: {error}");
    assert_eq!(std::fs::read(ruta_en(dir.path())).unwrap(), antes);
    assert!(llavero.recuperar("enorme").unwrap().is_none());
    assert!(llavero.recuperar("pequeña").unwrap().is_some());
    assert_eq!(llavero.estado(), EstadoFichero::Abierto);
}

#[test]
fn el_llavero_escrito_por_otra_ventana_no_cierra_este() {
    let dir = tempfile::tempdir().unwrap();
    let a = llavero_creado(dir.path());
    let b = llavero_en(dir.path());
    b.desbloquear(FRASE).unwrap();
    b.guardar("de-b", "1").unwrap();
    // A no ha leído el cambio, pero sigue abierto: la frase es la misma.
    assert_eq!(a.estado(), EstadoFichero::Abierto);
    // Si el fichero se estropea de verdad, sí se ve.
    std::fs::write(ruta_en(dir.path()), b"roto").unwrap();
    assert!(es_danado(&a.estado()));
}

// ---------------------------------------------------------------------------
// Nada de secretos en Debug ni en los errores; las interfaces
// ---------------------------------------------------------------------------

#[test]
fn ni_debug_ni_los_errores_llevan_la_frase_ni_las_entradas() {
    let dir = tempfile::tempdir().unwrap();
    let llavero = llavero_creado(dir.path());
    llavero.guardar("usuario@192.0.2.10", SECRETO).unwrap();

    let mut textos = vec![format!("{llavero:?}"), format!("{:?}", llavero.estado())];
    let otro = llavero_en(dir.path());
    // Errores de todas las clases.
    for error in [
        otro.desbloquear(OTRA_FRASE).unwrap_err(),
        otro.desbloquear("").unwrap_err(),
        otro.crear(FRASE, FRASE).unwrap_err(),
        otro.cambiar_frase(OTRA_FRASE, FRASE, FRASE).unwrap_err(),
        otro.cambiar_frase(FRASE, FRASE, OTRA_FRASE).unwrap_err(),
        otro.cambiar_frase(FRASE, "corta", "corta").unwrap_err(),
        preparar_frase(FRASE, Some(OTRA_FRASE)).unwrap_err(),
        preparar_frase("corta", None).unwrap_err(),
        ErrorLlavero::Bloqueado,
        ErrorLlavero::Espera { segundos: 3 },
    ] {
        textos.push(format!("{error}"));
        textos.push(format!("{error:?}"));
    }
    textos.push(otro.recuperar("x").unwrap_err());
    textos.push(llavero.guardar("a", &"y".repeat(TOPE_BYTES)).unwrap_err());
    // Un fichero dañado con la frase buena: el motivo no cita el contenido.
    let daniado = cifrar_age(
        format!("{{\"formato\":2,\"entradas\":{{\"x\":\"{SECRETO}\"}}}}").as_bytes(),
        FRASE,
        LOG_N_PRUEBAS,
    );
    let tercero = con_fichero(dir.path(), &daniado);
    textos.push(format!("{:?}", tercero.desbloquear(FRASE).unwrap_err()));
    let roto = cifrar_age(
        format!("{{\"formato\":1,\"entradas\":{{\"x\":5,\"y\":\"{SECRETO}\"}}}}").as_bytes(),
        FRASE,
        LOG_N_PRUEBAS,
    );
    let cuarto = con_fichero(dir.path(), &roto);
    textos.push(format!("{:?}", cuarto.desbloquear(FRASE).unwrap_err()));

    for texto in textos {
        for secreto in [FRASE, OTRA_FRASE, SECRETO, "usuario@192.0.2.10"] {
            assert!(!texto.contains(secreto), "«{secreto}» aparece en «{texto}»");
        }
    }
}

/// La misma secuencia de operaciones contra cualquier llavero.
fn recorrido_de_interfaz(llavero: &(impl LlaveroSecreto + LlaveroHost)) -> Vec<String> {
    let mut salida = Vec::new();
    let mut anota = |texto: String| salida.push(texto);
    anota(format!(
        "{:?}",
        llavero.recuperar("nada").map(|v| v.is_some())
    ));
    anota(format!(
        "{:?}",
        llavero.guardar("sincronizacion", "AGE-SECRET-KEY-1X")
    ));
    anota(format!(
        "{:?}",
        llavero.guardar_contrasena("192.0.2.10", "usuario", "clave de prueba")
    ));
    anota(format!(
        "{:?}",
        llavero
            .recuperar_contrasena("192.0.2.10", "usuario")
            .unwrap()
            .map(|v| v.to_string())
    ));
    // La misma cuenta, escrita a mano como `usuario@host`.
    anota(format!(
        "{:?}",
        llavero
            .recuperar("usuario@192.0.2.10")
            .unwrap()
            .map(|v| v.to_string())
    ));
    anota(format!(
        "{:?}",
        llavero
            .recuperar_contrasena("192.0.2.10", "otro")
            .unwrap()
            .is_some()
    ));
    anota(format!("{:?}", llavero.olvidar("usuario@192.0.2.10")));
    anota(format!("{:?}", llavero.olvidar("usuario@192.0.2.10")));
    anota(format!(
        "{:?}",
        llavero
            .recuperar("sincronizacion")
            .unwrap()
            .map(|v| v.to_string())
    ));
    salida
}

#[test]
fn implementa_llavero_secreto_y_llavero_host_como_el_de_memoria() {
    let dir = tempfile::tempdir().unwrap();
    let de_fichero = llavero_creado(dir.path());
    let de_memoria = LlaveroEnMemoria::nuevo();
    let esperado = recorrido_de_interfaz(&de_memoria);
    assert_eq!(recorrido_de_interfaz(&de_fichero), esperado);
    // Y la cuenta es `usuario@host`, como en el de memoria.
    de_fichero
        .guardar_contrasena("192.0.2.20", "usuario", "otra")
        .unwrap();
    let otro = llavero_en(dir.path());
    otro.desbloquear(FRASE).unwrap();
    assert_eq!(
        otro.recuperar("usuario@192.0.2.20")
            .unwrap()
            .as_deref()
            .map(String::as_str),
        Some("otra")
    );
    // Como `Arc<dyn …>`, que es como lo usa quien lo inyecta.
    let _: Arc<dyn LlaveroSecreto> = Arc::new(llavero_en(dir.path()));
    let _: Arc<dyn LlaveroHost> = Arc::new(llavero_en(dir.path()));
}
