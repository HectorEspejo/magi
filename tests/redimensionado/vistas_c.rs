//! Instantáneas y pruebas de disposición del grupo C: la vista Sesión.
//!
//! La pantalla remota se vuelca como lo haría la tarea de lectura del cliente
//! con una `PantallaCompleta` (en el arnés no hay tarea de lectura: la App
//! ignora ese mensaje) y el tamaño remoto llega con `Redimensionada`, como si
//! el servidor hubiera aplicado el `Redimensionar` de esta ventana.

use crossterm::event::{KeyCode, KeyModifiers};

use magi::app::{alto_pty, Evento};
use magi::protocolo::{EstadoSesionRemota, InfoSesion, MensajeServidor};
use magi::tema::Tema;

use crate::arnes::{self, sesion, AppPrueba};
use crate::semilla::Sembrado;

/// Los tres tamaños obligatorios.
const TAMANOS: [(u16, u16); 3] = [(40, 12), (80, 24), (200, 60)];

/// Hace `segundos` (lejos de los redondeos de minuto).
fn hace(segundos: i64) -> i64 {
    chrono::Utc::now().timestamp() - segundos
}

/// Las pestañas de la maqueta §6.2: hetzner-01 (la activa), mac-mini-m1 con
/// actividad sin ver y rincon-dev caída.
fn tres_pestanas(sembrado: &Sembrado) -> Vec<InfoSesion> {
    vec![
        InfoSesion {
            host_id: sembrado.host("hetzner-01"),
            abierta_en: hace(18 * 60 + 30),
            identidad: "ed25519 (agente)".to_string(),
            ventanas: 2,
            ..sesion(1, "hetzner-01")
        },
        InfoSesion {
            host_id: sembrado.host("mac-mini-m1"),
            abierta_en: hace(4 * 60 + 30),
            actividad_no_vista: true,
            ..sesion(2, "mac-mini-m1")
        },
        InfoSesion {
            host_id: sembrado.host("rincon-dev"),
            abierta_en: hace(2 * 3600 + 30),
            estado: EstadoSesionRemota::Caida,
            motivo: Some("conexión cerrada por el remoto".to_string()),
            ..sesion(3, "rincon-dev")
        },
    ]
}

/// Doce pestañas: más de las nueve que alcanza el prefijo `1`-`9`.
fn doce_pestanas(sembrado: &Sembrado) -> Vec<InfoSesion> {
    let hosts = crate::semilla::HOSTS.map(|(nombre, ..)| nombre);
    (0..12u32)
        .map(|posicion| {
            let host = hosts[posicion as usize % hosts.len()];
            let nombre = if posicion < 7 {
                host.to_string()
            } else {
                format!("{host} (2)")
            };
            InfoSesion {
                id: posicion + 1,
                nombre,
                host_id: sembrado.host(host),
                host_nombre: host.to_string(),
                abierta_en: hace(18 * 60 + 30),
                actividad_no_vista: posicion % 3 == 1,
                ..sesion(posicion + 1, host)
            }
        })
        .collect()
}

/// Pantalla de `htop` redibujada al tamaño dado.
fn htop(cols: u16, filas: u16) -> String {
    format!("\x1b[H\x1b[2Jroot@hetzner-01:~# htop\r\n  (htop repintado a {cols}x{filas})\r\n")
}

/// Vuelca una pantalla completa en una pestaña, como la tarea de lectura.
fn volcar(prueba: &mut AppPrueba, sesion_id: u32, texto: &str, cols: u16, filas: u16) {
    prueba
        .app
        .pantallas
        .volcar(sesion_id, filas, cols, texto.as_bytes());
    prueba.evento(Evento::Pantallas(vec![sesion_id]));
}

fn activa(prueba: &AppPrueba) -> u32 {
    let indice = prueba.app.pestana_activa.expect("pestaña activa");
    prueba.app.pestanas[indice].sesion_id
}

/// El servidor fija el tamaño remoto de la pestaña activa (quién impone el
/// mínimo en `ventana_minima`) y el remoto repinta a ese tamaño.
fn remoto(prueba: &mut AppPrueba, cols: u16, filas: u16, ventana_minima: Option<u32>) {
    let sesion_id = activa(prueba);
    prueba.servidor(MensajeServidor::Redimensionada {
        sesion_id,
        cols,
        filas,
        ventana_minima,
    });
    volcar(prueba, sesion_id, &htop(cols, filas), cols, filas);
}

/// Esta ventana es la única adjunta: el remoto sigue a su tamaño aplicado.
fn remoto_sigue(prueba: &mut AppPrueba) {
    let (cols, filas) = prueba.app.tamano_pty();
    remoto(prueba, cols, filas, Some(1));
}

/// El prefijo (`Ctrl+]`) y una tecla.
fn prefijo(prueba: &mut AppPrueba, tecla: char) {
    prueba.tecla_con(KeyCode::Char(']'), KeyModifiers::CONTROL);
    prueba.tecla(KeyCode::Char(tecla));
}

/// App en la vista Sesión con las pestañas dadas, a `cols`×`filas` y con el
/// remoto a su tamaño. Se entra a 80×24 (la lista Sesiones necesita 40×12).
fn en_sesion(
    cols: u16,
    filas: u16,
    tema: Tema,
    pestanas: fn(&Sembrado) -> Vec<InfoSesion>,
) -> AppPrueba {
    let (mut prueba, sembrado) = AppPrueba::con_semilla_y_tema(80, 24, tema);
    prueba.entrar_en_sesion(pestanas(&sembrado));
    assert_eq!(prueba.app.pestana_activa, Some(0));
    prueba.redimensionar(cols, filas);
    remoto_sigue(&mut prueba);
    prueba
}

/// Doce pestañas con la 11 activa (dos veces «anterior» desde la primera).
fn en_sesion_doce(cols: u16, filas: u16, tema: Tema) -> AppPrueba {
    let mut prueba = en_sesion(cols, filas, tema, doce_pestanas);
    prefijo(&mut prueba, 'p');
    prefijo(&mut prueba, 'p');
    assert_eq!(prueba.app.pestana_activa, Some(10));
    remoto_sigue(&mut prueba);
    prueba
}

/// La pestaña 3 (rincon-dev, caída) activa.
fn en_sesion_caida(cols: u16, filas: u16, tema: Tema) -> AppPrueba {
    let mut prueba = en_sesion(cols, filas, tema, tres_pestanas);
    prefijo(&mut prueba, '3');
    assert_eq!(prueba.app.pestana_activa, Some(2));
    prueba
}

/// Pestaña compartida con una ventana más pequeña (la 2): el remoto mide
/// `cols`×`filas` y esta ventana rellena lo que sobra.
fn con_relleno(ventana: (u16, u16), remoto_menor: (u16, u16), tema: Tema) -> AppPrueba {
    let mut prueba = en_sesion(ventana.0, ventana.1, tema, tres_pestanas);
    remoto(&mut prueba, remoto_menor.0, remoto_menor.1, Some(2));
    prueba
}

fn fila(prueba: &AppPrueba, indice: usize) -> String {
    prueba
        .texto()
        .lines()
        .nth(indice)
        .unwrap_or_default()
        .to_string()
}

/// La barra de pestañas (segunda fila).
fn barra_pestanas(prueba: &AppPrueba) -> String {
    fila(prueba, 1)
}

/// La barra de estado de la sesión (penúltima fila; la última es la global).
fn barra_estado(prueba: &AppPrueba) -> String {
    let (_, filas) = prueba.tamano_pintado();
    fila(prueba, usize::from(filas) - 2)
}

fn sin_glifos_unicode(texto: &str) -> bool {
    texto.chars().all(|c| c.is_ascii() || c.is_alphabetic())
}

#[test]
fn sesion_a_los_tres_tamanos() {
    for (cols, filas) in TAMANOS {
        let prueba = en_sesion(cols, filas, Tema::respaldo(), tres_pestanas);
        assert_eq!(prueba.tamano_pintado(), (cols, filas));
        assert!(
            !prueba.app.disposicion().aviso,
            "Sesión cabe a {cols}×{filas}"
        );
        prueba.instantanea(&format!("sesion_{cols}x{filas}"));
    }
}

/// Maqueta §6.2: a 60×16 la barra de pestañas es compacta (la activa con
/// número y nombre, las demás solo glifo y número) y la de estado se queda
/// con lo prioritario, con la ayuda abreviada.
#[test]
fn sesion_maqueta_60x16() {
    let prueba = en_sesion(60, 16, Tema::respaldo(), tres_pestanas);
    prueba.instantanea("sesion_60x16");
    assert_eq!(barra_pestanas(&prueba), "▸● 1 hetzner-01 │ ◐ 2 │ ✕ 3 │ +");
    let estado = barra_estado(&prueba);
    assert!(
        estado.starts_with(" ● 1/3 · hetzner-01 · 18 m · "),
        "{estado}"
    );
    assert!(estado.ends_with(" · ^] ?"), "{estado}");
    assert!(!estado.contains("conectado") && !estado.contains("ventanas"));
    // El PTY recibe el área real: 60 columnas y 16 − 4 filas.
    assert_eq!(prueba.app.tamano_pty(), (60, 12));
    assert!(prueba.texto().contains("(htop repintado a 60x12)"));
}

/// El mínimo de Sesión (40×8) sustituye al global: a 40×8 se pinta la vista y
/// por debajo, el aviso con «mínimo 40×8».
#[test]
fn sesion_en_su_minimo_y_por_debajo() {
    let prueba = en_sesion(40, 8, Tema::respaldo(), tres_pestanas);
    assert!(!prueba.app.disposicion().aviso);
    prueba.instantanea("sesion_40x8");
    assert_eq!(barra_pestanas(&prueba), "▸● 1 hetzner-01 │ ◐ 2 │ ✕ 3 │ +");
    assert!(barra_estado(&prueba).starts_with(" ● 1/3 · "));

    for (cols, filas) in [(39, 8), (40, 7)] {
        let mut prueba = en_sesion(40, 8, Tema::respaldo(), tres_pestanas);
        prueba.redimensionar(cols, filas);
        assert!(prueba.app.disposicion().aviso, "{cols}×{filas}");
        assert!(prueba.texto().contains("mínimo 40×8"), "{}", prueba.texto());
        assert!(!prueba.texto().contains("SESIÓN"), "sin vista recortada");
    }
    let mut prueba = en_sesion(40, 8, Tema::respaldo(), tres_pestanas);
    prueba.redimensionar(39, 7);
    prueba.instantanea("sesion_aviso_39x7");
}

/// El contenido mide exactamente `alto_pty(filas)` y todo el ancho: la
/// primera fila remota va bajo las pestañas y la última justo encima de la
/// barra de estado.
#[test]
fn el_terminal_ocupa_el_area_real_del_pty() {
    for (cols, filas) in [(40, 8), (40, 12), (60, 16), (80, 24), (200, 60)] {
        let mut prueba = en_sesion(cols, filas, Tema::respaldo(), tres_pestanas);
        let (cols_pty, filas_pty) = prueba.app.tamano_pty();
        assert_eq!((cols_pty, filas_pty), (cols, alto_pty(filas)));
        let lineas: Vec<String> = (1..=filas_pty)
            .map(|numero| {
                let mut linea = format!("L{numero:02}");
                // La primera ocupa todo el ancho: se ve entera o no cabe.
                if numero == 1 {
                    linea.push_str(&"=".repeat(usize::from(cols_pty) - 3));
                }
                linea
            })
            .collect();
        let pantalla = format!("\x1b[H\x1b[2J{}", lineas.join("\r\n"));
        volcar(&mut prueba, 1, &pantalla, cols_pty, filas_pty);
        let texto = prueba.texto();
        let pintadas: Vec<&str> = texto.lines().collect();
        assert_eq!(pintadas[2], lineas[0], "{cols}×{filas}:\n{texto}");
        let ultima = usize::from(filas) - 3;
        assert!(
            pintadas[ultima].starts_with(&format!("L{filas_pty:02}")),
            "{cols}×{filas}:\n{texto}"
        );
        assert!(pintadas[ultima + 1].starts_with(" ● 1/3"), "{texto}");
    }
}

/// Barra de pestañas: completa en modo normal, compacta en estrecho (solo
/// glifo y número salvo la activa) y `+ nueva` / `+`.
#[test]
fn barra_de_pestanas_compacta_en_estrecho() {
    let normal = en_sesion(100, 30, Tema::respaldo(), tres_pestanas);
    assert_eq!(
        barra_pestanas(&normal),
        "▸● hetzner-01 │ ◐ mac-mini-m1 │ ✕ rincon-dev │ + nueva"
    );
    for (cols, filas) in [(99, 30), (80, 24), (40, 12)] {
        let estrecha = en_sesion(cols, filas, Tema::respaldo(), tres_pestanas);
        assert_eq!(
            barra_pestanas(&estrecha),
            "▸● 1 hetzner-01 │ ◐ 2 │ ✕ 3 │ +",
            "{cols}×{filas}"
        );
    }
    // Con otra activa: la marca y el nombre pasan a ella.
    let mut prueba = en_sesion(80, 24, Tema::respaldo(), tres_pestanas);
    prefijo(&mut prueba, '2');
    assert_eq!(barra_pestanas(&prueba), " ● 1 │ ▸● 2 mac-mini-m1 │ ✕ 3 │ +");
}

/// Doce pestañas con la 11 activa: la activa siempre se ve con su marca; lo
/// que no cabe queda tras `‹ ›` y la pista de la lista aparece si cabe.
#[test]
fn sesion_con_doce_pestanas() {
    for (cols, filas) in TAMANOS {
        let prueba = en_sesion_doce(cols, filas, Tema::respaldo());
        prueba.instantanea(&format!("sesion_12_pestanas_{cols}x{filas}"));
        let barra = barra_pestanas(&prueba);
        assert!(barra.contains("▸●"), "{cols}×{filas}: {barra}");
        assert!(barra.contains('‹'), "{cols}×{filas}: {barra}");
        assert!(
            barra.chars().count() <= usize::from(cols),
            "{cols}×{filas}: {barra}"
        );
        if cols >= 100 {
            // Forma completa: nueve pestañas con nombre alrededor de la activa.
            assert!(barra.contains("▸● mac-mini-m1 (…"), "{barra}");
            assert_eq!(barra.matches(" │ ").count(), 9, "{barra}");
            assert!(barra.contains("12 sesiones: usa ‹ › o la lista"), "{barra}");
        } else {
            assert!(barra.contains("▸● 11 mac-mini-m1 (…"), "{barra}");
            assert!(
                barra.ends_with("│ +") || barra.contains("sesiones"),
                "{barra}"
            );
        }
    }
    // A 80 columnas la ventana llega hasta la última: `‹` sin `›`.
    let prueba = en_sesion_doce(80, 24, Tema::respaldo());
    let barra = barra_pestanas(&prueba);
    assert_eq!(
        barra,
        "‹ ● 4 │ ◐ 5 │ ● 6 │ ● 7 │ ◐ 8 │ ● 9 │ ● 10 │ ▸● 11 mac-mini-m1 (… │ ● 12 │ +"
    );
    // Con la primera activa, al revés: `›` sin `‹`.
    let mut prueba = en_sesion_doce(80, 24, Tema::respaldo());
    prefijo(&mut prueba, '1');
    let barra = barra_pestanas(&prueba);
    assert!(barra.starts_with("▸● 1 hetzner-01 │ "), "{barra}");
    assert!(barra.ends_with(" › │ +") && !barra.contains('‹'), "{barra}");
}

/// Relleno: el remoto es menor (otra ventana impone el mínimo); lo que sobra
/// lleva `░` y la barra mantiene «c×f (mín. ventana N)» aunque quite el resto.
#[test]
fn sesion_relleno() {
    let prueba = con_relleno((80, 24), (60, 14), Tema::respaldo());
    prueba.instantanea("sesion_relleno");
    let texto = prueba.texto();
    let pintadas: Vec<&str> = texto.lines().collect();
    // Fila remota: 60 columnas de terminal y 20 de relleno.
    assert!(pintadas[2].starts_with("root@hetzner-01:~# htop"));
    assert!(pintadas[2].ends_with(&"░".repeat(20)), "{}", pintadas[2]);
    // Por debajo de las 14 filas remotas, todo relleno.
    assert_eq!(pintadas[2 + 14], "░".repeat(80));
    assert!(barra_estado(&prueba).contains("60×14 (mín. ventana 2)"));

    // A 40×12, estrecho: el indicador se queda aunque se vayan host y tiempo.
    let prueba = con_relleno((40, 12), (30, 5), Tema::respaldo());
    prueba.instantanea("sesion_relleno_40x12");
    let estado = barra_estado(&prueba);
    assert_eq!(estado, " ● 1/3 · 30×5 (mín. ventana 2) · ^] ?");

    // Si el mínimo lo impone esta ventana (aún sin aplicar), lo dice.
    let prueba = con_relleno((80, 24), (60, 14), Tema::respaldo());
    let mut prueba = prueba;
    remoto(&mut prueba, 60, 14, Some(1));
    assert!(barra_estado(&prueba).contains("60×14 (mín. esta ventana)"));

    // Sin relleno, el tamaño remoto es lo primero que se quita.
    let prueba = en_sesion(80, 24, Tema::respaldo(), tres_pestanas);
    assert!(!prueba.texto().contains('░'));
    assert!(!barra_estado(&prueba).contains("80×20"));
    let prueba = en_sesion(200, 60, Tema::respaldo(), tres_pestanas);
    assert!(barra_estado(&prueba).contains(" · 200×56 · "));
}

/// Barra de estado por prioridad: glifo · posición · host · tiempo ·
/// identidad · carga · ventanas; la ayuda siempre al final.
#[test]
fn barra_de_estado_por_prioridad() {
    let prueba = en_sesion(200, 60, Tema::respaldo(), tres_pestanas);
    assert_eq!(
        barra_estado(&prueba),
        " ● 1/3 · hetzner-01 · conectado 18 m · ed25519 (agente) · 200×56 · carga 0.4 · \
         2 ventanas · Ctrl+] ? ayuda"
    );
    let orden = [
        "1/3",
        "hetzner-01",
        "18 m",
        "ed25519 (agente)",
        "carga 0.4",
        "2 ventanas",
    ];
    for cols in [40u16, 50, 60, 70, 80, 90, 99, 100, 120] {
        let prueba = en_sesion(cols, 24, Tema::respaldo(), tres_pestanas);
        let estado = barra_estado(&prueba);
        assert!(estado.chars().count() <= usize::from(cols), "{estado}");
        assert!(estado.starts_with(" ● "), "{cols}: {estado}");
        assert!(
            estado.ends_with("^] ?") || estado.ends_with("Ctrl+] ? ayuda"),
            "{cols}: {estado}"
        );
        // Si se ve un elemento, se ven todos los de más prioridad.
        let vistos: Vec<bool> = orden
            .iter()
            .map(|elemento| estado.contains(elemento))
            .collect();
        let primero_oculto = vistos.iter().position(|visto| !visto);
        if let Some(oculto) = primero_oculto {
            assert!(
                vistos[oculto..].iter().all(|visto| !visto),
                "{cols}: {estado}"
            );
        }
        if cols < 100 {
            assert!(
                !estado.contains("conectado") && estado.ends_with("^] ?"),
                "{estado}"
            );
        }
    }
}

/// Pestaña caída: la última pantalla en gris y el aviso, que en áreas bajas
/// quita líneas en blanco y abrevia las acciones sin salirse del área.
#[test]
fn sesion_caida() {
    for (cols, filas) in [(40, 12), (80, 24)] {
        let prueba = en_sesion_caida(cols, filas, Tema::respaldo());
        prueba.instantanea(&format!("sesion_caida_{cols}x{filas}"));
        let texto = prueba.texto();
        assert!(texto.contains("Sesión caída hace 2 h 00 m"), "{texto}");
        assert!(
            texto.contains("reconectar") && texto.contains("cerrar"),
            "{texto}"
        );
        assert!(texto.contains("conexión cerrada por el remoto"), "{texto}");
    }
    // En el mínimo de Sesión (4 filas de contenido) quedan el título y las
    // acciones.
    let prueba = en_sesion_caida(40, 8, Tema::respaldo());
    let texto = prueba.texto();
    assert!(texto.contains("Sesión caída"), "{texto}");
    assert!(texto.contains("^] r reconectar  ^] x cerrar"), "{texto}");
}

/// Modo prefijo: la barra dice qué hace cada tecla y se compacta por
/// prioridad sin cortar ninguna a medias.
#[test]
fn sesion_modo_prefijo() {
    let mut prueba = en_sesion(200, 60, Tema::respaldo(), tres_pestanas);
    prueba.tecla_con(KeyCode::Char(']'), KeyModifiers::CONTROL);
    assert_eq!(
        barra_estado(&prueba),
        " [MAGI] Ctrl+]  1-9 n p pestañas · l lista · c conectar · x cerrar · r reconectar · \
         w ventana · q volver · prefijo de nuevo = literal"
    );
    for (cols, filas) in [(40, 12), (80, 24)] {
        let mut prueba = en_sesion(cols, filas, Tema::respaldo(), tres_pestanas);
        prueba.tecla_con(KeyCode::Char(']'), KeyModifiers::CONTROL);
        prueba.instantanea(&format!("sesion_prefijo_{cols}x{filas}"));
        let estado = barra_estado(&prueba);
        assert!(
            estado.starts_with(" [MAGI] ^]  1-9 n p pestañas"),
            "{estado}"
        );
        assert!(estado.contains("q volver"), "{estado}");
        let ultimo = estado.rsplit(" · ").next().unwrap();
        assert!(
            [
                "l lista",
                "c conectar",
                "x cerrar",
                "r reconectar",
                "w ventana",
                "q volver",
                "prefijo de nuevo = literal"
            ]
            .contains(&ultimo),
            "cortado a medias: {estado}"
        );
    }
}

/// Modo ASCII: ningún glifo Unicode en ningún tamaño ni estado; el relleno es
/// `.` y el tamaño usa `x`.
#[test]
fn sesion_en_ascii() {
    let mut pintados: Vec<(String, String)> = Vec::new();
    for (cols, filas) in [(40, 12), (80, 24), (200, 60), (60, 16), (40, 8)] {
        let prueba = en_sesion(cols, filas, arnes::tema_ascii(), tres_pestanas);
        pintados.push((format!("{cols}×{filas}"), prueba.texto()));
        let prueba = en_sesion_doce(cols, filas, arnes::tema_ascii());
        pintados.push((format!("12 pestañas {cols}×{filas}"), prueba.texto()));
        let prueba = en_sesion_caida(cols, filas, arnes::tema_ascii());
        pintados.push((format!("caída {cols}×{filas}"), prueba.texto()));
        let mut prueba = en_sesion(cols, filas, arnes::tema_ascii(), tres_pestanas);
        prueba.tecla_con(KeyCode::Char(']'), KeyModifiers::CONTROL);
        pintados.push((format!("prefijo {cols}×{filas}"), prueba.texto()));
    }
    let prueba = con_relleno((80, 24), (60, 14), arnes::tema_ascii());
    let relleno = prueba.texto();
    assert!(relleno.contains(&".".repeat(20)), "{relleno}");
    assert!(barra_estado(&prueba).contains("60x14 (mín. ventana 2)"));
    prueba.instantanea("sesion_relleno_ascii");
    pintados.push(("relleno".to_string(), relleno));
    let prueba = con_relleno((40, 12), (30, 5), arnes::tema_ascii());
    pintados.push(("relleno 40×12".to_string(), prueba.texto()));
    let mut prueba = en_sesion(40, 8, arnes::tema_ascii(), tres_pestanas);
    prueba.redimensionar(39, 7);
    pintados.push(("aviso".to_string(), prueba.texto()));
    for (caso, texto) in pintados {
        assert!(
            sin_glifos_unicode(&texto),
            "glifo Unicode en {caso}:\n{texto}"
        );
    }
}

/// Secuencia 200×60 → 80×24 → 40×12 → 200×60 con doce pestañas y el prefijo
/// a medias: la pestaña activa, su pantalla y el prefijo se conservan, la
/// activa siempre se ve en la barra y cada tamaño llega al remoto.
#[test]
fn la_secuencia_de_tamanos_conserva_el_estado() {
    let mut prueba = en_sesion_doce(200, 60, Tema::respaldo());
    prueba.tecla_con(KeyCode::Char(']'), KeyModifiers::CONTROL);
    prueba.enviados();
    for (paso, (cols, filas)) in [(200, 60), (80, 24), (40, 12), (200, 60)]
        .into_iter()
        .enumerate()
    {
        prueba.pasar_por(cols, filas);
        // El primer paso repite el tamaño aplicado: no envía nada. Los demás,
        // incluida la vuelta a 200×60, envían exactamente un `Redimensionar`.
        let esperado = if paso == 0 {
            Vec::new()
        } else {
            vec![(11, cols, alto_pty(filas))]
        };
        assert_eq!(prueba.redimensionares(), esperado, "{cols}×{filas}");
        assert_eq!(prueba.app.pestana_activa, Some(10));
        assert!(prueba.app.modo_prefijo, "el prefijo sigue pendiente");
        let barra = barra_pestanas(&prueba);
        assert!(
            barra.contains("▸● mac-mini-m1") || barra.contains("▸● 11 "),
            "{cols}×{filas}: {barra}"
        );
        assert!(barra.chars().count() <= usize::from(cols), "{barra}");
        assert!(prueba.texto().contains("root@hetzner-01:~# htop"));
        assert!(barra_estado(&prueba).starts_with(" [MAGI] "));
    }
    // El prefijo pendiente sigue valiendo: `n` pasa a la pestaña 12.
    prueba.tecla(KeyCode::Char('n'));
    assert_eq!(prueba.app.pestana_activa, Some(11));
    assert!(!prueba.app.modo_prefijo);
}

/// Regresión: la vista no entra en pánico en ningún área, por diminuta que
/// sea (restas e índices con anchos y altos de 0 a 3), ni pegada al borde de
/// un búfer mayor. `ui::dibujar` nunca la pinta por debajo de 40×8, pero la
/// vista no debe depender de ello.
#[test]
fn sesion_sin_panico_en_areas_diminutas() {
    use magi::ui::disposicion::{minimo_de, Disposicion};
    use magi::ui::Vista;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::Terminal;

    let mut prefijo_pendiente = en_sesion(80, 24, Tema::respaldo(), tres_pestanas);
    prefijo_pendiente.tecla_con(KeyCode::Char(']'), KeyModifiers::CONTROL);
    let estados = [
        en_sesion(80, 24, Tema::respaldo(), tres_pestanas),
        en_sesion_doce(80, 24, Tema::respaldo()),
        en_sesion_caida(80, 24, Tema::respaldo()),
        con_relleno((80, 24), (60, 14), Tema::respaldo()),
        en_sesion_caida(80, 24, arnes::tema_ascii()),
        prefijo_pendiente,
    ];
    let anchos = [0u16, 1, 2, 3, 4, 7, 12, 20, 39, 40];
    let altos = [0u16, 1, 2, 3, 4, 5, 6, 7, 8];
    for prueba in &estados {
        for cols in anchos {
            for filas in altos {
                let mut terminal =
                    Terminal::new(TestBackend::new(cols + 3, filas + 2)).expect("terminal");
                terminal
                    .draw(|marco| {
                        let total = marco.area();
                        let area = Rect {
                            x: total.width - cols,
                            y: total.height - filas,
                            width: cols,
                            height: filas,
                        };
                        let mut disp = Disposicion::nueva(area, minimo_de(Vista::Sesion, false));
                        magi::ui::sesion::dibujar(marco, area, &prueba.app, &mut disp);
                    })
                    .expect("pintado");
            }
        }
    }
}

/// Entre el mínimo y más de 200 columnas ninguna barra se corta a medias: el
/// título llena el ancho y acaba en trazo, la de pestañas acaba en `+`,
/// `+ nueva` o la pista y siempre lleva la activa, y la de estado acaba en la
/// ayuda.
#[test]
fn las_barras_de_sesion_nunca_se_cortan() {
    let casos = [
        ("tres", en_sesion(80, 24, Tema::respaldo(), tres_pestanas)),
        ("doce", en_sesion_doce(80, 24, Tema::respaldo())),
        ("caída", en_sesion_caida(80, 24, Tema::respaldo())),
        ("relleno", con_relleno((80, 24), (60, 14), Tema::respaldo())),
        ("doce ascii", en_sesion_doce(80, 24, arnes::tema_ascii())),
    ];
    let anchos: Vec<u16> = (40..=130).chain([150, 199, 200, 210]).collect();
    for (caso, mut prueba) in casos {
        for filas in [8u16, 24] {
            for &cols in &anchos {
                prueba.redimensionar(cols, filas);
                let texto = prueba.texto();
                let lineas: Vec<&str> = texto.lines().collect();
                let (titulo, pestanas) = (lineas[0], lineas[1]);
                let estado = lineas[usize::from(filas) - 2];
                let donde = format!("{caso} {cols}×{filas}:\n{texto}");
                assert_eq!(titulo.chars().count(), usize::from(cols), "{donde}");
                assert!(titulo.ends_with('─') || titulo.ends_with('-'), "{donde}");
                assert!(
                    pestanas.ends_with('+')
                        || pestanas.ends_with("+ nueva")
                        || pestanas.ends_with("sesiones")
                        || pestanas.ends_with("la lista"),
                    "{donde}"
                );
                // La activa lleva `▸` (`>` en ASCII, que también es `›`: se
                // busca junto a su glifo).
                assert!(pestanas.contains('▸') || pestanas.contains(">*"), "{donde}");
                assert!(
                    estado.ends_with("^] ?") || estado.ends_with("Ctrl+] ? ayuda"),
                    "{donde}"
                );
            }
        }
    }
}
