//! Fase 17 (Termux), grupo P: los cinco diálogos del llavero con frase maestra
//! (LLAVERO DE MAGI, CREAR LA FRASE MAESTRA, CAMBIAR LA FRASE MAESTRA, BORRAR
//! EL LLAVERO y LLAVERO DAÑADO) a 40×12, 80×24, 200×60 y 48×20 (un móvil en
//! vertical, T45), y Flota, Hosts y Sesión a 48×20.
//!
//! Los diálogos se abren ya hechos (`abrir_dialogo`): lo que tiene su camino
//! de teclas y su llavero real se prueba en `tests/termux_ui.rs`. Nada de aquí
//! gasta scrypt ni toca un llavero.

use crossterm::event::KeyCode;

use magi::app::llavero::{DialogoLlavero, ModoLlavero};
use magi::app::{Dialogo, Evento};
use magi::tema::Tema;
use magi::ui::componentes::CampoTexto;
use magi::ui::disposicion::{ESTRECHO_LLAVERO, MINIMO_LLAVERO};
use magi::ui::Vista;

use crate::arnes::{self, AppPrueba};
use crate::vistas_g::FabricaDialogo;

/// Los tres tamaños de la fase y el de un móvil en vertical.
const TAMANOS: [(u16, u16); 4] = [(40, 12), (80, 24), (200, 60), (48, 20)];

/// Una frase de mentira con la longitud justa para ver la máscara.
const FRASE: &str = "frase-de-prueba!";

fn campo(texto: &str) -> CampoTexto {
    CampoTexto::nuevo(texto)
}

/// El diálogo de un modo con algo escrito (para ver la máscara y el foco).
pub(crate) fn dialogo_de(modo: ModoLlavero) -> Dialogo {
    let mut dialogo = match modo {
        ModoLlavero::Danado => DialogoLlavero::danado(
            "la cabecera de age no se puede leer: no es un fichero age".to_string(),
        ),
        otro => DialogoLlavero::nuevo(otro),
    };
    match modo {
        ModoLlavero::Desbloquear => dialogo.campos[0] = campo(FRASE),
        ModoLlavero::Crear => {
            dialogo.campos[0] = campo(FRASE);
            dialogo.campos[1] = campo("frase-de-pru");
            dialogo.foco = 1;
        }
        ModoLlavero::Cambiar => {
            dialogo.campos[0] = campo("la-frase-actual");
            dialogo.campos[1] = campo(FRASE);
            dialogo.foco = 1;
        }
        ModoLlavero::Borrar => dialogo.campos[0] = campo("borra"),
        ModoLlavero::Danado => dialogo.al_arrancar = true,
    }
    Dialogo::Llavero(Box::new(dialogo))
}

const MODOS: [(ModoLlavero, &str); 5] = [
    (ModoLlavero::Desbloquear, "desbloquear"),
    (ModoLlavero::Crear, "crear"),
    (ModoLlavero::Cambiar, "cambiar"),
    (ModoLlavero::Borrar, "borrar"),
    (ModoLlavero::Danado, "danado"),
];

/// Fábricas de los cinco diálogos para el barrido y la red de seguridad de
/// `vistas_g` (sin pánico en áreas diminutas, enteros a 40×12, ASCII).
pub(crate) fn dialogos() -> Vec<(&'static str, FabricaDialogo)> {
    vec![
        (
            "LLAVERO DE MAGI",
            Box::new(|| dialogo_de(ModoLlavero::Desbloquear)),
        ),
        (
            "CREAR LA FRASE MAESTRA",
            Box::new(|| dialogo_de(ModoLlavero::Crear)),
        ),
        (
            "CAMBIAR LA FRASE MAESTRA",
            Box::new(|| dialogo_de(ModoLlavero::Cambiar)),
        ),
        (
            "BORRAR EL LLAVERO",
            Box::new(|| dialogo_de(ModoLlavero::Borrar)),
        ),
        (
            "LLAVERO DAÑADO",
            Box::new(|| dialogo_de(ModoLlavero::Danado)),
        ),
    ]
}

fn abrir(prueba: &mut AppPrueba, dialogo: Dialogo) {
    prueba.app.abrir_dialogo(dialogo);
    prueba.paso_en(std::time::Instant::now());
}

/// El texto pintado de un recuadro, de su esquina de arriba a la de abajo.
fn recuadro(texto: &str, titulo: &str, ascii: bool) -> Vec<String> {
    let (izquierda, derecha, abajo) = if ascii {
        ('+', '+', '+')
    } else {
        ('┌', '┐', '└')
    };
    let lineas: Vec<&str> = texto.lines().collect();
    let marca = format!("{izquierda} {titulo}");
    let Some(y0) = lineas.iter().position(|linea| linea.contains(&marca)) else {
        panic!("sin recuadro «{titulo}»:\n{texto}");
    };
    // Columnas (no bytes) de las esquinas del diálogo, no las del marco de
    // la vista que hay detrás.
    let byte_izquierda = lineas[y0].find(&marca).expect("esquina izquierda");
    let x0 = lineas[y0][..byte_izquierda].chars().count();
    let byte_derecha = byte_izquierda
        + lineas[y0][byte_izquierda..]
            .find(derecha)
            .expect("esquina derecha");
    let x1 = lineas[y0][..byte_derecha].chars().count();
    let mut salida = Vec::new();
    for linea in &lineas[y0..] {
        let caracteres: Vec<char> = linea.chars().collect();
        if caracteres.len() <= x0 {
            break;
        }
        let fin = (x1 + 1).min(caracteres.len());
        salida.push(caracteres[x0..fin].iter().collect::<String>());
        if caracteres[x0] == abajo {
            break;
        }
    }
    salida
}

// ---------------------------------------------------------------- instantáneas

#[test]
fn instantaneas_de_los_cinco_dialogos() {
    for (modo, nombre) in MODOS {
        for (cols, filas) in TAMANOS {
            let (mut prueba, _) = AppPrueba::con_semilla(cols, filas);
            abrir(&mut prueba, dialogo_de(modo));
            prueba.instantanea(&format!("llavero_{nombre}_{cols}x{filas}"));
        }
    }
}

#[test]
fn instantaneas_de_los_estados_del_dialogo() {
    for (cols, filas) in [(80, 24), (48, 20)] {
        // Con scrypt en marcha: «trabajando…» y «espera…», sin teclas.
        let (mut prueba, _) = AppPrueba::con_semilla(cols, filas);
        let mut dialogo = DialogoLlavero::nuevo(ModoLlavero::Desbloquear);
        dialogo.trabajando = Some(1);
        abrir(&mut prueba, Dialogo::Llavero(Box::new(dialogo)));
        prueba.instantanea(&format!("llavero_trabajando_{cols}x{filas}"));

        // Frase incorrecta.
        let (mut prueba, _) = AppPrueba::con_semilla(cols, filas);
        let mut dialogo = DialogoLlavero::nuevo(ModoLlavero::Desbloquear);
        dialogo.error = Some("frase incorrecta".to_string());
        abrir(&mut prueba, Dialogo::Llavero(Box::new(dialogo)));
        prueba.instantanea(&format!("llavero_incorrecta_{cols}x{filas}"));

        // Las frases no coinciden al crear.
        let (mut prueba, _) = AppPrueba::con_semilla(cols, filas);
        let mut dialogo = DialogoLlavero::nuevo(ModoLlavero::Crear);
        dialogo.campos[0] = campo(FRASE);
        dialogo.campos[1] = campo("otra-frase-distinta");
        dialogo.error = Some("las frases no coinciden".to_string());
        abrir(&mut prueba, Dialogo::Llavero(Box::new(dialogo)));
        prueba.instantanea(&format!("llavero_crear_error_{cols}x{filas}"));
    }
}

// ---------------------------------------------------------------- las tres vistas a 48×20

#[test]
fn flota_hosts_y_sesion_a_48x20() {
    let (mut prueba, _) = AppPrueba::con_semilla(48, 20);
    assert_eq!(prueba.app.vista, Vista::Flota);
    prueba.instantanea("flota_48x20");

    prueba.tecla(KeyCode::F(2));
    assert_eq!(prueba.app.vista, Vista::Hosts);
    prueba.instantanea("hosts_48x20");

    // Sesión: se entra a 80×24 (la lista Sesiones necesita 40×12) y luego se
    // pasa a 48×20 con el remoto a su tamaño, como en `vistas_c`.
    let (mut prueba, sembrado) = AppPrueba::con_semilla(80, 24);
    let mut sesion = arnes::sesion(1, "hetzner-01");
    sesion.host_id = sembrado.host("hetzner-01");
    sesion.abierta_en = chrono::Utc::now().timestamp() - 18 * 60 - 30;
    prueba.entrar_en_sesion(vec![sesion]);
    prueba.redimensionar(48, 20);
    let (_, rect) = prueba.app.disposicion().panes[0];
    let (cols, filas) = (rect.width, rect.height);
    prueba.servidor(magi::protocolo::MensajeServidor::Redimensionada {
        sesion_id: 1,
        cols,
        filas,
        ventana_minima: Some(1),
    });
    prueba.app.pantallas.volcar(
        1,
        filas,
        cols,
        format!("\x1b[H\x1b[2Jusuario@hetzner-01:~$ htop\r\n  (htop a {cols}x{filas})\r\n")
            .as_bytes(),
    );
    prueba.evento(Evento::Pantallas(vec![1]));
    assert_eq!(prueba.app.vista, Vista::Sesion);
    prueba.instantanea("sesion_48x20");
}

// ---------------------------------------------------------------- comprobaciones de disposición

/// Los cinco diálogos se ven enteros (recuadro cerrado) a los cuatro tamaños,
/// con el tema Unicode y con el ASCII, y en ASCII no pintan ningún glifo
/// fuera de él.
#[test]
fn los_cinco_dialogos_se_ven_enteros_y_en_ascii() {
    for (modo, _) in MODOS {
        let titulo = magi::ui::llavero::titulo(modo);
        for (cols, filas) in TAMANOS {
            let (mut prueba, _) = AppPrueba::con_semilla(cols, filas);
            abrir(&mut prueba, dialogo_de(modo));
            let texto = prueba.texto();
            let recuadro = recuadro(&texto, titulo, false);
            assert!(
                recuadro.last().is_some_and(|linea| linea.starts_with('└')),
                "«{titulo}» cortado a {cols}×{filas}:\n{texto}"
            );
            // Lo escrito nunca sale en claro.
            assert!(
                !texto.contains(FRASE),
                "la frase se ve en claro a {cols}×{filas}:\n{texto}"
            );

            let (mut prueba, _) = AppPrueba::con_semilla_y_tema(cols, filas, arnes::tema_ascii());
            abrir(&mut prueba, dialogo_de(modo));
            let texto = prueba.texto();
            let dentro = recuadro_ascii(&texto, titulo);
            let malos: String = dentro
                .chars()
                .filter(|c| !(c.is_ascii() || c.is_alphabetic()))
                .collect();
            assert!(
                malos.is_empty(),
                "glifos {malos:?} en «{titulo}» a {cols}×{filas}:\n{dentro}"
            );
        }
    }
}

fn recuadro_ascii(texto: &str, titulo: &str) -> String {
    recuadro(texto, titulo, true).join("\n")
}

/// El mínimo del diálogo es el de la fase y por encima de él el diálogo se ve
/// entero con sus teclas; por debajo entra el aviso de tamaño.
#[test]
fn el_minimo_del_dialogo_es_40x12() {
    assert_eq!((MINIMO_LLAVERO.cols, MINIMO_LLAVERO.filas), (40, 12));
    // Un móvil en vertical (48 columnas) no está en modo estrecho; 40 sí.
    const { assert!(48 >= ESTRECHO_LLAVERO && 40 < ESTRECHO_LLAVERO) };
    for (modo, _) in MODOS {
        let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
        abrir(&mut prueba, dialogo_de(modo));
        let minimo = magi::ui::disposicion::minimo_dialogo(
            prueba.app.dialogo.as_ref().expect("diálogo abierto"),
        )
        .expect("el diálogo declara su mínimo");
        assert_eq!(minimo.tamano, MINIMO_LLAVERO);
        assert_eq!(minimo.exige, magi::ui::llavero::titulo(modo));
    }
}

/// A 48×20 se leen las teclas del pie (T45): lo que hay que pulsar siempre
/// se ve, también con el cuerpo desplazado.
#[test]
fn el_pie_con_las_teclas_se_ve_a_48x20_y_a_40x12() {
    for (cols, filas) in [(48, 20), (40, 12)] {
        for (modo, esperado) in [
            (ModoLlavero::Desbloquear, "desbloquear"),
            (ModoLlavero::Crear, "crear"),
            (ModoLlavero::Cambiar, "cambiar"),
            (ModoLlavero::Borrar, "borrar"),
            (ModoLlavero::Danado, "apartar"),
        ] {
            let (mut prueba, _) = AppPrueba::con_semilla(cols, filas);
            abrir(&mut prueba, dialogo_de(modo));
            let texto = prueba.texto();
            assert!(
                texto.contains(esperado) && texto.contains("Esc"),
                "{modo:?} a {cols}×{filas} sin sus teclas:\n{texto}"
            );
        }
    }
}

// ---------------------------------------------------------------- mensajes largos de la barra

/// Lo que dice la barra de un Termux vertical de 48 columnas cuando el mensaje
/// es de los largos de §4.4 (el portapapeles) o la explicación de `prefijo w`
/// (§5.4): miden entre 70 y 95 caracteres y la barra, de una sola línea, los
/// recorta con «…». El texto es el de la especificación y no se acorta; esta
/// instantánea documenta lo que se ve. Variantes cortas, si el analista las
/// quiere, son una petición abierta del informe de implementación.
#[test]
fn la_barra_recorta_a_48x20_los_mensajes_largos_de_termux() {
    let casos: [(&str, String, bool); 4] = [
        (
            "no_cabe",
            magi::termux::portapapeles::mensaje_no_cabe(20 * 1024),
            true,
        ),
        (
            "demasiado",
            magi::termux::portapapeles::mensaje_demasiado(900 * 1024),
            true,
        ),
        (
            "api_apagada",
            magi::termux::portapapeles::AVISO_API_APAGADA.to_string(),
            true,
        ),
        (
            "explicacion_w",
            magi::cliente::ventana::EXPLICACION_TERMUX.to_string(),
            false,
        ),
    ];
    for (nombre, texto, error) in casos {
        let largo = texto.chars().count();
        assert!(
            (60..=100).contains(&largo),
            "«{nombre}» mide {largo}: ya no es de los largos"
        );
        // Se pone el mensaje un poco más ancho y se pasa a 48×20: poner un
        // mensaje no marca la pantalla sucia y el cambio de tamaño repinta.
        let (mut prueba, _) = AppPrueba::con_semilla(49, 20);
        prueba.app.mensaje(texto, error);
        prueba.redimensionar(48, 20);
        let texto_pintado = prueba.texto();
        let barra = texto_pintado.lines().last().expect("barra").to_string();
        assert_eq!(barra.chars().count(), 48, "{barra}");
        assert!(
            barra.trim_end().ends_with('…'),
            "el mensaje largo se recorta con marca: {barra}"
        );
        prueba.instantanea(&format!("barra_mensaje_{nombre}_48x20"));
    }
}

/// El tema por defecto no se toca: los diálogos son los de siempre en los
/// demás tamaños y no cambian el aviso de tamaño.
#[test]
fn los_dialogos_no_cambian_el_aviso_de_tamano() {
    let (mut prueba, _) = AppPrueba::con_semilla_y_tema(80, 24, Tema::respaldo());
    abrir(&mut prueba, dialogo_de(ModoLlavero::Desbloquear));
    prueba.pasar_por(48, 20);
    assert!(!prueba.app.disposicion().aviso);
    prueba.pasar_por(40, 12);
    assert!(!prueba.app.disposicion().aviso);
}
