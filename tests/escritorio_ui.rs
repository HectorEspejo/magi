//! Pruebas de interfaz de los escritorios remotos (Fase 12) sin hilos, red ni
//! servidor: App::de_prueba y Cliente::de_prueba capturan lo que se manda.
//!
//! Cubren el flujo de Hosts (▣, ↵ y peticion_id en su rango), la pestaña ▣ y
//! sus teclas, el visor (enlace cuando no hay navegador), los diálogos de
//! certificado y credenciales, olvidar la huella con confirmación y las vistas
//! Flota y Sesiones.

use std::path::PathBuf;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use magi::app::{App, Dialogo, Evento};
use magi::config::{Config, Rutas};
use magi::escritorio::{
    DatosCertificado, EstadoEscritorio, InfoEscritorio, OpcionesEscritorio, PeticionCredenciales,
    TamanoEscritorio, TipoCambioCertificado, TipoHost,
};
use magi::modelo::{DatosHost, IdentidadRef, Origen, ResultadoRegistro};
use magi::protocolo::{MensajeCliente, MensajeServidor, RANGO_ESCRITORIOS};
use magi::registro::FiltroRegistro;
use magi::tema::Tema;
use magi::ui::Vista;

/// Huella de documentación.
const HUELLA: &str = "SHA256:T2qRAb3dE5fG7hJ9kL1mN3pQ5rS7tU9vW1xY3zA5bC7";

struct Entorno {
    _temporal: tempfile::TempDir,
    rutas: Rutas,
    config: Config,
}

fn entorno() -> Entorno {
    let temporal = tempfile::Builder::new()
        .prefix("magiui")
        .tempdir()
        .expect("directorio temporal");
    let raiz = temporal.path().to_path_buf();
    Entorno {
        rutas: Rutas {
            datos: raiz.join("datos"),
            config: raiz.join("config"),
            estado: raiz.join("estado"),
            hogar: raiz.join("hogar"),
            runtime: raiz.join("runtime"),
        },
        config: Config::default(),
        _temporal: temporal,
    }
}

/// Siembra un RDP con dominio, certificado aceptado y etiquetas.
fn sembrar_rdp(entorno: &Entorno) -> i64 {
    let almacen = magi::almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
    let id = almacen
        .crear_host(
            &DatosHost {
                nombre: "win-01".to_string(),
                direccion: "192.0.2.30".to_string(),
                puerto: 3389,
                tipo: TipoHost::Rdp,
                escritorio_dominio: Some("OFICINA".to_string()),
                escritorio_opciones: OpcionesEscritorio {
                    tamano: TamanoEscritorio::Fijo {
                        ancho: 1920,
                        alto: 1080,
                    },
                    portapapeles: true,
                    ..OpcionesEscritorio::default()
                },
                usuario: Some("usuario".to_string()),
                identidad_ref: IdentidadRef::Contrasena,
                etiquetas: vec!["windows".to_string()],
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .expect("host de prueba");
    almacen
        .fijar_huella_escritorio(id, Some(HUELLA))
        .expect("huella");
    id
}

fn con_app(
    entorno: &Entorno,
) -> (
    App,
    Terminal<TestBackend>,
    tokio::sync::mpsc::UnboundedReceiver<MensajeCliente>,
) {
    let almacen = magi::almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
    let (mut app, enviados) = App::de_prueba(
        entorno.rutas.clone(),
        entorno.config.clone(),
        Tema::respaldo(),
        almacen,
    )
    .expect("app de prueba");
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("terminal");
    app.iniciar_pintado(&mut terminal).expect("pintado");
    (app, terminal, enviados)
}

fn tecla(app: &mut App, terminal: &mut Terminal<TestBackend>, codigo: KeyCode) {
    app.procesar(Evento::Tecla(KeyEvent::new(codigo, KeyModifiers::NONE)));
    app.paso(terminal, Instant::now()).expect("paso");
}

fn tecla_con(
    app: &mut App,
    terminal: &mut Terminal<TestBackend>,
    codigo: KeyCode,
    modificadores: KeyModifiers,
) {
    app.procesar(Evento::Tecla(KeyEvent::new(codigo, modificadores)));
    app.paso(terminal, Instant::now()).expect("paso");
}

fn escribir(app: &mut App, terminal: &mut Terminal<TestBackend>, texto: &str) {
    for caracter in texto.chars() {
        tecla(app, terminal, KeyCode::Char(caracter));
    }
}

fn servidor(app: &mut App, terminal: &mut Terminal<TestBackend>, mensaje: MensajeServidor) {
    app.procesar(Evento::Servidor(mensaje));
    app.paso(terminal, Instant::now()).expect("paso");
}

fn enviados(rx: &mut tokio::sync::mpsc::UnboundedReceiver<MensajeCliente>) -> Vec<MensajeCliente> {
    let mut lista = Vec::new();
    while let Ok(mensaje) = rx.try_recv() {
        lista.push(mensaje);
    }
    lista
}

fn texto(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let mut lineas = Vec::new();
    for y in 0..buffer.area.height {
        let mut linea = String::new();
        for x in 0..buffer.area.width {
            linea.push_str(buffer[(x, y)].symbol());
        }
        lineas.push(linea.trim_end().to_string());
    }
    lineas.join("\n")
}

/// Sesión de escritorio que difunde el servidor.
fn info(id: u32, host_id: i64, estado: EstadoEscritorio, visor: bool) -> InfoEscritorio {
    let ahora = chrono::Utc::now().timestamp();
    InfoEscritorio {
        id,
        host_id,
        nombre: "win-01".to_string(),
        tipo: TipoHost::Rdp,
        estado,
        fase: None,
        motivo: (estado == EstadoEscritorio::Caido).then(|| "conexión perdida".to_string()),
        visor,
        sin_visor_desde: if estado == EstadoEscritorio::Abierto && !visor {
            Some(ahora - 15)
        } else {
            None
        },
        ancho: 1920,
        alto: 1080,
        imagen: None,
        fps: 0,
        directo: false,
        a_traves_de: None,
        desde: ahora - 60,
        redimensionable: false,
    }
}

/// Abre el escritorio desde Hosts y crea su pestaña con lo que difunde el
/// servidor.
fn abrir_con_pestana(
    app: &mut App,
    terminal: &mut Terminal<TestBackend>,
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<MensajeCliente>,
    host_id: i64,
) -> (u64, u32) {
    tecla(app, terminal, KeyCode::F(2));
    app.seleccionar_host_id(host_id);
    tecla(app, terminal, KeyCode::Enter);
    let peticion = enviados(rx)
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::AbrirEscritorio {
                peticion_id,
                host_id: enviado,
            } => {
                assert_eq!(enviado, host_id);
                assert!(
                    peticion_id >= RANGO_ESCRITORIOS,
                    "peticion_id {peticion_id} fuera del rango de escritorios"
                );
                Some(peticion_id)
            }
            _ => None,
        })
        .expect("sin AbrirEscritorio");
    let escritorio_id = 7;
    servidor(
        app,
        terminal,
        MensajeServidor::Escritorios {
            lista: vec![info(
                escritorio_id,
                host_id,
                EstadoEscritorio::Abriendo,
                false,
            )],
        },
    );
    assert_eq!(app.vista, Vista::Sesion, "no se abrió la pestaña");
    assert_eq!(app.escritorio_activo_id(), Some(escritorio_id));
    (peticion, escritorio_id)
}

// ------------------------------------------------------------------- pruebas

/// ↵ en un escritorio de Hosts manda AbrirEscritorio con peticion_id del
/// rango y la difusión crea y activa la pestaña ▣.
#[test]
fn abrir_desde_hosts_manda_la_peticion_y_abre_la_pestana() {
    let entorno = entorno();
    let host_id = sembrar_rdp(&entorno);
    let (mut app, mut terminal, mut rx) = con_app(&entorno);

    tecla(&mut app, &mut terminal, KeyCode::F(2));
    let texto_hosts = texto(&terminal);
    assert!(texto_hosts.contains('▣'), "{texto_hosts}");
    assert!(texto_hosts.contains("RDP"), "{texto_hosts}");
    app.seleccionar_host_id(host_id);
    tecla(&mut app, &mut terminal, KeyCode::Enter);

    let peticion = enviados(&mut rx)
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::AbrirEscritorio { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("sin AbrirEscritorio");
    assert!(peticion >= RANGO_ESCRITORIOS);

    servidor(
        &mut app,
        &mut terminal,
        MensajeServidor::Escritorios {
            lista: vec![info(7, host_id, EstadoEscritorio::Abriendo, false)],
        },
    );
    assert_eq!(app.vista, Vista::Sesion);
    assert_eq!(app.escritorio_activo_id(), Some(7));
    let texto_sesion = texto(&terminal);
    assert!(texto_sesion.contains("ESCRITORIO"), "{texto_sesion}");
    assert!(texto_sesion.contains("abriendo"), "{texto_sesion}");
}

/// Si ya hay sesión de ese host, ↵ va a su pestaña y pide un enlace nuevo.
#[test]
fn si_ya_hay_sesion_pide_un_enlace_nuevo() {
    let entorno = entorno();
    let host_id = sembrar_rdp(&entorno);
    let (mut app, mut terminal, mut rx) = con_app(&entorno);
    abrir_con_pestana(&mut app, &mut terminal, &mut rx, host_id);
    servidor(
        &mut app,
        &mut terminal,
        MensajeServidor::Escritorios {
            lista: vec![info(7, host_id, EstadoEscritorio::Abierto, true)],
        },
    );

    tecla(&mut app, &mut terminal, KeyCode::F(2));
    app.seleccionar_host_id(host_id);
    tecla(&mut app, &mut terminal, KeyCode::Enter);
    let peticion = enviados(&mut rx)
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::PedirEnlace {
                peticion_id,
                escritorio_id,
            } => {
                assert_eq!(escritorio_id, 7);
                Some(peticion_id)
            }
            _ => None,
        })
        .expect("sin PedirEnlace");
    assert!(peticion >= RANGO_ESCRITORIOS);
    assert_eq!(app.vista, Vista::Sesion);
}

/// EscritorioAbierto abre el visor; si el navegador no se puede lanzar, la
/// pestaña enseña el enlace para copiarlo con y durante 60 s.
#[test]
fn escritorio_abierto_lanza_el_visor_o_ensena_el_enlace() {
    let mut entorno = entorno();
    entorno.config.escritorio.navegador = "magi-navegador-que-no-existe --app={url}".to_string();
    let host_id = sembrar_rdp(&entorno);
    let (mut app, mut terminal, mut rx) = con_app(&entorno);
    let (peticion, escritorio_id) = abrir_con_pestana(&mut app, &mut terminal, &mut rx, host_id);

    let enlace = "http://127.0.0.1:8080/entrar?t=enlace-de-prueba";
    servidor(
        &mut app,
        &mut terminal,
        MensajeServidor::EscritorioAbierto {
            peticion_id: peticion,
            escritorio_id,
            enlace: enlace.to_string(),
        },
    );
    assert!(
        app.enlace_visible.is_some(),
        "sin enlace a la vista tras fallar el navegador"
    );
    assert_eq!(app.enlace_reciente(escritorio_id).as_deref(), Some(enlace));
    let texto_sesion = texto(&terminal);
    assert!(texto_sesion.contains("Enlace"), "{texto_sesion}");
    assert!(texto_sesion.contains("entrar?t="), "{texto_sesion}");
    assert!(texto_sesion.contains("cópialo con y"), "{texto_sesion}");
}

/// Enlace con abrir=true abre el visor; con abrir=false se copia.
#[test]
fn el_enlace_se_abre_o_se_copia_segun_lo_pedido() {
    let mut entorno = entorno();
    entorno.config.escritorio.navegador = "magi-navegador-que-no-existe --app={url}".to_string();
    let host_id = sembrar_rdp(&entorno);
    let (mut app, mut terminal, mut rx) = con_app(&entorno);
    let (_, escritorio_id) = abrir_con_pestana(&mut app, &mut terminal, &mut rx, host_id);

    tecla(&mut app, &mut terminal, KeyCode::Enter);
    let peticion = enviados(&mut rx)
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::PedirEnlace { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("sin PedirEnlace al pulsar enter");
    servidor(
        &mut app,
        &mut terminal,
        MensajeServidor::Enlace {
            peticion_id: peticion,
            escritorio_id,
            enlace: "http://127.0.0.1:8080/entrar?t=uno".to_string(),
        },
    );
    assert!(app.enlace_visible.is_some(), "enter no abrió el visor");

    // y: pide otro enlace que se copia (sin navegador de por medio).
    app.enlace_visible = None;
    tecla(&mut app, &mut terminal, KeyCode::Char('y'));
    let peticion = enviados(&mut rx)
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::PedirEnlace {
                peticion_id,
                escritorio_id: enviado,
            } => {
                assert_eq!(enviado, escritorio_id);
                Some(peticion_id)
            }
            _ => None,
        })
        .expect("sin PedirEnlace al pulsar y");
    servidor(
        &mut app,
        &mut terminal,
        MensajeServidor::Enlace {
            peticion_id: peticion,
            escritorio_id,
            enlace: "http://127.0.0.1:8080/entrar?t=dos".to_string(),
        },
    );
    // Sin portapapeles utilizable, el enlace se queda a la vista.
    assert!(
        app.enlace_visible.is_some() || app.mensaje.is_some(),
        "ni copiado ni a la vista"
    );
}

/// En la pestaña: prefijo r reconecta, prefijo x cierra con confirmación y
/// «s» la confirma.
#[test]
fn las_teclas_de_la_pestana_reconectan_y_cierran() {
    let entorno = entorno();
    let host_id = sembrar_rdp(&entorno);
    let (mut app, mut terminal, mut rx) = con_app(&entorno);
    let (_, escritorio_id) = abrir_con_pestana(&mut app, &mut terminal, &mut rx, host_id);
    servidor(
        &mut app,
        &mut terminal,
        MensajeServidor::Escritorios {
            lista: vec![info(7, host_id, EstadoEscritorio::Caido, false)],
        },
    );

    // prefijo r
    tecla_con(
        &mut app,
        &mut terminal,
        KeyCode::Char(']'),
        KeyModifiers::CONTROL,
    );
    tecla(&mut app, &mut terminal, KeyCode::Char('r'));
    let peticion = enviados(&mut rx)
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::ReconectarEscritorio {
                peticion_id,
                escritorio_id: enviado,
            } => {
                assert_eq!(enviado, escritorio_id);
                Some(peticion_id)
            }
            _ => None,
        })
        .expect("sin ReconectarEscritorio");
    assert!(peticion >= RANGO_ESCRITORIOS);

    // prefijo x → confirmación → s
    tecla_con(
        &mut app,
        &mut terminal,
        KeyCode::Char(']'),
        KeyModifiers::CONTROL,
    );
    tecla(&mut app, &mut terminal, KeyCode::Char('x'));
    assert!(
        matches!(app.dialogo, Some(Dialogo::Confirmar { .. })),
        "sin confirmación de cierre"
    );
    tecla(&mut app, &mut terminal, KeyCode::Char('s'));
    assert!(
        enviados(&mut rx)
            .into_iter()
            .any(|mensaje| mensaje == MensajeCliente::CerrarEscritorio { escritorio_id }),
        "sin CerrarEscritorio"
    );
}

/// Certificado nuevo: «a» guarda la huella, la anota y responde.
#[test]
fn el_certificado_nuevo_se_guarda_antes_de_responder() {
    let entorno = entorno();
    let host_id = sembrar_rdp(&entorno);
    let (mut app, mut terminal, mut rx) = con_app(&entorno);
    let peticion = RANGO_ESCRITORIOS + 42;
    let datos = DatosCertificado {
        tipo: TipoCambioCertificado::Nuevo,
        sujeto: "CN=WIN-01".to_string(),
        caducidad: "3 may 2027".to_string(),
        huella: HUELLA.to_string(),
        anterior: None,
    };
    servidor(
        &mut app,
        &mut terminal,
        MensajeServidor::PideHuellaEscritorio {
            peticion_id: peticion,
            escritorio_id: 7,
            host_id,
            host_nombre: "win-01".to_string(),
            datos,
        },
    );
    assert!(matches!(app.dialogo, Some(Dialogo::Escritorio(_))));
    tecla(&mut app, &mut terminal, KeyCode::Char('a'));
    assert!(
        enviados(&mut rx).into_iter().any(|mensaje| mensaje
            == MensajeCliente::RespuestaHuellaEscritorio {
                peticion_id: peticion,
                aceptada: true
            }),
        "sin RespuestaHuellaEscritorio aceptada"
    );
    let host = app.almacen.obtener_host(host_id).expect("host");
    assert_eq!(host.huella_escritorio.as_deref(), Some(HUELLA));
    let registro = app
        .almacen
        .listar_registro(&FiltroRegistro::default(), 50, 0)
        .expect("registro");
    assert!(
        registro.iter().any(|entrada| {
            entrada.tipo == "huella_escritorio"
                && entrada.resultado == ResultadoRegistro::Ok
                && entrada.detalle.contains("aceptada")
        }),
        "sin anotación de huella_escritorio"
    );
}

/// Certificado cambiado: no se sustituye sin el nombre exacto; al escribirlo
/// se guarda, se anota «sustituida» y se responde.
#[test]
fn el_certificado_cambiado_exige_el_nombre() {
    let entorno = entorno();
    let host_id = sembrar_rdp(&entorno);
    let (mut app, mut terminal, mut rx) = con_app(&entorno);
    let nueva = "SHA256:NuevaHuellaDeDocumentacion1234567890abcdefghij";
    let peticion = RANGO_ESCRITORIOS + 43;
    let datos = DatosCertificado {
        tipo: TipoCambioCertificado::Cambiado,
        sujeto: "CN=WIN-01".to_string(),
        caducidad: "3 may 2027".to_string(),
        huella: nueva.to_string(),
        anterior: Some(HUELLA.to_string()),
    };
    servidor(
        &mut app,
        &mut terminal,
        MensajeServidor::PideHuellaEscritorio {
            peticion_id: peticion,
            escritorio_id: 7,
            host_id,
            host_nombre: "win-01".to_string(),
            datos,
        },
    );
    escribir(&mut app, &mut terminal, "win-1");
    tecla(&mut app, &mut terminal, KeyCode::Enter);
    assert!(
        matches!(app.dialogo, Some(Dialogo::Escritorio(_))),
        "sustituyó sin el nombre exacto"
    );
    assert!(
        enviados(&mut rx).is_empty(),
        "contestó antes de escribir el nombre"
    );

    tecla(&mut app, &mut terminal, KeyCode::Backspace);
    escribir(&mut app, &mut terminal, "01");
    tecla(&mut app, &mut terminal, KeyCode::Enter);
    assert!(
        enviados(&mut rx).into_iter().any(|mensaje| mensaje
            == MensajeCliente::RespuestaHuellaEscritorio {
                peticion_id: peticion,
                aceptada: true
            }),
        "sin respuesta al sustituir"
    );
    let host = app.almacen.obtener_host(host_id).expect("host");
    assert_eq!(host.huella_escritorio.as_deref(), Some(nueva));
    let registro = app
        .almacen
        .listar_registro(&FiltroRegistro::default(), 50, 0)
        .expect("registro");
    assert!(
        registro
            .iter()
            .any(|entrada| entrada.tipo == "huella_escritorio"
                && entrada.detalle.contains("sustituida")),
        "sin anotación «sustituida»"
    );
}

/// Credenciales que pide el servidor: se responden con el usuario y la
/// contraseña, sin pasar por el llavero.
#[test]
fn las_credenciales_del_servidor_se_responden() {
    let entorno = entorno();
    let host_id = sembrar_rdp(&entorno);
    let (mut app, mut terminal, mut rx) = con_app(&entorno);
    let peticion = RANGO_ESCRITORIOS + 44;
    servidor(
        &mut app,
        &mut terminal,
        MensajeServidor::PideCredenciales {
            peticion_id: peticion,
            escritorio_id: 7,
            host_nombre: "win-01".to_string(),
            peticion: PeticionCredenciales {
                tipo: TipoHost::Rdp,
                usuario: Some("usuario".to_string()),
                dominio: Some("OFICINA".to_string()),
                intento: 1,
            },
        },
    );
    assert!(matches!(app.dialogo, Some(Dialogo::Escritorio(_))));
    escribir(&mut app, &mut terminal, "secreto");
    tecla(&mut app, &mut terminal, KeyCode::Enter);
    let credenciales = enviados(&mut rx)
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::Credenciales {
                peticion_id,
                usuario,
                dominio,
                contrasena,
                recordar,
            } => Some((peticion_id, usuario, dominio, contrasena, recordar)),
            _ => None,
        })
        .expect("sin Credenciales");
    assert_eq!(credenciales.0, peticion);
    assert_eq!(credenciales.1, "usuario");
    assert_eq!(credenciales.2.as_deref(), Some("OFICINA"));
    assert_eq!(credenciales.3.como_str(), "secreto");
    assert!(!credenciales.4);
    assert!(app.almacen.obtener_host(host_id).is_ok());
}

/// Cancelar las credenciales cierra la apertura (no hay mensaje negativo).
#[test]
fn cancela_credenciales_cerrando_el_escritorio() {
    let entorno = entorno();
    sembrar_rdp(&entorno);
    let (mut app, mut terminal, mut rx) = con_app(&entorno);
    servidor(
        &mut app,
        &mut terminal,
        MensajeServidor::PideCredenciales {
            peticion_id: RANGO_ESCRITORIOS + 45,
            escritorio_id: 7,
            host_nombre: "win-01".to_string(),
            peticion: PeticionCredenciales {
                tipo: TipoHost::Rdp,
                usuario: None,
                dominio: None,
                intento: 2,
            },
        },
    );
    tecla(&mut app, &mut terminal, KeyCode::Esc);
    assert!(
        enviados(&mut rx)
            .into_iter()
            .any(|mensaje| mensaje == MensajeCliente::CerrarEscritorio { escritorio_id: 7 }),
        "sin CerrarEscritorio al cancelar"
    );
}

/// Ficha: «olvidar la huella del certificado» pide confirmación y la borra.
#[test]
fn olvidar_la_huella_pide_confirmacion() {
    let entorno = entorno();
    let host_id = sembrar_rdp(&entorno);
    let (mut app, mut terminal, _rx) = con_app(&entorno);
    tecla(&mut app, &mut terminal, KeyCode::F(2));
    app.seleccionar_host_id(host_id);
    tecla(&mut app, &mut terminal, KeyCode::Char('e'));
    assert_eq!(app.vista, Vista::Ficha);
    // Tabulador hasta «olvidar la huella» (el último campo del formulario RDP;
    // el tamaño fijo de la semilla añade el campo del AxH).
    for _ in 0..13 {
        tecla(&mut app, &mut terminal, KeyCode::Tab);
    }
    assert_eq!(
        app.ficha.as_ref().map(|ficha| ficha.campo),
        Some(magi::app::CampoFicha::OlvidarHuella),
        "el foco no llegó a «olvidar la huella»"
    );
    tecla(&mut app, &mut terminal, KeyCode::Enter);
    assert!(
        matches!(app.dialogo, Some(Dialogo::Confirmar { .. })),
        "sin confirmación"
    );
    tecla(&mut app, &mut terminal, KeyCode::Char('s'));
    let host = app.almacen.obtener_host(host_id).expect("host");
    assert_eq!(host.huella_escritorio, None);
}

/// La lista del servidor se refleja en la pestaña y, al desaparecer, la
/// pestaña se retira.
#[test]
fn la_lista_se_refleja_en_la_pestana() {
    let entorno = entorno();
    let host_id = sembrar_rdp(&entorno);
    let (mut app, mut terminal, mut rx) = con_app(&entorno);
    abrir_con_pestana(&mut app, &mut terminal, &mut rx, host_id);
    servidor(
        &mut app,
        &mut terminal,
        MensajeServidor::Escritorios {
            lista: vec![info(7, host_id, EstadoEscritorio::Caido, false)],
        },
    );
    assert_eq!(
        app.pestanas
            .iter()
            .find(|pestana| pestana.escritorio_id == Some(7))
            .map(|pestana| pestana.estado),
        Some(magi::protocolo::EstadoSesionRemota::Caida)
    );
    let texto_sesion = texto(&terminal);
    assert!(texto_sesion.contains("caído"), "{texto_sesion}");
    assert!(texto_sesion.contains("conexión perdida"), "{texto_sesion}");

    servidor(
        &mut app,
        &mut terminal,
        MensajeServidor::Escritorios { lista: Vec::new() },
    );
    assert_eq!(app.escritorio_activo_id(), None);
    assert!(
        app.pestanas
            .iter()
            .all(|pestana| pestana.escritorio_id.is_none()),
        "la pestaña de escritorio no se retiró"
    );
}

/// La paleta incluye los escritorios y el filtro de búsqueda los encuentra.
#[test]
fn la_paleta_incluye_escritorios() {
    let entorno = entorno();
    sembrar_rdp(&entorno);
    let (mut app, mut terminal, _rx) = con_app(&entorno);
    tecla(&mut app, &mut terminal, KeyCode::F(2));
    tecla_con(
        &mut app,
        &mut terminal,
        KeyCode::Char('p'),
        KeyModifiers::CONTROL,
    );
    let paleta = app.paleta.as_ref().expect("paleta");
    let etiquetas: Vec<String> = paleta
        .entradas
        .iter()
        .map(|entrada| entrada.etiqueta.clone())
        .collect();
    assert!(
        etiquetas
            .iter()
            .any(|etiqueta| etiqueta.starts_with("abrir escritorio · win-01")),
        "{etiquetas:?}"
    );
    assert!(
        etiquetas
            .iter()
            .any(|etiqueta| etiqueta == "editar host · win-01"),
        "{etiquetas:?}"
    );
    app.paleta.as_mut().unwrap().consulta = magi::ui::componentes::CampoTexto::nuevo("rdp");
    app.paleta.as_mut().unwrap().recalcular();
    assert!(
        app.paleta
            .as_ref()
            .is_some_and(|paleta| !paleta.filtradas.is_empty()),
        "el filtro no encuentra «rdp»"
    );
}

/// Flota lista los escritorios sin sondearlos y con «sin sondeo».
#[test]
fn la_flota_no_sondea_los_escritorios() {
    let entorno = entorno();
    let host_id = sembrar_rdp(&entorno);
    let (mut app, mut terminal, _rx) = con_app(&entorno);
    assert_eq!(app.vista, Vista::Flota);
    let texto_flota = texto(&terminal);
    assert!(texto_flota.contains("SIN SONDEO"), "{texto_flota}");
    app.seleccionar_host_id(host_id);
    app.seleccion_flota = 0;
    tecla(&mut app, &mut terminal, KeyCode::Char('r'));
    assert!(
        app.mensaje
            .as_ref()
            .is_some_and(|mensaje| mensaje.texto.contains("no se sondea")),
        "{:?}",
        app.mensaje
    );
    assert!(app.sondeando.is_empty());
}

/// Vista Sesiones: columna TIPO, reconectar y cerrar como las de terminal.
#[test]
fn sesiones_tiene_columna_tipo_y_las_mismas_teclas() {
    let entorno = entorno();
    let host_id = sembrar_rdp(&entorno);
    let (mut app, mut terminal, mut rx) = con_app(&entorno);
    let (_, escritorio_id) = abrir_con_pestana(&mut app, &mut terminal, &mut rx, host_id);
    servidor(
        &mut app,
        &mut terminal,
        MensajeServidor::Escritorios {
            lista: vec![info(7, host_id, EstadoEscritorio::Abierto, true)],
        },
    );
    // Desde Sesión se va a la lista con el prefijo y la tecla l.
    tecla_con(
        &mut app,
        &mut terminal,
        KeyCode::Char(']'),
        KeyModifiers::CONTROL,
    );
    tecla(&mut app, &mut terminal, KeyCode::Char('l'));
    assert_eq!(app.vista, Vista::Sesiones);
    let texto_lista = texto(&terminal);
    assert!(texto_lista.contains("TIPO"), "{texto_lista}");
    assert!(texto_lista.contains("RDP"), "{texto_lista}");

    tecla(&mut app, &mut terminal, KeyCode::Char('r'));
    assert!(
        enviados(&mut rx).into_iter().any(|mensaje| matches!(
            mensaje,
            MensajeCliente::ReconectarEscritorio { escritorio_id: enviado, .. }
                if enviado == escritorio_id
        )),
        "sin ReconectarEscritorio desde Sesiones"
    );

    tecla(&mut app, &mut terminal, KeyCode::Char('x'));
    assert!(matches!(app.dialogo, Some(Dialogo::Confirmar { .. })));
    tecla(&mut app, &mut terminal, KeyCode::Char('s'));
    assert!(
        enviados(&mut rx)
            .into_iter()
            .any(|mensaje| mensaje == MensajeCliente::CerrarEscritorio { escritorio_id }),
        "sin CerrarEscritorio desde Sesiones"
    );
}

/// El peticion_id de cada petición es único y del rango de escritorios.
#[test]
fn las_peticiones_llevan_ids_unicos_del_rango() {
    let entorno = entorno();
    let host_id = sembrar_rdp(&entorno);
    let (mut app, mut terminal, mut rx) = con_app(&entorno);
    let (primera, escritorio_id) = abrir_con_pestana(&mut app, &mut terminal, &mut rx, host_id);
    tecla(&mut app, &mut terminal, KeyCode::Enter);
    let segunda = enviados(&mut rx)
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::PedirEnlace { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("sin PedirEnlace");
    assert_ne!(primera, segunda);
    assert!(primera >= RANGO_ESCRITORIOS && segunda >= RANGO_ESCRITORIOS);
    assert_eq!(app.escritorio_activo_id(), Some(escritorio_id));
}

/// Un Error con peticion_id de escritorio que ya no está en vuelo solo avisa.
#[test]
fn un_error_de_escritorio_avisa_sin_confundirse_con_archivos() {
    let entorno = entorno();
    let host_id = sembrar_rdp(&entorno);
    let (mut app, mut terminal, mut rx) = con_app(&entorno);
    let (peticion, _) = abrir_con_pestana(&mut app, &mut terminal, &mut rx, host_id);
    servidor(
        &mut app,
        &mut terminal,
        MensajeServidor::Error {
            mensaje: "cancelado desde la ventana".to_string(),
            peticion_id: Some(peticion),
        },
    );
    assert!(app.peticiones_escritorio.is_empty());
    assert!(
        app.mensaje
            .as_ref()
            .is_some_and(|mensaje| mensaje.texto.contains("cancelado")),
        "{:?}",
        app.mensaje
    );
    servidor(
        &mut app,
        &mut terminal,
        MensajeServidor::Error {
            mensaje: "ya no estaba".to_string(),
            peticion_id: Some(RANGO_ESCRITORIOS + 999),
        },
    );
    assert!(
        app.mensaje
            .as_ref()
            .is_some_and(|mensaje| mensaje.texto.contains("escritorio")),
        "{:?}",
        app.mensaje
    );
}

/// Las preguntas del servidor se apilan: no pisan el diálogo abierto y, al
/// contestarlas, vuelve el que estaba debajo.
#[test]
fn las_preguntas_del_servidor_se_apilan() {
    let entorno = entorno();
    let host_id = sembrar_rdp(&entorno);
    let (mut app, mut terminal, _rx) = con_app(&entorno);
    // Un diálogo local abierto encima de Hosts: la confirmación de borrado.
    tecla(&mut app, &mut terminal, KeyCode::F(2));
    app.seleccionar_host_id(host_id);
    tecla(&mut app, &mut terminal, KeyCode::Char('x'));
    assert!(matches!(app.dialogo, Some(Dialogo::Confirmar { .. })));

    // Llega una pregunta del servidor: se queda encima.
    let datos = DatosCertificado {
        tipo: TipoCambioCertificado::Nuevo,
        sujeto: "CN=WIN-01".to_string(),
        caducidad: "3 may 2027".to_string(),
        huella: HUELLA.to_string(),
        anterior: None,
    };
    servidor(
        &mut app,
        &mut terminal,
        MensajeServidor::PideHuellaEscritorio {
            peticion_id: RANGO_ESCRITORIOS + 46,
            escritorio_id: 7,
            host_id,
            host_nombre: "win-01".to_string(),
            datos,
        },
    );
    assert!(matches!(app.dialogo, Some(Dialogo::Escritorio(_))));
    // Al cancelarla vuelve la confirmación que estaba debajo.
    tecla(&mut app, &mut terminal, KeyCode::Esc);
    assert!(
        matches!(app.dialogo, Some(Dialogo::Confirmar { .. })),
        "no volvió el diálogo apilado"
    );
}

/// Si la sesión desaparece con una pregunta del servidor abierta, el diálogo
/// se retira y no queda ninguna petición en vuelo apuntando a esa sesión.
#[test]
fn las_preguntas_de_una_sesion_que_desaparece_se_retiran() {
    let entorno = entorno();
    let host_id = sembrar_rdp(&entorno);
    let (mut app, mut terminal, mut rx) = con_app(&entorno);
    abrir_con_pestana(&mut app, &mut terminal, &mut rx, host_id);
    servidor(
        &mut app,
        &mut terminal,
        MensajeServidor::Escritorios {
            lista: vec![info(7, host_id, EstadoEscritorio::Abriendo, false)],
        },
    );
    // Se pide un enlace y, antes de que llegue, el servidor pregunta por el
    // certificado (queda una petición en vuelo y un diálogo abierto).
    tecla(&mut app, &mut terminal, KeyCode::Enter);
    let datos = DatosCertificado {
        tipo: TipoCambioCertificado::Nuevo,
        sujeto: "CN=WIN-01".to_string(),
        caducidad: "3 may 2027".to_string(),
        huella: HUELLA.to_string(),
        anterior: None,
    };
    servidor(
        &mut app,
        &mut terminal,
        MensajeServidor::PideHuellaEscritorio {
            peticion_id: RANGO_ESCRITORIOS + 47,
            escritorio_id: 7,
            host_id,
            host_nombre: "win-01".to_string(),
            datos,
        },
    );
    assert!(matches!(app.dialogo, Some(Dialogo::Escritorio(_))));
    servidor(
        &mut app,
        &mut terminal,
        MensajeServidor::Escritorios { lista: Vec::new() },
    );
    assert!(app.dialogo.is_none(), "la pregunta no se retiró");
    assert!(app.peticiones_escritorio.is_empty());
    assert_eq!(app.escritorio_activo_id(), None);
}

/// m3: una respuesta de visor de una petición que ya no está en vuelo se
/// descarta (no abre el navegador ni copia nada por sorpresa).
#[test]
fn las_respuestas_de_peticiones_canceladas_se_descartan() {
    let entorno = entorno();
    let host_id = sembrar_rdp(&entorno);
    let (mut app, mut terminal, mut rx) = con_app(&entorno);
    abrir_con_pestana(&mut app, &mut terminal, &mut rx, host_id);
    // Se pide un enlace y la sesión desaparece antes de contestar.
    tecla(&mut app, &mut terminal, KeyCode::Enter);
    let peticion = enviados(&mut rx)
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::PedirEnlace { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("sin PedirEnlace");
    servidor(
        &mut app,
        &mut terminal,
        MensajeServidor::Escritorios { lista: Vec::new() },
    );
    assert!(app.peticiones_escritorio.is_empty());
    // La respuesta llega tarde: no se hace nada.
    servidor(
        &mut app,
        &mut terminal,
        MensajeServidor::Enlace {
            peticion_id: peticion,
            escritorio_id: 7,
            enlace: "http://127.0.0.1:8080/entrar?t=tarde".to_string(),
        },
    );
    assert!(app.enlace_visible.is_none(), "se abrió un enlace caducado");
    assert!(
        app.mensaje
            .as_ref()
            .is_none_or(|mensaje| !mensaje.texto.contains("enlace")),
        "{:?}",
        app.mensaje
    );
    // Y un EscritorioAbierto con un peticion_id que nadie espera tampoco.
    servidor(
        &mut app,
        &mut terminal,
        MensajeServidor::EscritorioAbierto {
            peticion_id: peticion + 1,
            escritorio_id: 7,
            enlace: "http://127.0.0.1:8080/entrar?t=tarde2".to_string(),
        },
    );
    assert!(app.enlace_visible.is_none());
}

/// Ruta del entorno, para no dejar el helper sin uso.
#[allow(dead_code)]
fn ruta_temporal(entorno: &Entorno) -> PathBuf {
    entorno.rutas.datos.clone()
}
