//! Instantáneas y pruebas de disposición del grupo E: Túneles, Registro,
//! Identidades y Snippets (tabla con detalle inferior).
//!
//! Lo que se comprueba: columnas por prioridad (el identificador y el glifo
//! de estado no se ocultan), host abreviado en Túneles en modo estrecho,
//! detalle inferior plegado con la vista baja (se abre con `↵`, o con `i` en
//! Snippets), contenido centrado con ancho máximo en ventanas muy grandes,
//! modo ASCII sin glifos y estado conservado al cambiar de tamaño.

use crossterm::event::KeyCode;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

use magi::almacen::{snippets, tuneles, Almacen};
use magi::app::Dialogo;
use magi::modelo::{DatosTunel, ResultadoRegistro, TipoTunel};
use magi::protocolo::{EstadoTunelRemoto, InfoTunel, MensajeServidor, OrigenTunel};
use magi::snippets::{DatosSnippet, Destino};
use magi::ui::disposicion::{Disposicion, Lista, Minimo, ModoAncho, Tamano};
use magi::ui::Vista;

use crate::arnes::{self, AppPrueba};
use crate::semilla::Sembrado;

/// Las vistas del grupo: nombre de las instantáneas, tecla `F` y vista.
const VISTAS: [(&str, u8, Vista); 4] = [
    ("tuneles", 6, Vista::Tuneles),
    ("registro", 7, Vista::Registro),
    ("identidades", 5, Vista::Identidades),
    ("snippets", 8, Vista::Snippets),
];

/// Los tres tamaños de la fase, la vista baja y estrecha (90×18) y el mínimo
/// de estas vistas (50×12).
const TAMANOS: [(u16, u16); 5] = [(40, 12), (80, 24), (200, 60), (90, 18), (50, 12)];

/// Estado en vivo de los túneles sembrados: `postgres` activo, `grafana`
/// caído con su error, `webhook` activándose y `socks` inactivo (el servidor
/// no lo tiene). Sin `desde`, para que nada dependa de la hora.
fn difusion_tuneles(sembrado: &Sembrado) -> MensajeServidor {
    let info = |indice: usize, host: &str, nombre: &str, estado: EstadoTunelRemoto| InfoTunel {
        tunel_id: sembrado.tuneles[indice],
        host_id: sembrado.host(host),
        host_nombre: host.to_string(),
        nombre: nombre.to_string(),
        tipo: "local".to_string(),
        escucha: String::new(),
        escucha_efectiva: String::new(),
        destino: None,
        automatico: false,
        estado,
        origen: Some(OrigenTunel::Manual),
        solicitante: Some(1),
        conexiones: 0,
        aceptadas: 0,
        bytes_subidos: 0,
        bytes_bajados: 0,
        desde: None,
        ultimo_error: None,
    };
    let mut postgres = info(0, "hetzner-01", "postgres", EstadoTunelRemoto::Activo);
    postgres.escucha = "127.0.0.1:5432".to_string();
    postgres.escucha_efectiva = "127.0.0.1:5432".to_string();
    postgres.destino = Some("127.0.0.1:5432".to_string());
    postgres.automatico = true;
    postgres.origen = Some(OrigenTunel::Automatico);
    postgres.conexiones = 1;
    postgres.aceptadas = 3;
    postgres.bytes_bajados = 14_200_000;
    postgres.bytes_subidos = 890_000;
    let mut grafana = info(1, "hetzner-02", "grafana", EstadoTunelRemoto::Caido);
    grafana.escucha = "127.0.0.1:3000".to_string();
    grafana.escucha_efectiva = "127.0.0.1:3000".to_string();
    grafana.destino = Some("10.0.1.50:3000".to_string());
    grafana.ultimo_error = Some("connect: conexión rechazada por 10.0.1.50:3000".to_string());
    let mut webhook = info(3, "mac-mini-m1", "webhook", EstadoTunelRemoto::Activando);
    webhook.tipo = "remoto".to_string();
    webhook.escucha = "0.0.0.0:8080".to_string();
    webhook.escucha_efectiva = "0.0.0.0:8080".to_string();
    webhook.destino = Some("127.0.0.1:8080".to_string());
    MensajeServidor::Tuneles {
        lista: vec![postgres, grafana, webhook],
    }
}

/// Abre la vista con la semilla. En Túneles llega además la difusión con el
/// estado en vivo y se selecciona `grafana` (caído), cuyo detalle es el más
/// completo.
fn abrir(tecla: u8, cols: u16, filas: u16, tema: magi::tema::Tema) -> AppPrueba {
    let (mut prueba, sembrado) = AppPrueba::con_semilla_y_tema(cols, filas, tema);
    prueba.tecla(KeyCode::F(tecla));
    if tecla == 6 {
        prueba.servidor(difusion_tuneles(&sembrado));
        prueba.tecla(KeyCode::Down);
    }
    prueba
}

fn abrir_unicode(tecla: u8, cols: u16, filas: u16) -> AppPrueba {
    abrir(tecla, cols, filas, magi::tema::Tema::respaldo())
}

/// Líneas pintadas.
fn lineas(prueba: &AppPrueba) -> Vec<String> {
    prueba.texto().lines().map(str::to_string).collect()
}

fn solo_ascii(texto: &str) -> bool {
    texto.chars().all(|c| c.is_ascii() || c.is_alphabetic())
}

/// La tecla que abre el detalle de la vista: `↵`, o `i` en Snippets (`↵`
/// ejecuta).
fn tecla_detalle(vista: Vista) -> KeyCode {
    if vista == Vista::Snippets {
        KeyCode::Char('i')
    } else {
        KeyCode::Enter
    }
}

/// Algo que solo sale en el panel de detalle inferior de cada vista.
fn marca_del_detalle(vista: Vista) -> &'static str {
    match vista {
        Vista::Tuneles => "hetzner-02 · grafana",
        Vista::Registro => "— · 15/09 15:41:02 · ok",
        Vista::Identidades => "Huella",
        Vista::Snippets => "Confirmación",
        _ => unreachable!(),
    }
}

#[test]
fn instantaneas_de_las_vistas() {
    for (nombre, tecla, _) in VISTAS {
        for (cols, filas) in TAMANOS {
            let prueba = abrir_unicode(tecla, cols, filas);
            prueba.instantanea(&format!("{nombre}_{cols}x{filas}"));
        }
    }
}

/// Con la vista baja el detalle inferior está plegado; `↵` (`i` en Snippets)
/// lo abre en diálogo con todo lo del panel.
#[test]
fn instantaneas_del_detalle_abierto_en_vista_baja() {
    for (nombre, tecla, vista) in VISTAS {
        let mut prueba = abrir_unicode(tecla, 90, 18);
        prueba.tecla(tecla_detalle(vista));
        assert!(
            matches!(prueba.app.dialogo, Some(Dialogo::Detalle { .. })),
            "{nombre}: no se abrió el detalle"
        );
        prueba.instantanea(&format!("{nombre}_detalle_90x18"));
    }
}

#[test]
fn el_detalle_se_pliega_con_la_vista_baja_y_se_abre_en_dialogo() {
    for (nombre, tecla, vista) in VISTAS {
        let marca = marca_del_detalle(vista);
        let prueba = abrir_unicode(tecla, 80, 24);
        assert!(!prueba.app.disposicion().bajo);
        assert!(
            prueba.texto().contains(marca),
            "{nombre} a 80x24 debe tener el detalle abierto:\n{}",
            prueba.texto()
        );

        let mut prueba = abrir_unicode(tecla, 90, 18);
        assert!(prueba.app.disposicion().bajo);
        let texto = prueba.texto();
        assert!(
            !texto.contains(marca),
            "{nombre} a 90x18 debe plegar el detalle:\n{texto}"
        );
        let rotulo = if vista == Vista::Snippets {
            "i detalle"
        } else {
            "↵ detalle"
        };
        assert!(
            texto.contains(rotulo),
            "{nombre}: falta «{rotulo}»:\n{texto}"
        );

        prueba.enviados();
        prueba.tecla(tecla_detalle(vista));
        let Some(Dialogo::Detalle { lineas, .. }) = &prueba.app.dialogo else {
            panic!("{nombre}: no se abrió el detalle");
        };
        let todo = lineas.join("\n");
        let esperado: &[&str] = match vista {
            Vista::Tuneles => &[
                "hetzner-02",
                "127.0.0.1:3000 → 10.0.1.50:3000",
                "caído",
                "connect: conexión rechazada",
            ],
            Vista::Registro => &["deliberacion_forzada", "reiniciar nginx → 3 hosts"],
            Vista::Identidades => &[
                "trabajo",
                "ssh-ed25519",
                "fichero",
                "SHA256:Ab3dE5fG7hJ9kL1mN3pQ5rS7tU9vW1xY3zA5bC7dE9",
                "01/09/26 · sin uso",
                "~/.ssh/id_ed25519",
                "no encontrada en el último escaneo",
            ],
            Vista::Snippets => &[
                "web → 3 hosts · CRÍTICO",
                "sudo apt update && sudo apt -y upgrade",
                "etiqueta «web»: hetzner-01, hetzner-02, vps-openclaw",
                "Variables     ninguna",
                "requiere deliberación MAGI (crítico, 3 hosts)",
                "Timeout 60 s · usado 0 veces · última nunca",
            ],
            _ => unreachable!(),
        };
        for parte in esperado {
            assert!(todo.contains(parte), "{nombre}: falta «{parte}» en\n{todo}");
        }
        // `i` en Snippets solo enseña: no lanza nada ni abre EJECUTAR.
        assert!(
            prueba.enviados().is_empty(),
            "{nombre} envió algo al servidor"
        );
        assert!(prueba.app.deliberacion.is_none());

        // Se cierra con esc y la vista sigue igual.
        prueba.tecla(KeyCode::Esc);
        assert!(prueba.app.dialogo.is_none());
        assert_eq!(prueba.app.vista, vista);
    }
}

#[test]
fn columnas_por_prioridad() {
    // Túneles: a 50 columnas cae el tipo; el glifo y el tramo se quedan y el
    // host va abreviado.
    let estrecha = abrir_unicode(6, 50, 12);
    let texto = estrecha.texto();
    assert!(
        !texto.contains("TIPO") && !texto.contains("dinámico"),
        "{texto}"
    );
    for parte in [
        "ESCUCHA → DESTINO",
        "127.0.0.1:5432 → 127.0.0.1:5432",
        "hetz-01",
        "●",
        "✕",
        "○",
    ] {
        assert!(texto.contains(parte), "falta «{parte}»:\n{texto}");
    }
    assert!(!texto.contains("hetzner-01"), "{texto}");
    // En estrecho (80 columnas) hay sitio para el tipo, pero el host sigue
    // abreviado; en Normal va entero.
    let texto = abrir_unicode(6, 80, 24).texto();
    assert!(
        texto.contains("TIPO") && texto.contains("│▸ ✕  local"),
        "{texto}"
    );
    assert!(texto.contains("  hetz-02  "), "{texto}");
    let texto = abrir_unicode(6, 120, 30).texto();
    assert!(texto.contains("  hetzner-02  "), "{texto}");

    // Registro: fecha, tipo y resultado siempre; host y detalle caen.
    let texto = abrir_unicode(7, 50, 12).texto();
    for parte in [
        "15/09 10:02:17",
        "sondeo_fallido",
        "error",
        "deliberacion_forzada",
    ] {
        assert!(texto.contains(parte), "falta «{parte}»:\n{texto}");
    }
    assert!(
        !texto.contains("rincon-dev") && !texto.contains("rechazada"),
        "{texto}"
    );
    let texto = abrir_unicode(7, 80, 24).texto();
    assert!(
        texto.contains("rincon-dev  error  conexión rechazada"),
        "{texto}"
    );

    // Snippets: antes de recortar el nombre cae el destino resumido; la
    // marca de crítico se queda.
    let texto = abrir_unicode(8, 50, 12).texto();
    assert!(texto.contains("▸ actualizar paquetes"), "{texto}");
    assert!(
        texto.contains("CRÍTICO") && texto.contains("→ 2 hosts"),
        "{texto}"
    );
    assert!(!texto.contains("hetzner-01 +1"), "{texto}");
    let texto = abrir_unicode(8, 80, 24).texto();
    assert!(texto.contains("hetzner-01 +1  → 2 hosts"), "{texto}");

    // Identidades: el tipo se ve entero si cabe todo y recortado si no; el
    // alias no se oculta.
    let texto = abrir_unicode(5, 50, 12).texto();
    assert!(texto.contains("yubikey  sk-ssh-ed25519@op…"), "{texto}");
    let texto = abrir_unicode(5, 80, 24).texto();
    assert!(
        texto.contains("yubikey  sk-ssh-ed25519@openssh.com"),
        "{texto}"
    );
}

/// A 200×60 tabla y detalle se quedan con el ancho máximo de un detalle
/// (120) y centrados: el marco del detalle va de la columna 40 a la 159.
#[test]
fn en_ventanas_muy_grandes_el_contenido_va_centrado() {
    for (nombre, tecla, _) in VISTAS {
        let prueba = abrir_unicode(tecla, 200, 60);
        let lineas = lineas(&prueba);
        let esquina = lineas.iter().position(|linea| {
            let caracteres: Vec<char> = linea.chars().collect();
            caracteres.get(40) == Some(&'┌') && caracteres.get(159) == Some(&'┐')
        });
        assert!(
            esquina.is_some(),
            "{nombre}: el detalle no está centrado con 120 de ancho:\n{}",
            prueba.texto()
        );
        // La fila seleccionada empieza en la columna del contenido.
        assert!(
            lineas
                .iter()
                .any(|linea| linea.chars().nth(40) == Some('▸')),
            "{nombre}: la tabla no está centrada:\n{}",
            prueba.texto()
        );
    }
}

/// Con el tema ASCII no se pinta ningún glifo Unicode (las letras con tilde
/// sí) en ningún modo, tampoco con los filtros escribiéndose.
#[test]
fn en_ascii_no_hay_glifos_unicode() {
    for (nombre, tecla, vista) in VISTAS {
        for (cols, filas) in TAMANOS {
            let mut prueba = abrir(tecla, cols, filas, arnes::tema_ascii());
            let texto = prueba.texto();
            assert!(solo_ascii(&texto), "{nombre} {cols}x{filas}:\n{texto}");
            // Con el filtro a medio escribir.
            match vista {
                Vista::Registro | Vista::Snippets | Vista::Tuneles => {
                    prueba.tecla(KeyCode::Char('/'));
                    prueba.tecla(KeyCode::Char('e'));
                }
                _ => {}
            }
            let texto = prueba.texto();
            assert!(
                solo_ascii(&texto),
                "{nombre} {cols}x{filas} filtrando:\n{texto}"
            );
        }
    }
}

// ---------------------------------------------------------------- secuencia

/// Muchas filas en cada vista, para que haya desplazamiento.
fn sembrar_mas(prueba: &AppPrueba, sembrado: &Sembrado) {
    let almacen = Almacen::abrir(&prueba._entorno.rutas.base_datos()).expect("almacén");
    let conexion = almacen.conexion();
    for indice in 0..40 {
        tuneles::crear(
            conexion,
            &DatosTunel {
                host_id: sembrado.host("hetzner-01"),
                nombre: format!("extra-{indice:02}"),
                tipo: TipoTunel::Local,
                escucha: format!("127.0.0.1:{}", 20_000 + indice),
                destino: Some(format!("10.0.0.{indice}:80")),
                automatico: false,
            },
        )
        .expect("túnel");
        snippets::crear(
            conexion,
            &DatosSnippet {
                nombre: format!("tarea {indice:02}"),
                comando: format!("echo {indice}"),
                destinos: vec![Destino::Etiqueta("web".to_string())],
                ..DatosSnippet::default()
            },
        )
        .expect("snippet");
        magi::registro::anotar(
            conexion,
            magi::registro::CLAVE_GENERADA,
            None,
            None,
            &format!("clave de prueba {indice:02}"),
            ResultadoRegistro::Ok,
        )
        .expect("registro");
        conexion
            .execute(
                "INSERT INTO IDENTIDADES (alias, tipo, huella, origen, ruta, anadida_en)
                 VALUES (?1, 'ssh-ed25519', ?2, 'fichero', NULL, '2026-09-01T10:00:00+00:00')",
                rusqlite::params![
                    format!("clave-{indice:02}"),
                    format!("SHA256:prueba{indice:02}")
                ],
            )
            .expect("identidad");
    }
    almacen.cerrar().expect("cerrar");
}

/// Selección de la vista, su lista y su filtro.
fn estado_de(prueba: &AppPrueba, vista: Vista) -> (usize, Lista, String, bool) {
    let app = &prueba.app;
    match vista {
        Vista::Tuneles => (
            app.seleccion_tunel,
            Lista::Tuneles,
            app.filtro_tuneles.clone(),
            app.filtro_tuneles_activo,
        ),
        Vista::Registro => (
            app.registro.seleccion,
            Lista::Registro,
            if app.registro.texto_activo {
                app.registro.campo.texto.clone()
            } else {
                app.registro.filtro.texto.clone()
            },
            app.registro.texto_activo,
        ),
        Vista::Identidades => (
            app.seleccion_identidad,
            Lista::Identidades,
            String::new(),
            false,
        ),
        Vista::Snippets => (
            app.snippets.seleccion,
            Lista::Snippets,
            app.snippets.filtro.clone(),
            app.snippets.filtro_activo,
        ),
        _ => unreachable!(),
    }
}

/// 200×60 → 80×24 → 40×12 → 200×60: la selección, el filtro (aplicado o a
/// medio escribir) y la vista se conservan, y la selección está siempre a la
/// vista (con la marca `▸` pintada) salvo con el aviso de tamaño.
#[test]
fn el_estado_se_conserva_al_cambiar_de_tamano() {
    for (nombre, tecla, vista) in VISTAS {
        for escribiendo in [false, true] {
            if vista == Vista::Identidades && escribiendo {
                continue;
            }
            let (mut prueba, sembrado) = AppPrueba::con_semilla(200, 60);
            sembrar_mas(&prueba, &sembrado);
            prueba.tecla(KeyCode::F(tecla));
            // Filtro: aplicado o a medio escribir.
            let filtro = match vista {
                Vista::Tuneles => "extra",
                Vista::Registro => "prueba",
                Vista::Snippets => "tarea",
                _ => "",
            };
            if !filtro.is_empty() {
                prueba.tecla(KeyCode::Char('/'));
                for caracter in filtro.chars() {
                    prueba.tecla(KeyCode::Char(caracter));
                }
                if !escribiendo {
                    prueba.tecla(KeyCode::Enter);
                }
            }
            for _ in 0..33 {
                prueba.tecla(KeyCode::Down);
            }
            let antes = estado_de(&prueba, vista);
            assert_eq!(antes.0, 33, "{nombre}: la selección no llegó a 33");
            assert_eq!(antes.3, escribiendo, "{nombre}");

            for (cols, filas) in [(200, 60), (80, 24), (40, 12), (200, 60), (90, 18), (50, 12)] {
                prueba.pasar_por(cols, filas);
                assert_eq!(prueba.app.vista, vista, "{nombre} {cols}x{filas}");
                assert_eq!(
                    estado_de(&prueba, vista),
                    antes,
                    "{nombre} {cols}x{filas}: cambió el estado"
                );
                let disposicion = prueba.app.disposicion();
                if disposicion.aviso {
                    assert_eq!((cols, filas), (40, 12));
                    continue;
                }
                let ventana = disposicion
                    .lista(antes.1)
                    .unwrap_or_else(|| panic!("{nombre} {cols}x{filas}: lista sin pintar"));
                assert!(
                    ventana.inicio <= antes.0 && antes.0 < ventana.inicio + ventana.filas,
                    "{nombre} {cols}x{filas}: selección fuera de la vista {ventana:?}"
                );
                assert!(
                    prueba.texto().contains('▸'),
                    "{nombre} {cols}x{filas}: sin marca de selección\n{}",
                    prueba.texto()
                );
            }
        }
    }
}

/// `PgDn` avanza lo que se ve de la lista en el último pintado: con la vista
/// baja, menos filas.
#[test]
fn la_pagina_usa_las_filas_del_ultimo_pintado() {
    for (nombre, tecla, vista) in VISTAS {
        let (mut prueba, sembrado) = AppPrueba::con_semilla(80, 24);
        sembrar_mas(&prueba, &sembrado);
        prueba.tecla(KeyCode::F(tecla));
        let (_, lista, _, _) = estado_de(&prueba, vista);
        let alta = prueba.app.disposicion().filas(lista);
        prueba.pasar_por(80, 14);
        let baja = prueba.app.disposicion().filas(lista);
        assert!(
            baja < alta,
            "{nombre}: {baja} filas en baja y {alta} en alta"
        );
        // Túneles entra con el túnel del host seleccionado: se parte de arriba.
        prueba.tecla(KeyCode::Home);
        assert_eq!(estado_de(&prueba, vista).0, 0, "{nombre}");
        prueba.tecla(KeyCode::PageDown);
        assert_eq!(estado_de(&prueba, vista).0, baja, "{nombre}");
        prueba.comprobar_listas();
    }
}

// ---------------------------------------------------------------- revisión

/// Pinta solo la vista en un área de `cols`×`filas`, sin el mínimo que
/// impone `ui::dibujar`.
fn pintar_suelta(prueba: &AppPrueba, vista: Vista, cols: u16, filas: u16) {
    let mut terminal =
        Terminal::new(TestBackend::new(cols.max(1), filas.max(1))).expect("terminal");
    terminal
        .draw(|marco| {
            let area = Rect::new(0, 0, cols, filas);
            let minimo = Minimo {
                tamano: Tamano::new(0, 0),
                exige: "prueba",
            };
            let mut disp = Disposicion::nueva(area, minimo);
            let app = &prueba.app;
            match vista {
                Vista::Tuneles => magi::ui::tuneles::dibujar(marco, area, app, &mut disp),
                Vista::Registro => magi::ui::registro::dibujar(marco, area, app, &mut disp),
                Vista::Identidades => magi::ui::identidades::dibujar(marco, area, app, &mut disp),
                Vista::Snippets => magi::ui::snippets::dibujar(marco, area, app, &mut disp),
                _ => unreachable!(),
            }
        })
        .unwrap_or_else(|error| panic!("{vista:?} a {cols}x{filas}: {error}"));
}

/// Cada vista se pinta sin pánico en cualquier área, también diminuta o de
/// ancho o alto cero (restas e índices), con la lista desplazada, el filtro
/// escribiéndose y en los dos temas. `ui::dibujar` no las llama por debajo de
/// su mínimo, pero la vista no debe depender de eso.
#[test]
fn las_vistas_no_entran_en_panico_con_areas_diminutas() {
    let anchos = [0, 1, 2, 3, 4, 12, 50, 99, 100, 200];
    let altos = [0, 1, 2, 3, 4, 12, 19, 20, 60];
    for (_, tecla, vista) in VISTAS {
        for tema in [magi::tema::Tema::respaldo(), arnes::tema_ascii()] {
            let (mut prueba, sembrado) = AppPrueba::con_semilla_y_tema(80, 24, tema);
            sembrar_mas(&prueba, &sembrado);
            prueba.tecla(KeyCode::F(tecla));
            if tecla == 6 {
                prueba.servidor(difusion_tuneles(&sembrado));
            }
            for _ in 0..20 {
                prueba.tecla(KeyCode::Down);
            }
            for escribiendo in [false, true] {
                if escribiendo && vista != Vista::Identidades {
                    prueba.tecla(KeyCode::Char('/'));
                    prueba.tecla(KeyCode::Char('e'));
                }
                for cols in anchos {
                    for filas in altos {
                        pintar_suelta(&prueba, vista, cols, filas);
                    }
                }
            }
        }
    }
}

/// Con el tema ASCII el diálogo de detalle (título y líneas) tampoco lleva
/// glifos: flechas, puntos medios, rayas y marcas pasan a ASCII. En Túneles
/// el seleccionado está caído (tráfico con flechas, «—» en «Desde») y en
/// Registro el primero es una deliberación con «→» y sin host.
#[test]
fn en_ascii_el_detalle_en_dialogo_no_tiene_glifos() {
    for (nombre, tecla, vista) in VISTAS {
        let mut prueba = abrir(tecla, 90, 18, arnes::tema_ascii());
        prueba.tecla(tecla_detalle(vista));
        let Some(Dialogo::Detalle { titulo, lineas, .. }) = &prueba.app.dialogo else {
            panic!("{nombre}: no se abrió el detalle");
        };
        let todo = format!("{titulo}\n{}", lineas.join("\n"));
        assert!(solo_ascii(&todo), "{nombre}:\n{todo}");
    }
}

/// Las palabras de un texto, separadas por un espacio.
fn palabras(texto: &str) -> String {
    texto.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// El diálogo de detalle enseña entero el texto largo, partido a lo ancho
/// del diálogo (68) y sin secuencias de escape ni caracteres de control.
fn comprobar_entero(nombre: &str, lineas: &[String], esperado: &str) {
    let todo = palabras(&lineas.join(" "));
    assert!(
        todo.contains(&palabras(esperado)),
        "{nombre}: el texto no está entero:\n{}",
        lineas.join("\n")
    );
    for linea in lineas {
        assert!(
            linea.chars().count() <= 68,
            "{nombre}: línea larga «{linea}»"
        );
        assert!(
            !linea.chars().any(char::is_control),
            "{nombre}: control en «{linea:?}»"
        );
    }
}

/// El diálogo de detalle no enseña menos que el panel inferior: el detalle de
/// una entrada del registro o el error de un túnel largos se parten a lo
/// ancho del diálogo (que no parte líneas) en lugar de cortarse, y llegan
/// saneados.
#[test]
fn el_detalle_en_dialogo_no_corta_los_textos_largos() {
    let largo = "la conexión con el host se cerró de forma inesperada \u{1b}[31mmientras\u{1b}[0m \
                 se copiaba el fichero de configuración principal del servicio web y habrá \
                 que repetir la transferencia completa desde el principio";
    let limpio = largo.replace("\u{1b}[31m", "").replace("\u{1b}[0m", "");

    // Registro: la entrada nueva se busca con el filtro.
    let (mut prueba, _) = AppPrueba::con_semilla(90, 18);
    let almacen = Almacen::abrir(&prueba._entorno.rutas.base_datos()).expect("almacén");
    magi::registro::anotar(
        almacen.conexion(),
        magi::registro::CLAVE_GENERADA,
        None,
        None,
        largo,
        ResultadoRegistro::Error,
    )
    .expect("registro");
    almacen.cerrar().expect("cerrar");
    prueba.tecla(KeyCode::F(7));
    prueba.tecla(KeyCode::Char('/'));
    for caracter in "inesperada".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    prueba.tecla(KeyCode::Enter);
    assert_eq!(prueba.app.registro.entradas.len(), 1);
    prueba.tecla(KeyCode::Enter);
    let Some(Dialogo::Detalle { lineas, .. }) = &prueba.app.dialogo else {
        panic!("registro: no se abrió el detalle");
    };
    comprobar_entero("registro", lineas, &limpio);

    // Túneles: el error de `grafana`, caído.
    let (mut prueba, sembrado) = AppPrueba::con_semilla(90, 18);
    prueba.tecla(KeyCode::F(6));
    let mut difusion = difusion_tuneles(&sembrado);
    if let MensajeServidor::Tuneles { lista } = &mut difusion {
        lista[1].ultimo_error = Some(largo.to_string());
    }
    prueba.servidor(difusion);
    prueba.tecla(KeyCode::Down);
    prueba.tecla(KeyCode::Enter);
    let Some(Dialogo::Detalle { lineas, .. }) = &prueba.app.dialogo else {
        panic!("túneles: no se abrió el detalle");
    };
    comprobar_entero("túneles", lineas, &limpio);
}

/// El host de Túneles se abrevia con el modo de la vista (menos de 100
/// columnas de terminal), no con el ancho del interior del marco: a 100
/// columnas la vista es Normal y va entero.
#[test]
fn el_host_se_abrevia_solo_en_modo_estrecho() {
    for (cols, modo, host) in [
        (100, ModoAncho::Normal, "hetzner-02"),
        (101, ModoAncho::Normal, "hetzner-02"),
        (99, ModoAncho::Estrecho, "hetz-02"),
    ] {
        let prueba = abrir_unicode(6, cols, 24);
        assert_eq!(prueba.app.disposicion().modo, modo, "{cols}");
        let fila = lineas(&prueba)
            .into_iter()
            .find(|linea| linea.contains("▸ ✕"))
            .expect("fila seleccionada");
        assert!(fila.contains(&format!("  {host}  ")), "{cols}: {fila}");
    }
}

/// En Identidades, con la vista baja y las revocadas a la vista, el recuento
/// del pie no tapa «↵ detalle»: cae antes el recordatorio de `v` (la barra
/// ya lo enseña), que vuelve cuando hay sitio.
#[test]
fn el_pie_de_identidades_no_tapa_el_detalle_plegado() {
    let (mut prueba, _) = AppPrueba::con_semilla(50, 12);
    let almacen = Almacen::abrir(&prueba._entorno.rutas.base_datos()).expect("almacén");
    almacen
        .conexion()
        .execute(
            "UPDATE IDENTIDADES SET revocada_en = '2026-09-02T10:00:00+00:00'
             WHERE alias = 'yubikey'",
            [],
        )
        .expect("revocar");
    almacen.cerrar().expect("cerrar");
    prueba.tecla(KeyCode::F(5));
    prueba.tecla(KeyCode::Char('v'));
    assert!(prueba.app.ver_revocadas);
    let pie = |prueba: &AppPrueba| {
        let lineas = lineas(prueba);
        lineas[lineas.len() - 2].clone()
    };
    let estrecho = pie(&prueba);
    assert!(
        estrecho.contains(" ↵ detalle ") && estrecho.contains("2 clave(s) · con revocadas "),
        "{estrecho}"
    );
    assert!(!estrecho.contains("v revocadas"), "{estrecho}");
    prueba.pasar_por(80, 12);
    let ancho = pie(&prueba);
    assert!(
        ancho.contains(" ↵ detalle ")
            && ancho.contains("2 clave(s) · con revocadas · v revocadas "),
        "{ancho}"
    );
}
