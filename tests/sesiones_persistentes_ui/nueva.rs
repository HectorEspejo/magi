//! Pruebas de la interfaz de las sesiones persistentes: diálogo «Nueva sesión
//! persistente» (pista A2 del Sprint 3), con `App::de_prueba` (sin hilos, red
//! ni servidor) y `Cliente::de_prueba`, que captura lo que se manda.
//!
//! Cubren el formulario (campos, validación mientras se escribe, pegado,
//! teclas, el desplegable de hosts), que crear manda `AbrirSesion` con
//! `PeticionTmux{crear}` sin guardar todavía la ficha, que la ficha y la
//! anotación `sesion_persistente_creada` aparecen solo cuando la pestaña
//! existe, que un fallo del servidor no deja ficha ni pendiente, el nombre
//! ocupado (engancharse a la existente o cambiar el nombre), la ficha de un
//! host marcado, reabrir una terminada, la caducidad de lo pendiente y las
//! instantáneas a 40×12, 80×24 y 200×60.

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use magi::app::persistentes::DialogoPersistentes;
use magi::app::persistentes_nueva::{
    CampoNueva, ClaseCreacion, CreacionPendiente, EstadoOcupado, FormularioNueva,
    CADUCIDAD_CON_APERTURA, CADUCIDAD_CREACION,
};
use magi::app::{App, Dialogo, Evento};
use magi::config::{Config, Rutas};
use magi::escritorio::TipoHost;
use magi::modelo::{DatosHost, Origen};
use magi::protocolo::{
    CausaAperturaTmux, CrearTmux, EstadoFicha, EstadoSesionRemota, FichaVista, InfoSesion,
    ListaPersistentes, MensajeCliente, MensajeServidor, PeticionTmux, ServidorTmux, TmuxPestana,
};
use magi::registro::FiltroRegistro;
use magi::tema::Tema;
use magi::ui::Vista;

use magi_nucleo::sesiones_persistentes::{DatosFicha, Ficha, OrigenFicha};

struct Entorno {
    _temporal: tempfile::TempDir,
    rutas: Rutas,
    config: Config,
}

fn entorno() -> Entorno {
    let temporal = tempfile::Builder::new()
        .prefix("magipers")
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

struct Prueba {
    app: App,
    terminal: Terminal<TestBackend>,
    enviados: tokio::sync::mpsc::UnboundedReceiver<MensajeCliente>,
    _entorno: Entorno,
}

impl Prueba {
    /// App con estos hosts `ssh` (en 192.0.2.x) y, si se pide, un escritorio.
    fn con_hosts(ssh: &[&str], escritorio: bool) -> Self {
        Self::con_inventario(|almacen| {
            for (indice, nombre) in ssh.iter().enumerate() {
                almacen
                    .crear_host(
                        &DatosHost {
                            nombre: (*nombre).to_string(),
                            direccion: format!("192.0.2.{}", 10 + indice),
                            usuario: Some("usuario".to_string()),
                            ..DatosHost::default()
                        },
                        Origen::Manual,
                    )
                    .expect("host de prueba");
            }
            if escritorio {
                almacen
                    .crear_host(
                        &DatosHost {
                            nombre: "win-01".to_string(),
                            direccion: "192.0.2.30".to_string(),
                            puerto: 3389,
                            tipo: TipoHost::Rdp,
                            ..DatosHost::default()
                        },
                        Origen::Manual,
                    )
                    .expect("escritorio de prueba");
            }
        })
    }

    fn con_inventario(tocar: impl FnOnce(&magi::almacen::Almacen)) -> Self {
        let entorno = entorno();
        {
            let almacen = magi::almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
            tocar(&almacen);
        }
        let almacen = magi::almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
        let (app, enviados) = App::de_prueba(
            entorno.rutas.clone(),
            entorno.config.clone(),
            Tema::respaldo(),
            almacen,
        )
        .expect("app de prueba");
        let terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        let mut prueba = Self {
            app,
            terminal,
            enviados,
            _entorno: entorno,
        };
        prueba
            .app
            .iniciar_pintado(&mut prueba.terminal)
            .expect("pintado");
        prueba
    }

    fn host_id(&self, nombre: &str) -> i64 {
        self.app
            .hosts
            .iter()
            .find(|host| host.nombre == nombre)
            .map(|host| host.id)
            .unwrap_or_else(|| panic!("host {nombre} no sembrado"))
    }

    fn paso(&mut self) {
        self.app
            .paso(&mut self.terminal, Instant::now())
            .expect("paso");
    }

    fn tecla(&mut self, codigo: KeyCode) {
        self.app
            .procesar(Evento::Tecla(KeyEvent::new(codigo, KeyModifiers::NONE)));
        self.paso();
    }

    /// Cada carácter, como una tecla suelta.
    fn escribir(&mut self, texto: &str) {
        for caracter in texto.chars() {
            self.tecla(KeyCode::Char(caracter));
        }
    }

    fn servidor(&mut self, mensaje: MensajeServidor) {
        self.app.procesar(Evento::Servidor(mensaje));
        self.paso();
    }

    fn drenar(&mut self) -> Vec<MensajeCliente> {
        let mut mensajes = Vec::new();
        while let Ok(mensaje) = self.enviados.try_recv() {
            mensajes.push(mensaje);
        }
        mensajes
    }

    /// Las `AbrirSesion` con tmux que se mandaron, con su petición.
    fn aperturas_tmux(&mut self) -> Vec<(i64, PeticionTmux)> {
        self.drenar()
            .into_iter()
            .filter_map(|mensaje| match mensaje {
                MensajeCliente::AbrirSesion {
                    host_id,
                    tmux: Some(tmux),
                    ..
                } => Some((host_id, tmux)),
                _ => None,
            })
            .collect()
    }

    fn formulario(&self) -> &FormularioNueva {
        match &self.app.dialogo {
            Some(Dialogo::Persistentes(DialogoPersistentes::Nueva(formulario))) => formulario,
            otro => panic!(
                "se esperaba el diálogo de nueva sesión y hay {}",
                if otro.is_some() { "otro" } else { "ninguno" }
            ),
        }
    }

    fn hay_dialogo(&self) -> bool {
        matches!(
            self.app.dialogo,
            Some(Dialogo::Persistentes(DialogoPersistentes::Nueva(_)))
        )
    }

    fn fichas(&self) -> Vec<Ficha> {
        magi::almacen::sesiones_persistentes::listar(self.app.almacen.conexion()).expect("fichas")
    }

    fn sembrar_ficha(&self, host: &str, datos: DatosFicha) {
        magi::almacen::sesiones_persistentes::crear(
            self.app.almacen.conexion(),
            self.host_id(host),
            &datos,
        )
        .expect("ficha sembrada");
    }

    fn registro(&self, tipo: &str) -> Vec<magi::modelo::EntradaRegistro> {
        self.app
            .almacen
            .listar_registro(&FiltroRegistro::default(), 100, 0)
            .expect("registro")
            .into_iter()
            .filter(|entrada| entrada.tipo == tipo)
            .collect()
    }

    fn texto(&self) -> String {
        let buffer = self.terminal.backend().buffer();
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

    fn instantanea(&self, nombre: &str) {
        let texto = self.texto();
        let mut ajustes = insta::Settings::clone_current();
        ajustes.set_snapshot_path("../snapshots");
        ajustes.set_prepend_module_to_snapshot(false);
        ajustes.set_omit_expression(true);
        ajustes.bind(|| insta::assert_snapshot!(nombre, texto));
    }

    fn redimensionar(&mut self, cols: u16, filas: u16) {
        self.terminal.backend_mut().resize(cols, filas);
        self.app
            .procesar_en(Evento::Redimension(cols, filas), Instant::now());
        self.app
            .paso(
                &mut self.terminal,
                Instant::now() + Duration::from_millis(60),
            )
            .expect("paso");
    }
}

fn datos_de_ficha(nombre: &str) -> DatosFicha {
    DatosFicha {
        nombre: nombre.to_string(),
        carpeta: Some("~/proyectos/api".to_string()),
        comando: Some("claude".to_string()),
        comando_reanudar: Some("claude --continue".to_string()),
        nota: Some("refactor del módulo de pagos".to_string()),
        origen: OrigenFicha::Manual,
        creada_por: "portatil".to_string(),
    }
}

fn fallida(host_id: i64, nombre: &str, causa: CausaAperturaTmux, mensaje: &str) -> MensajeServidor {
    MensajeServidor::AperturaTmuxFallida {
        host_id,
        nombre: nombre.to_string(),
        causa,
        mensaje: mensaje.to_string(),
    }
}

/// La lista del servidor con una sola ficha en este estado.
fn lista_con_ficha(host_id: i64, nombre: &str, estado: EstadoFicha) -> MensajeServidor {
    MensajeServidor::SesionesPersistentes {
        lista: ListaPersistentes {
            fichas: vec![FichaVista {
                uuid: "00000000-0000-4000-8000-000000000001".to_string(),
                host_id,
                nombre: nombre.to_string(),
                estado,
                sesion: None,
                comprobada_en: 1_700_000_000,
            }],
            hosts: Vec::new(),
        },
    }
}

/// Abre el diálogo desde Flota (la vista inicial de la app de pruebas) con la
/// `N` de la vista.
fn abrir_desde_flota(prueba: &mut Prueba) {
    prueba.tecla(KeyCode::Char('N'));
    assert!(prueba.hay_dialogo(), "N en Flota abre el diálogo");
}

// ------------------------------------------------------------------ entradas

#[test]
fn la_tecla_n_mayuscula_de_flota_abre_el_dialogo_con_el_host_de_la_fila() {
    let mut prueba = Prueba::con_hosts(&["dev-01", "web-01"], false);
    let propuesto = prueba
        .app
        .host_flota_seleccionado()
        .map(|host| (host.id, host.nombre.clone()))
        .expect("host seleccionado");
    abrir_desde_flota(&mut prueba);
    let formulario = prueba.formulario();
    assert_eq!(formulario.host_id(), Some(propuesto.0));
    assert_eq!(formulario.host_nombre(), propuesto.1);
    assert_eq!(formulario.foco(), CampoNueva::Nombre);
}

#[test]
fn la_tecla_n_mayuscula_de_hosts_abre_el_dialogo_con_el_host_de_la_fila() {
    let mut prueba = Prueba::con_hosts(&["dev-01", "web-01"], false);
    prueba.tecla(KeyCode::F(2));
    let web = prueba.host_id("web-01");
    prueba.app.seleccionar_host_id(web);
    prueba.tecla(KeyCode::Char('N'));
    assert!(prueba.hay_dialogo());
    assert_eq!(prueba.formulario().host_id(), Some(web));
    assert_eq!(prueba.formulario().host_nombre(), "web-01");
}

#[test]
fn un_escritorio_no_tiene_sesiones_persistentes() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], true);
    prueba.tecla(KeyCode::F(2));
    let escritorio = prueba.host_id("win-01");
    prueba.app.seleccionar_host_id(escritorio);
    prueba.tecla(KeyCode::Char('N'));
    assert!(!prueba.hay_dialogo(), "un escritorio no abre el diálogo");
    let mensaje = prueba.app.mensaje.as_ref().expect("aviso");
    assert!(mensaje.error);
    assert!(
        mensaje.texto.contains("escritorio"),
        "aviso: {}",
        mensaje.texto
    );
}

#[test]
fn sin_hosts_ssh_el_dialogo_no_se_abre_y_dice_por_que() {
    let mut prueba = Prueba::con_hosts(&[], true);
    prueba.app.abrir_dialogo_nueva_sesion(None);
    assert!(!prueba.hay_dialogo());
    let mensaje = prueba.app.mensaje.as_ref().expect("aviso");
    assert!(mensaje.error);
    assert!(
        mensaje.texto.contains("ningún host SSH"),
        "aviso: {}",
        mensaje.texto
    );
}

#[test]
fn desde_la_paleta_sin_host_se_propone_el_primero_y_el_foco_va_al_host() {
    let mut prueba = Prueba::con_hosts(&["dev-01", "web-01"], false);
    prueba.app.abrir_dialogo_nueva_sesion(None);
    let formulario = prueba.formulario();
    assert!(formulario.host_id().is_some());
    assert_eq!(formulario.foco(), CampoNueva::Host);
}

#[test]
fn el_host_dado_gana_aunque_no_sea_el_primero() {
    let mut prueba = Prueba::con_hosts(&["dev-01", "web-01"], false);
    let web = prueba.host_id("web-01");
    prueba.app.abrir_dialogo_nueva_sesion(Some(web));
    assert_eq!(prueba.formulario().host_id(), Some(web));
}

#[test]
fn la_paleta_abre_el_dialogo() {
    let mut prueba = Prueba::con_hosts(&["dev-01", "web-01"], false);
    prueba.app.procesar(Evento::Tecla(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::CONTROL,
    )));
    prueba.escribir("nueva sesion persistente");
    prueba.tecla(KeyCode::Enter);
    assert!(
        prueba.hay_dialogo(),
        "la paleta abre «nueva sesión persistente»"
    );
    assert!(prueba.formulario().host_id().is_some());
}

#[test]
fn el_prefijo_n_de_una_pestana_abre_el_dialogo_con_el_host_de_la_pestana() {
    let mut prueba = Prueba::con_hosts(&["dev-01", "web-01"], false);
    let web = prueba.host_id("web-01");
    prueba.servidor(MensajeServidor::Bienvenida {
        version: magi::protocolo::VERSION_PROTOCOLO,
        version_app: "1.5.0".to_string(),
        pid: 4242,
        cliente_id: 1,
        clientes: 1,
        sesiones: vec![InfoSesion {
            id: 1,
            nombre: "web-01".to_string(),
            host_id: web,
            host_nombre: "web-01".to_string(),
            estado: EstadoSesionRemota::Abierta,
            motivo: None,
            identidad: "clave".to_string(),
            abierta_en: chrono::Utc::now().timestamp() - 60,
            ventanas: 1,
            actividad_no_vista: false,
            reconexion: None,
            titulo: None,
            aviso: None,
            tmux: None,
            escritura_varios: false,
        }],
        transferencias: Vec::new(),
        tuneles: Vec::new(),
        ejecuciones: Vec::new(),
        acciones_servicio: Vec::new(),
        tuneles_temporales: Vec::new(),
    });
    prueba.tecla(KeyCode::F(3));
    prueba.tecla(KeyCode::Enter);
    assert_eq!(prueba.app.vista, Vista::Sesion);
    prueba.app.procesar(Evento::Tecla(KeyEvent::new(
        KeyCode::Char(']'),
        KeyModifiers::CONTROL,
    )));
    prueba.tecla(KeyCode::Char('N'));
    assert!(prueba.hay_dialogo(), "prefijo N abre el diálogo");
    assert_eq!(prueba.formulario().host_id(), Some(web));
}

// ---------------------------------------------------------------- formulario

#[test]
fn el_nombre_propuesto_sigue_a_la_carpeta_hasta_que_se_escribe_el_nombre() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    abrir_desde_flota(&mut prueba);
    assert_eq!(prueba.formulario().texto(CampoNueva::Nombre), "sesion-1");

    // La carpeta propone su última componente.
    prueba.tecla(KeyCode::Tab);
    assert_eq!(prueba.formulario().foco(), CampoNueva::Carpeta);
    prueba.escribir("~/proyectos/api");
    assert_eq!(prueba.formulario().texto(CampoNueva::Nombre), "api");

    // Si se escribe el nombre, deja de proponerse.
    prueba.tecla(KeyCode::BackTab);
    assert_eq!(prueba.formulario().foco(), CampoNueva::Nombre);
    prueba.tecla(KeyCode::End);
    prueba.escribir("-v2");
    assert_eq!(prueba.formulario().texto(CampoNueva::Nombre), "api-v2");
    prueba.tecla(KeyCode::Tab);
    prueba.escribir("/web");
    assert_eq!(
        prueba.formulario().texto(CampoNueva::Nombre),
        "api-v2",
        "el nombre escrito a mano no se vuelve a proponer"
    );
}

#[test]
fn el_nombre_propuesto_evita_los_ya_usados_en_el_host() {
    let mut prueba = Prueba::con_hosts(&["dev-01", "web-01"], false);
    prueba.sembrar_ficha("dev-01", datos_de_ficha("sesion-1"));
    prueba.sembrar_ficha("dev-01", datos_de_ficha("sesion-2"));
    let dev = prueba.host_id("dev-01");
    prueba.app.abrir_dialogo_nueva_sesion(Some(dev));
    assert_eq!(prueba.formulario().texto(CampoNueva::Nombre), "sesion-3");

    // Carpeta que coincide con una ficha: no se repite el nombre.
    prueba.tecla(KeyCode::Tab);
    prueba.escribir("/home/usuario/sesion-1");
    assert_eq!(prueba.formulario().texto(CampoNueva::Nombre), "sesion-3");
}

#[test]
fn tab_recorre_los_campos_y_cada_uno_recibe_lo_escrito() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    abrir_desde_flota(&mut prueba);
    let orden = [
        CampoNueva::Nombre,
        CampoNueva::Carpeta,
        CampoNueva::Comando,
        CampoNueva::Reanudar,
        CampoNueva::Nota,
        CampoNueva::Host,
        CampoNueva::Nombre,
    ];
    assert_eq!(prueba.formulario().foco(), orden[0]);
    for esperado in &orden[1..] {
        prueba.tecla(KeyCode::Tab);
        assert_eq!(prueba.formulario().foco(), *esperado);
    }
    // Y hacia atrás.
    prueba.tecla(KeyCode::BackTab);
    assert_eq!(prueba.formulario().foco(), CampoNueva::Host);

    prueba.tecla(KeyCode::Tab); // Nombre
    prueba.tecla(KeyCode::Tab); // Carpeta
    prueba.escribir("~/proyectos/api");
    prueba.tecla(KeyCode::Tab);
    prueba.escribir("claude");
    prueba.tecla(KeyCode::Tab);
    prueba.escribir("claude --continue");
    prueba.tecla(KeyCode::Down); // ↓ también baja de campo
    prueba.escribir("refactor");
    let formulario = prueba.formulario();
    assert_eq!(formulario.texto(CampoNueva::Carpeta), "~/proyectos/api");
    assert_eq!(formulario.texto(CampoNueva::Comando), "claude");
    assert_eq!(formulario.texto(CampoNueva::Reanudar), "claude --continue");
    assert_eq!(formulario.texto(CampoNueva::Nota), "refactor");
}

#[test]
fn la_validacion_ensena_el_error_mientras_se_escribe_y_lo_retira_al_corregir() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    abrir_desde_flota(&mut prueba);
    assert_eq!(prueba.formulario().error(), None);

    // Un espacio dentro del nombre.
    prueba.tecla(KeyCode::End);
    prueba.escribir(" uno");
    let error = prueba.formulario().error().expect("error de nombre");
    assert!(error.contains("espacios"), "error: {error}");
    assert!(
        prueba.texto().contains("espacios"),
        "el error está a la vista"
    );

    // Corregido, desaparece.
    for _ in 0..4 {
        prueba.tecla(KeyCode::Backspace);
    }
    assert_eq!(prueba.formulario().error(), None);

    // Una carpeta relativa.
    prueba.tecla(KeyCode::Tab);
    prueba.escribir("proyectos/api");
    let error = prueba.formulario().error().expect("error de carpeta");
    assert!(error.contains("ruta absoluta"), "error: {error}");

    // Un nombre vacío.
    prueba.tecla(KeyCode::BackTab);
    for _ in 0..12 {
        prueba.tecla(KeyCode::Backspace);
    }
    assert_eq!(prueba.formulario().texto(CampoNueva::Nombre), "");
    let error = prueba.formulario().error().expect("error de nombre vacío");
    assert!(error.contains("vacío"), "error: {error}");
}

#[test]
fn aceptar_con_un_error_lo_deja_a_la_vista_con_el_foco_en_el_campo() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    abrir_desde_flota(&mut prueba);
    prueba.tecla(KeyCode::Tab);
    prueba.escribir("relativa");
    prueba.tecla(KeyCode::Tab);
    prueba.tecla(KeyCode::Tab);
    prueba.tecla(KeyCode::Enter);
    assert!(
        prueba.hay_dialogo(),
        "con un error el diálogo sigue abierto"
    );
    assert_eq!(prueba.formulario().foco(), CampoNueva::Carpeta);
    assert!(prueba.formulario().error().is_some());
    assert!(prueba.aperturas_tmux().is_empty(), "no se manda nada");
    assert!(prueba.app.persistentes.creaciones.is_empty());
}

#[test]
fn el_comando_demasiado_largo_se_rechaza() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    abrir_desde_flota(&mut prueba);
    prueba.tecla(KeyCode::Tab);
    prueba.tecla(KeyCode::Tab);
    assert_eq!(prueba.formulario().foco(), CampoNueva::Comando);
    // El pegado llega acotado a 4096 caracteres: justo el límite, sin error.
    prueba.app.procesar(Evento::Pegado("a".repeat(5000)));
    assert_eq!(prueba.formulario().texto(CampoNueva::Comando).len(), 4096);
    assert_eq!(prueba.formulario().error(), None);
    // Un byte más y el comando no vale.
    prueba.tecla(KeyCode::Char('b'));
    let error = prueba.formulario().error().expect("comando largo");
    assert!(error.contains("4096"), "error: {error}");
}

#[test]
fn pegar_texto_va_al_campo_con_foco_y_no_al_host_cerrado() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    abrir_desde_flota(&mut prueba);
    prueba.tecla(KeyCode::Tab);
    prueba
        .app
        .procesar(Evento::Pegado("~/proyectos/api\ny esto no".to_string()));
    // Los saltos de línea del pegado se juntan: es un campo de una línea.
    assert_eq!(
        prueba.formulario().texto(CampoNueva::Carpeta),
        "~/proyectos/apiy esto no"
    );

    // Con el foco en el host cerrado, el pegado no cambia nada.
    prueba.tecla(KeyCode::Tab);
    prueba.tecla(KeyCode::Tab);
    prueba.tecla(KeyCode::Tab);
    prueba.tecla(KeyCode::Tab);
    assert_eq!(prueba.formulario().foco(), CampoNueva::Host);
    let antes = prueba.formulario().texto(CampoNueva::Nombre).to_string();
    prueba.app.procesar(Evento::Pegado("otro".to_string()));
    assert_eq!(prueba.formulario().texto(CampoNueva::Nombre), antes);
}

#[test]
fn el_pegado_en_el_nombre_lo_valida_y_lo_da_por_escrito() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    abrir_desde_flota(&mut prueba);
    prueba.tecla(KeyCode::End);
    prueba.app.procesar(Evento::Pegado("a b".to_string()));
    assert!(prueba.formulario().error().is_some());
    prueba.tecla(KeyCode::Tab);
    prueba.app.procesar(Evento::Pegado("~/x".to_string()));
    assert!(
        prueba
            .formulario()
            .texto(CampoNueva::Nombre)
            .ends_with("a b"),
        "el nombre pegado no se pisa con la propuesta de la carpeta"
    );
}

#[test]
fn el_desplegable_cambia_de_host_y_el_nombre_propuesto_lo_sigue() {
    let mut prueba = Prueba::con_hosts(&["dev-01", "web-01"], false);
    prueba.sembrar_ficha("web-01", datos_de_ficha("sesion-1"));
    let dev = prueba.host_id("dev-01");
    let web = prueba.host_id("web-01");
    prueba.app.abrir_dialogo_nueva_sesion(Some(dev));
    assert_eq!(prueba.formulario().texto(CampoNueva::Nombre), "sesion-1");

    prueba.tecla(KeyCode::BackTab);
    assert_eq!(prueba.formulario().foco(), CampoNueva::Host);
    prueba.tecla(KeyCode::Char(' '));
    assert!(prueba.formulario().desplegable().abierto);
    // `esc` solo cierra la lista.
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.hay_dialogo());
    assert!(!prueba.formulario().desplegable().abierto);
    prueba.tecla(KeyCode::Char(' '));
    prueba.escribir("web");
    prueba.tecla(KeyCode::Enter);
    assert!(!prueba.formulario().desplegable().abierto);
    assert_eq!(prueba.formulario().host_id(), Some(web));
    // web-01 ya tiene `sesion-1`: la propuesta pasa a `sesion-2`.
    assert_eq!(prueba.formulario().texto(CampoNueva::Nombre), "sesion-2");
}

#[test]
fn esc_cancela_el_dialogo_sin_mandar_nada() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    abrir_desde_flota(&mut prueba);
    prueba.escribir("x");
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.dialogo.is_none());
    assert!(prueba.drenar().is_empty());
    assert!(prueba.app.persistentes.creaciones.is_empty());
    assert!(prueba.fichas().is_empty());
}

// --------------------------------------------------------------------- crear

/// Rellena el formulario abierto (host `dev-01`) como el de la maqueta.
fn rellenar_como_la_maqueta(prueba: &mut Prueba) {
    prueba.tecla(KeyCode::End);
    for _ in 0..8 {
        prueba.tecla(KeyCode::Backspace);
    }
    prueba.escribir("agente-api");
    prueba.tecla(KeyCode::Tab);
    prueba.escribir("~/proyectos/api");
    prueba.tecla(KeyCode::Tab);
    prueba.escribir("claude");
    prueba.tecla(KeyCode::Tab);
    prueba.escribir("claude --continue");
    prueba.tecla(KeyCode::Tab);
    prueba.escribir("refactor del módulo de pagos");
}

#[test]
fn crear_manda_abrir_sesion_con_crear_y_no_guarda_la_ficha_todavia() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    let dev = prueba.host_id("dev-01");
    abrir_desde_flota(&mut prueba);
    // El nombre propuesto es `sesion-1`; se cambia (8 caracteres) por otro.
    rellenar_como_la_maqueta(&mut prueba);
    assert_eq!(prueba.formulario().texto(CampoNueva::Nombre), "agente-api");
    prueba.drenar();

    prueba.tecla(KeyCode::Enter);
    assert!(prueba.app.dialogo.is_none(), "crear cierra el diálogo");

    let aperturas = prueba.aperturas_tmux();
    assert_eq!(aperturas.len(), 1, "una sola apertura: {aperturas:?}");
    let (host_id, tmux) = &aperturas[0];
    assert_eq!(*host_id, dev);
    assert_eq!(
        *tmux,
        PeticionTmux {
            servidor: ServidorTmux::Magi,
            nombre: "agente-api".to_string(),
            crear: Some(CrearTmux {
                carpeta: Some("~/proyectos/api".to_string()),
                comando: Some("claude".to_string()),
            }),
            compartir: false,
        }
    );

    // Todavía no hay ficha ni anotación: la sesión aún no existe.
    assert!(prueba.fichas().is_empty(), "la ficha espera a la sesión");
    assert!(prueba.registro("sesion_persistente_creada").is_empty());
    assert!(prueba.app.aperturas_pendientes.contains(&dev));
    let pendientes = &prueba.app.persistentes.creaciones;
    assert_eq!(pendientes.len(), 1);
    assert_eq!(pendientes[0].host_id, dev);
    assert_eq!(pendientes[0].nombre, "agente-api");
    assert_eq!(pendientes[0].clase, ClaseCreacion::Nueva);
    assert_eq!(
        pendientes[0].comando_reanudar.as_deref(),
        Some("claude --continue")
    );
    assert_eq!(
        pendientes[0].nota.as_deref(),
        Some("refactor del módulo de pagos")
    );
}

#[test]
fn crear_manda_lo_normalizado_y_los_campos_vacios_como_none() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    abrir_desde_flota(&mut prueba);
    prueba.tecla(KeyCode::End);
    prueba.escribir("  "); // espacios al final del nombre: se recortan
    prueba.drenar();
    prueba.tecla(KeyCode::Enter);
    let aperturas = prueba.aperturas_tmux();
    assert_eq!(aperturas.len(), 1);
    let tmux = &aperturas[0].1;
    assert_eq!(tmux.nombre, "sesion-1");
    assert_eq!(
        tmux.crear,
        Some(CrearTmux {
            carpeta: None,
            comando: None
        }),
        "sin carpeta ni comando: shell de acceso en la carpeta personal"
    );
}

#[test]
fn la_ficha_y_la_anotacion_llegan_solo_cuando_la_pestana_existe() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    let dev = prueba.host_id("dev-01");
    abrir_desde_flota(&mut prueba);
    rellenar_como_la_maqueta(&mut prueba);
    prueba.tecla(KeyCode::Enter);
    prueba.drenar();
    assert!(prueba.fichas().is_empty());

    // Otra sesión del mismo host no es la pendiente.
    prueba.app.pestana_persistente_abierta(dev, "otra");
    assert!(prueba.fichas().is_empty());
    assert_eq!(prueba.app.persistentes.creaciones.len(), 1);

    prueba.app.pestana_persistente_abierta(dev, "agente-api");
    let fichas = prueba.fichas();
    assert_eq!(fichas.len(), 1, "la ficha se guarda al existir la sesión");
    let ficha = &fichas[0];
    assert_eq!(ficha.host_id, dev);
    assert_eq!(ficha.nombre, "agente-api");
    assert_eq!(ficha.carpeta.as_deref(), Some("~/proyectos/api"));
    assert_eq!(ficha.comando.as_deref(), Some("claude"));
    assert_eq!(ficha.comando_reanudar.as_deref(), Some("claude --continue"));
    assert_eq!(ficha.nota.as_deref(), Some("refactor del módulo de pagos"));
    assert_eq!(ficha.origen, OrigenFicha::Manual);
    assert!(
        !ficha.creada_por.is_empty()
            && ficha
                .creada_por
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
        "creada_por es la etiqueta del equipo: {}",
        ficha.creada_por
    );
    assert!(prueba.app.persistentes.creaciones.is_empty());

    // REGISTRO: host y nombre, sin comandos ni notas.
    let creadas = prueba.registro("sesion_persistente_creada");
    assert_eq!(creadas.len(), 1);
    let detalle = &creadas[0].detalle;
    assert!(detalle.contains("agente-api"), "detalle: {detalle}");
    assert!(detalle.contains("dev-01"), "detalle: {detalle}");
    assert!(!detalle.contains("claude"), "el comando no va: {detalle}");
    assert!(!detalle.contains("refactor"), "la nota no va: {detalle}");
    assert!(
        !detalle.contains("proyectos"),
        "la carpeta no va: {detalle}"
    );

    // La lista se refresca enseguida para que enseñe la ficha.
    let consultas = prueba
        .drenar()
        .into_iter()
        .filter(|mensaje| {
            matches!(
                mensaje,
                MensajeCliente::ConsultarSesionesPersistentes { .. }
            )
        })
        .count();
    assert_eq!(consultas, 1);

    // Una pestaña repetida no duplica nada.
    prueba.app.pestana_persistente_abierta(dev, "agente-api");
    assert_eq!(prueba.fichas().len(), 1);
    assert_eq!(prueba.registro("sesion_persistente_creada").len(), 1);
}

#[test]
fn un_fallo_de_apertura_deja_la_pendiente_limpia_y_sin_ficha() {
    let causas = [
        (
            CausaAperturaTmux::SinCarpeta,
            "la carpeta ~/x no existe en dev-01",
        ),
        (CausaAperturaTmux::SinTmux, "instala tmux en dev-01"),
        (
            CausaAperturaTmux::TmuxAntiguo,
            "tmux demasiado antiguo (3.0 o posterior)",
        ),
        (CausaAperturaTmux::Conexion, "no hay conexión con dev-01"),
        (CausaAperturaTmux::Otra, "no se pudo crear la sesión"),
    ];
    for (causa, mensaje) in causas {
        let mut prueba = Prueba::con_hosts(&["dev-01"], false);
        let dev = prueba.host_id("dev-01");
        abrir_desde_flota(&mut prueba);
        rellenar_como_la_maqueta(&mut prueba);
        prueba.tecla(KeyCode::Enter);
        prueba.drenar();
        assert_eq!(prueba.app.persistentes.creaciones.len(), 1);

        prueba.servidor(fallida(dev, "agente-api", causa, mensaje));

        assert!(
            prueba.app.persistentes.creaciones.is_empty(),
            "{causa:?}: la pendiente se quita"
        );
        assert!(
            !prueba.app.aperturas_pendientes.contains(&dev),
            "{causa:?}: el host queda libre"
        );
        assert!(prueba.fichas().is_empty(), "{causa:?}: no queda ficha");
        assert!(prueba.registro("sesion_persistente_creada").is_empty());
        // El mensaje con instrucción lo dice la barra (pista B): no hay
        // diálogo propio para estas causas.
        assert!(prueba.app.dialogo.is_none(), "{causa:?}");
        let texto = prueba
            .app
            .mensaje
            .as_ref()
            .map(|m| (m.texto.clone(), m.error))
            .expect("aviso");
        assert!(texto.1, "{causa:?}: es un error");
        assert!(
            texto.0.contains(mensaje),
            "{causa:?}: el aviso conserva el texto del servidor: {}",
            texto.0
        );

        // Una pestaña que aparezca después (otro camino) no crea la ficha.
        prueba.app.pestana_persistente_abierta(dev, "agente-api");
        assert!(prueba.fichas().is_empty(), "{causa:?}");
    }
}

#[test]
fn un_fallo_de_otro_nombre_no_toca_la_pendiente() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    let dev = prueba.host_id("dev-01");
    abrir_desde_flota(&mut prueba);
    rellenar_como_la_maqueta(&mut prueba);
    prueba.tecla(KeyCode::Enter);
    prueba.drenar();
    prueba.servidor(fallida(dev, "otra", CausaAperturaTmux::Otra, "no"));
    assert_eq!(prueba.app.persistentes.creaciones.len(), 1);
}

/// Una pestaña persistente tal como la difunde el servidor en `Sesiones`.
fn info_persistente(id: u32, host_id: i64, host: &str, nombre: &str) -> InfoSesion {
    InfoSesion {
        id,
        nombre: host.to_string(),
        host_id,
        host_nombre: host.to_string(),
        estado: EstadoSesionRemota::Abierta,
        motivo: None,
        identidad: "clave".to_string(),
        abierta_en: chrono::Utc::now().timestamp(),
        ventanas: 1,
        actividad_no_vista: false,
        reconexion: None,
        titulo: None,
        aviso: None,
        tmux: Some(TmuxPestana {
            servidor: ServidorTmux::Magi,
            nombre: nombre.to_string(),
            version: None,
            soltada: None,
        }),
        escritura_varios: false,
    }
}

/// La pestaña tal como la difunde el servidor al empezar a abrirla: sin tmux
/// todavía (`marcar_abierta` lo rellena una lista después).
fn info_abriendo(id: u32, host_id: i64, host: &str, nombre: &str) -> InfoSesion {
    InfoSesion {
        estado: EstadoSesionRemota::Abriendo,
        tmux: None,
        ..info_persistente(id, host_id, host, nombre)
    }
}

#[test]
fn la_lista_de_sesiones_con_la_pestana_nueva_guarda_la_ficha_y_la_anota() {
    // El camino de verdad: el servidor difunde `Sesiones` con la pestaña y la
    // reconciliación avisa a `pestana_persistente_abierta`.
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    let dev = prueba.host_id("dev-01");
    abrir_desde_flota(&mut prueba);
    rellenar_como_la_maqueta(&mut prueba);
    prueba.tecla(KeyCode::Enter);
    prueba.drenar();
    assert!(prueba.fichas().is_empty());

    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![info_persistente(7, dev, "dev-01", "agente-api")],
    });
    assert_eq!(prueba.app.vista, Vista::Sesion, "la pestaña nueva se abre");
    let fichas = prueba.fichas();
    assert_eq!(fichas.len(), 1);
    assert_eq!(fichas[0].nombre, "agente-api");
    assert_eq!(fichas[0].origen, OrigenFicha::Manual);
    assert_eq!(prueba.registro("sesion_persistente_creada").len(), 1);

    // Una difusión posterior con la misma pestaña no repite nada.
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![info_persistente(7, dev, "dev-01", "agente-api")],
    });
    assert_eq!(prueba.fichas().len(), 1);
    assert_eq!(prueba.registro("sesion_persistente_creada").len(), 1);
}

#[test]
fn si_otra_ventana_guarda_la_misma_ficha_a_la_vez_no_es_un_error_ni_se_anota_dos_veces() {
    // Defecto de la revisión adversarial: dos ventanas de este equipo ven a la
    // vez la pestaña nueva; la perdedora choca con `UNIQUE(host_id, nombre)` y
    // el usuario veía «la sesión ya existe en el host, pero no se pudo guardar
    // su ficha». Un trigger temporal hace de la otra ventana: inserta la misma
    // ficha justo antes de la nuestra.
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    let dev = prueba.host_id("dev-01");
    abrir_desde_flota(&mut prueba);
    rellenar_como_la_maqueta(&mut prueba);
    prueba.tecla(KeyCode::Enter);
    prueba.drenar();
    prueba
        .app
        .almacen
        .conexion()
        .execute_batch(
            "CREATE TEMP TRIGGER otra_ventana BEFORE INSERT ON SESIONES_PERSISTENTES
             WHEN NEW.nombre = 'agente-api'
             BEGIN
                 INSERT INTO SESIONES_PERSISTENTES
                     (host_id, nombre, origen, creada_por, creada_en, actualizado_en)
                 VALUES (NEW.host_id, NEW.nombre, 'manual', 'otra',
                         '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
             END;",
        )
        .expect("trigger de la otra ventana");

    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![info_abriendo(7, dev, "dev-01", "agente-api")],
    });
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![info_persistente(7, dev, "dev-01", "agente-api")],
    });
    // La pestaña se abrió y el cliente no se queja: la otra ventana ya la
    // guardó y a ella le toca anotarla.
    assert_eq!(prueba.app.vista, Vista::Sesion);
    let mensaje = prueba
        .app
        .mensaje
        .as_ref()
        .map(|mensaje| mensaje.texto.clone())
        .unwrap_or_default();
    assert!(
        !mensaje.contains("no se pudo guardar su ficha"),
        "{mensaje}"
    );
    assert!(prueba.registro("sesion_persistente_creada").is_empty());
    assert!(prueba.app.persistentes.creaciones.is_empty());
}

#[test]
fn la_lista_de_sesiones_con_una_pestana_de_un_host_marcado_crea_su_ficha() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    let dev = prueba.host_id("dev-01");
    prueba
        .app
        .almacen
        .fijar_sesiones_persistentes(dev, true)
        .expect("marcar el host");
    prueba.app.recargar_inventario().expect("recargar");
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![info_persistente(3, dev, "dev-01", "sobremesa-1")],
    });
    let fichas = prueba.fichas();
    assert_eq!(fichas.len(), 1);
    assert_eq!(fichas[0].nombre, "sobremesa-1");
    assert_eq!(fichas[0].origen, OrigenFicha::Host);
}

// ------------------------------------------------------------ nombre ocupado

#[test]
fn un_nombre_con_ficha_en_ese_host_avisa_y_ofrece_engancharse_o_cambiar_el_nombre() {
    // Criterio de aceptación: «Dado un nombre que ya existe en ese host, cuando
    // se acepta, entonces avisa y ofrece engancharse a la existente o cambiar
    // el nombre».
    let mut prueba = Prueba::con_hosts(&["dev-01", "web-01"], false);
    prueba.sembrar_ficha("dev-01", datos_de_ficha("agente-api"));
    let dev = prueba.host_id("dev-01");
    // La lista del servidor dice que la sesión de la ficha vive.
    prueba.servidor(lista_con_ficha(dev, "agente-api", EstadoFicha::VivaLibre));
    prueba.app.abrir_dialogo_nueva_sesion(Some(dev));
    rellenar_como_la_maqueta(&mut prueba);
    prueba.drenar();

    prueba.tecla(KeyCode::Enter);
    assert!(prueba.hay_dialogo(), "avisa en el propio diálogo");
    assert_eq!(prueba.formulario().ocupado(), Some(false));
    assert_eq!(
        prueba.formulario().estado_ocupado(),
        Some(EstadoOcupado::Viva)
    );
    let texto = prueba.texto();
    assert!(
        texto.contains("Ya hay una sesión «agente-api» en dev-01"),
        "{texto}"
    );
    assert!(texto.contains("engancharse"), "{texto}");
    assert!(texto.contains("cambiar el nombre"), "{texto}");
    assert!(prueba.drenar().is_empty(), "no se manda nada todavía");
    assert!(prueba.app.persistentes.creaciones.is_empty());

    // El mismo nombre en otro host no estorba.
    prueba.tecla(KeyCode::Esc);
    let web = prueba.host_id("web-01");
    prueba.app.abrir_dialogo_nueva_sesion(Some(web));
    prueba.tecla(KeyCode::End);
    for _ in 0..8 {
        prueba.tecla(KeyCode::Backspace);
    }
    prueba.escribir("agente-api");
    prueba.tecla(KeyCode::Enter);
    assert!(prueba.app.dialogo.is_none());
    assert_eq!(prueba.aperturas_tmux().len(), 1);
}

#[test]
fn con_el_nombre_ocupado_c_vuelve_al_formulario_con_los_datos() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    prueba.sembrar_ficha("dev-01", datos_de_ficha("agente-api"));
    abrir_desde_flota(&mut prueba);
    rellenar_como_la_maqueta(&mut prueba);
    prueba.tecla(KeyCode::Enter);
    assert_eq!(prueba.formulario().ocupado(), Some(false));

    prueba.tecla(KeyCode::Char('c'));
    assert_eq!(prueba.formulario().ocupado(), None);
    assert_eq!(prueba.formulario().foco(), CampoNueva::Nombre);
    assert_eq!(prueba.formulario().texto(CampoNueva::Nombre), "agente-api");
    assert_eq!(
        prueba.formulario().texto(CampoNueva::Carpeta),
        "~/proyectos/api"
    );
    assert_eq!(
        prueba.formulario().texto(CampoNueva::Nota),
        "refactor del módulo de pagos"
    );

    // Con otro nombre, crea.
    prueba.tecla(KeyCode::End);
    prueba.escribir("-2");
    prueba.drenar();
    prueba.tecla(KeyCode::Enter);
    let aperturas = prueba.aperturas_tmux();
    assert_eq!(aperturas.len(), 1);
    assert_eq!(aperturas[0].1.nombre, "agente-api-2");
    assert!(aperturas[0].1.crear.is_some());
}

#[test]
fn con_el_nombre_ocupado_intro_se_engancha_sin_crear_y_anota_al_existir_la_pestana() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    prueba.sembrar_ficha("dev-01", datos_de_ficha("agente-api"));
    let dev = prueba.host_id("dev-01");
    prueba.servidor(lista_con_ficha(dev, "agente-api", EstadoFicha::VivaLibre));
    abrir_desde_flota(&mut prueba);
    rellenar_como_la_maqueta(&mut prueba);
    prueba.tecla(KeyCode::Enter);
    prueba.drenar();

    prueba.tecla(KeyCode::Enter);
    assert!(prueba.app.dialogo.is_none());
    let aperturas = prueba.aperturas_tmux();
    assert_eq!(aperturas.len(), 1);
    assert_eq!(
        aperturas[0],
        (
            dev,
            PeticionTmux {
                servidor: ServidorTmux::Magi,
                nombre: "agente-api".to_string(),
                crear: None,
                compartir: false,
            }
        )
    );
    // Todavía no se ha anotado: el efecto no se ha producido.
    assert!(prueba.registro("sesion_persistente_enganchada").is_empty());

    prueba.app.pestana_persistente_abierta(dev, "agente-api");
    let anotadas = prueba.registro("sesion_persistente_enganchada");
    assert_eq!(anotadas.len(), 1);
    assert!(anotadas[0].detalle.contains("agente-api"));
    assert!(!anotadas[0].detalle.contains("claude"));
    assert_eq!(prueba.fichas().len(), 1, "la ficha existente no se toca");
    assert!(prueba.registro("sesion_persistente_creada").is_empty());
}

#[test]
fn si_el_host_dice_que_existe_se_ofrece_engancharse_o_cambiar_el_nombre_con_los_datos() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    let dev = prueba.host_id("dev-01");
    abrir_desde_flota(&mut prueba);
    rellenar_como_la_maqueta(&mut prueba);
    prueba.tecla(KeyCode::Enter);
    prueba.drenar();
    assert!(prueba.app.dialogo.is_none());

    prueba.servidor(fallida(
        dev,
        "agente-api",
        CausaAperturaTmux::Existe,
        "ya hay una sesión agente-api en dev-01",
    ));

    assert!(
        prueba.hay_dialogo(),
        "el aviso es del diálogo, no de la barra"
    );
    assert_eq!(prueba.formulario().ocupado(), Some(false));
    assert!(prueba.app.persistentes.creaciones.is_empty());
    assert!(!prueba.app.aperturas_pendientes.contains(&dev));
    assert!(prueba.fichas().is_empty(), "sin ficha ni anotación");
    assert!(prueba.registro("sesion_persistente_creada").is_empty());
    let texto = prueba.texto();
    assert!(
        texto.contains("Ya hay una sesión «agente-api» en dev-01"),
        "{texto}"
    );

    // Cambiar el nombre devuelve al formulario con todo lo escrito.
    prueba.tecla(KeyCode::Char('c'));
    assert_eq!(prueba.formulario().texto(CampoNueva::Nombre), "agente-api");
    assert_eq!(
        prueba.formulario().texto(CampoNueva::Carpeta),
        "~/proyectos/api"
    );
    assert_eq!(prueba.formulario().texto(CampoNueva::Comando), "claude");
    assert_eq!(
        prueba.formulario().texto(CampoNueva::Reanudar),
        "claude --continue"
    );
    assert_eq!(
        prueba.formulario().texto(CampoNueva::Nota),
        "refactor del módulo de pagos"
    );
}

#[test]
fn si_el_host_dice_que_existe_intro_se_engancha_sin_crear() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    let dev = prueba.host_id("dev-01");
    abrir_desde_flota(&mut prueba);
    rellenar_como_la_maqueta(&mut prueba);
    prueba.tecla(KeyCode::Enter);
    prueba.drenar();
    prueba.servidor(fallida(
        dev,
        "agente-api",
        CausaAperturaTmux::Existe,
        "existe",
    ));

    prueba.tecla(KeyCode::Enter);
    assert!(prueba.app.dialogo.is_none());
    let aperturas = prueba.aperturas_tmux();
    assert_eq!(aperturas.len(), 1);
    assert_eq!(aperturas[0].1.crear, None, "engancharse no crea");
    assert_eq!(aperturas[0].1.nombre, "agente-api");

    // La sesión existía y no era nuestra: sin ficha (el host no está marcado);
    // solo queda el hecho de haberse enganchado.
    prueba.app.pestana_persistente_abierta(dev, "agente-api");
    assert!(prueba.fichas().is_empty());
    assert_eq!(prueba.registro("sesion_persistente_enganchada").len(), 1);
    assert!(prueba.registro("sesion_persistente_creada").is_empty());
}

#[test]
fn con_el_nombre_ocupado_el_enganche_se_anota_con_la_secuencia_real_del_servidor() {
    // Defecto de la revisión adversarial: con el servidor real la pestaña llega
    // primero `Abriendo`, sin tmux, y el tmux una lista después. Vale tanto
    // para engancharse con `↵` desde el aviso como cuando el host dice
    // «existe» después de pedir crear.
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    prueba.sembrar_ficha("dev-01", datos_de_ficha("agente-api"));
    let dev = prueba.host_id("dev-01");
    prueba.servidor(lista_con_ficha(dev, "agente-api", EstadoFicha::VivaLibre));
    abrir_desde_flota(&mut prueba);
    rellenar_como_la_maqueta(&mut prueba);
    prueba.tecla(KeyCode::Enter);
    prueba.drenar();
    prueba.tecla(KeyCode::Enter);
    assert_eq!(prueba.aperturas_tmux().len(), 1);

    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![info_abriendo(7, dev, "dev-01", "agente-api")],
    });
    assert!(
        prueba.registro("sesion_persistente_enganchada").is_empty(),
        "la pestaña aún se abre: sin tmux no hay efecto que anotar"
    );
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![info_persistente(7, dev, "dev-01", "agente-api")],
    });
    let anotadas = prueba.registro("sesion_persistente_enganchada");
    assert_eq!(anotadas.len(), 1, "{anotadas:?}");
    assert!(anotadas[0].detalle.contains("agente-api"));
    assert!(!anotadas[0].detalle.contains("claude"));
    assert_eq!(prueba.fichas().len(), 1, "la ficha existente no se toca");
    assert!(prueba.registro("sesion_persistente_creada").is_empty());
    // Una difusión posterior (o una reconexión) no repite la anotación.
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![info_persistente(7, dev, "dev-01", "agente-api")],
    });
    assert_eq!(prueba.registro("sesion_persistente_enganchada").len(), 1);

    // El host dice «existe» a una creación pedida: se ofrece engancharse y,
    // al hacerlo, el enganche se anota con la misma secuencia.
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    let dev = prueba.host_id("dev-01");
    abrir_desde_flota(&mut prueba);
    rellenar_como_la_maqueta(&mut prueba);
    prueba.tecla(KeyCode::Enter);
    prueba.drenar();
    prueba.servidor(fallida(
        dev,
        "agente-api",
        CausaAperturaTmux::Existe,
        "existe",
    ));
    prueba.tecla(KeyCode::Enter);
    assert_eq!(prueba.aperturas_tmux().len(), 1);
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![info_abriendo(8, dev, "dev-01", "agente-api")],
    });
    assert!(prueba.registro("sesion_persistente_enganchada").is_empty());
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![info_persistente(8, dev, "dev-01", "agente-api")],
    });
    assert_eq!(prueba.registro("sesion_persistente_enganchada").len(), 1);
    assert!(
        prueba.fichas().is_empty(),
        "la sesión existía y no era nuestra: sin ficha"
    );
    assert!(prueba.registro("sesion_persistente_creada").is_empty());
}

#[test]
fn con_la_lista_sin_cargar_el_aviso_no_afirma_que_la_sesion_exista() {
    // Defecto de la revisión adversarial: la ficha local no sabe si su sesión
    // vive. Sin la lista del servidor (la ventana acaba de abrirse), el aviso
    // decía «Ya hay una sesión: engancharse» sobre una ficha terminada y `↵`
    // fallaba con «la sesión ya no existe».
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    prueba.sembrar_ficha("dev-01", datos_de_ficha("agente-api"));
    let dev = prueba.host_id("dev-01");
    assert!(prueba.app.persistentes.lista.is_none());
    abrir_desde_flota(&mut prueba);
    rellenar_como_la_maqueta(&mut prueba);
    prueba.tecla(KeyCode::Enter);
    assert_eq!(
        prueba.formulario().estado_ocupado(),
        Some(EstadoOcupado::SinComprobar)
    );
    let texto = prueba.texto();
    assert!(!texto.contains("Ya hay una sesión"), "{texto}");
    assert!(texto.contains("puede que su"), "{texto}");
    assert!(texto.contains("sesión exista"), "{texto}");
    assert!(texto.contains("abrirla"), "{texto}");
    assert!(prueba.drenar().is_empty(), "no se manda nada todavía");

    // `↵` la abre como una reapertura, que nunca pisa una sesión viva: la
    // ficha terminada se vuelve a crear con su comando.
    prueba.tecla(KeyCode::Enter);
    let aperturas = prueba.aperturas_tmux();
    assert_eq!(aperturas.len(), 1);
    assert_eq!(
        aperturas[0].1.crear,
        Some(CrearTmux {
            carpeta: Some("~/proyectos/api".to_string()),
            comando: Some("claude --continue".to_string()),
        })
    );
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![info_abriendo(7, dev, "dev-01", "agente-api")],
    });
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![info_persistente(7, dev, "dev-01", "agente-api")],
    });
    assert_eq!(prueba.registro("sesion_persistente_reabierta").len(), 1);
    assert!(prueba.registro("sesion_persistente_enganchada").is_empty());

    // Si la sesión sí existía, el servidor lo dice y entonces ya se sabe:
    // engancharse, nunca reabrir.
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    prueba.sembrar_ficha("dev-01", datos_de_ficha("agente-api"));
    let dev = prueba.host_id("dev-01");
    abrir_desde_flota(&mut prueba);
    rellenar_como_la_maqueta(&mut prueba);
    prueba.tecla(KeyCode::Enter);
    prueba.tecla(KeyCode::Enter);
    prueba.drenar();
    prueba.servidor(fallida(
        dev,
        "agente-api",
        CausaAperturaTmux::Existe,
        "existe",
    ));
    assert_eq!(
        prueba.formulario().estado_ocupado(),
        Some(EstadoOcupado::Viva)
    );
    assert!(
        prueba.texto().contains("Ya hay una sesión"),
        "{}",
        prueba.texto()
    );
    prueba.tecla(KeyCode::Enter);
    let aperturas = prueba.aperturas_tmux();
    assert_eq!(aperturas.len(), 1);
    assert_eq!(aperturas[0].1.crear, None, "engancharse no crea");
}

#[test]
fn con_la_lista_cargada_el_aviso_distingue_viva_terminada_y_ficha_ausente() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    prueba.sembrar_ficha("dev-01", datos_de_ficha("agente-api"));
    let dev = prueba.host_id("dev-01");
    let estado_con = |prueba: &mut Prueba| {
        prueba.app.abrir_dialogo_nueva_sesion(Some(dev));
        rellenar_como_la_maqueta(prueba);
        prueba.tecla(KeyCode::Enter);
        let estado = prueba.formulario().estado_ocupado();
        prueba.tecla(KeyCode::Esc);
        estado
    };
    // Una lista que no trae esa ficha tampoco sabe nada de ella.
    prueba.servidor(lista_con_ficha(dev, "otra", EstadoFicha::VivaLibre));
    assert_eq!(estado_con(&mut prueba), Some(EstadoOcupado::SinComprobar));
    for (estado, esperado) in [
        (EstadoFicha::VivaLibre, EstadoOcupado::Viva),
        (EstadoFicha::VivaEnganchada, EstadoOcupado::Viva),
        (EstadoFicha::Terminada, EstadoOcupado::Terminada),
        (EstadoFicha::SinComprobar, EstadoOcupado::SinComprobar),
        (EstadoFicha::Inalcanzable, EstadoOcupado::SinComprobar),
        (EstadoFicha::SinTmux, EstadoOcupado::SinComprobar),
    ] {
        prueba.servidor(lista_con_ficha(dev, "agente-api", estado));
        assert_eq!(estado_con(&mut prueba), Some(esperado), "{estado:?}");
    }
}

#[test]
fn el_aviso_de_existe_se_apila_sobre_otro_dialogo_abierto() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    let dev = prueba.host_id("dev-01");
    abrir_desde_flota(&mut prueba);
    prueba.tecla(KeyCode::Enter);
    prueba.drenar();
    // Mientras tanto el usuario abre otro diálogo (el de borrar un host).
    prueba.app.dialogo = Some(Dialogo::Confirmar {
        titulo: "OTRO".to_string(),
        lineas: vec!["otro".to_string()],
        peligro: false,
        accion: magi::app::AccionDialogo::BorrarHost(dev),
    });
    prueba.servidor(fallida(
        dev,
        "sesion-1",
        CausaAperturaTmux::Existe,
        "existe",
    ));
    assert!(prueba.hay_dialogo(), "el aviso va delante");
    prueba.tecla(KeyCode::Esc);
    assert!(
        matches!(prueba.app.dialogo, Some(Dialogo::Confirmar { .. })),
        "al cerrarlo vuelve el que estaba"
    );
}

#[test]
fn con_una_ficha_terminada_el_aviso_ofrece_reabrirla() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    prueba.sembrar_ficha("dev-01", datos_de_ficha("agente-api"));
    let dev = prueba.host_id("dev-01");
    prueba.servidor(lista_con_ficha(dev, "agente-api", EstadoFicha::Terminada));
    prueba.app.abrir_dialogo_nueva_sesion(Some(dev));
    prueba.tecla(KeyCode::End);
    for _ in 0..8 {
        prueba.tecla(KeyCode::Backspace);
    }
    prueba.escribir("agente-api");
    prueba.tecla(KeyCode::Enter);
    assert_eq!(prueba.formulario().ocupado(), Some(true));
    assert!(prueba.texto().contains("reabrirla"), "{}", prueba.texto());
    prueba.drenar();

    prueba.tecla(KeyCode::Enter);
    let aperturas = prueba.aperturas_tmux();
    assert_eq!(aperturas.len(), 1);
    assert_eq!(
        aperturas[0].1.crear,
        Some(CrearTmux {
            carpeta: Some("~/proyectos/api".to_string()),
            comando: Some("claude --continue".to_string()),
        }),
        "reabrir usa el comando de reanudación de la ficha"
    );
}

// -------------------------------------------------------------------- reabrir

#[test]
fn reabrir_usa_el_comando_de_reanudacion_y_solo_anota_al_existir_la_pestana() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    prueba.sembrar_ficha("dev-01", datos_de_ficha("agente-api"));
    let dev = prueba.host_id("dev-01");
    let antes = prueba.fichas()[0].clone();

    prueba.app.reabrir_ficha(dev, "agente-api");
    let aperturas = prueba.aperturas_tmux();
    assert_eq!(aperturas.len(), 1);
    assert_eq!(
        aperturas[0],
        (
            dev,
            PeticionTmux {
                servidor: ServidorTmux::Magi,
                nombre: "agente-api".to_string(),
                crear: Some(CrearTmux {
                    carpeta: Some("~/proyectos/api".to_string()),
                    comando: Some("claude --continue".to_string()),
                }),
                compartir: false,
            }
        )
    );
    assert!(
        prueba.registro("sesion_persistente_reabierta").is_empty(),
        "no se anota al pedirlo"
    );

    prueba.app.pestana_persistente_abierta(dev, "agente-api");
    // La lista se vuelve a pedir: la fila no puede seguir «terminada» con la
    // sesión ya viva.
    let consultas = prueba
        .drenar()
        .into_iter()
        .filter(|mensaje| {
            matches!(
                mensaje,
                MensajeCliente::ConsultarSesionesPersistentes { .. }
            )
        })
        .count();
    assert_eq!(consultas, 1);
    let anotadas = prueba.registro("sesion_persistente_reabierta");
    assert_eq!(anotadas.len(), 1);
    assert!(anotadas[0].detalle.contains("agente-api"));
    assert!(anotadas[0].detalle.contains("dev-01"));
    assert!(
        !anotadas[0].detalle.contains("claude"),
        "{}",
        anotadas[0].detalle
    );
    // La ficha sigue igual (misma versión) y no se creó otra.
    let despues = prueba.fichas();
    assert_eq!(despues.len(), 1);
    assert_eq!(despues[0], antes);
    assert!(prueba.registro("sesion_persistente_creada").is_empty());
}

#[test]
fn reabrir_sin_comando_de_reanudacion_usa_el_de_arranque() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    prueba.sembrar_ficha(
        "dev-01",
        DatosFicha {
            comando_reanudar: None,
            ..datos_de_ficha("agente-api")
        },
    );
    let dev = prueba.host_id("dev-01");
    prueba.app.reabrir_ficha(dev, "agente-api");
    let aperturas = prueba.aperturas_tmux();
    assert_eq!(
        aperturas[0].1.crear,
        Some(CrearTmux {
            carpeta: Some("~/proyectos/api".to_string()),
            comando: Some("claude".to_string()),
        })
    );
}

#[test]
fn reabrir_una_ficha_sin_comando_la_deja_con_el_shell_en_su_carpeta() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    prueba.sembrar_ficha(
        "dev-01",
        DatosFicha {
            comando: None,
            comando_reanudar: None,
            ..datos_de_ficha("agente-api")
        },
    );
    let dev = prueba.host_id("dev-01");
    prueba.app.reabrir_ficha(dev, "agente-api");
    let aperturas = prueba.aperturas_tmux();
    assert_eq!(
        aperturas[0].1.crear,
        Some(CrearTmux {
            carpeta: Some("~/proyectos/api".to_string()),
            comando: None,
        })
    );
}

#[test]
fn si_reabrir_falla_no_se_anota_nada() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    prueba.sembrar_ficha("dev-01", datos_de_ficha("agente-api"));
    let dev = prueba.host_id("dev-01");
    prueba.app.reabrir_ficha(dev, "agente-api");
    prueba.drenar();
    prueba.servidor(fallida(
        dev,
        "agente-api",
        CausaAperturaTmux::SinCarpeta,
        "la carpeta ~/proyectos/api no existe en dev-01",
    ));
    assert!(prueba.app.persistentes.creaciones.is_empty());
    assert!(prueba.registro("sesion_persistente_reabierta").is_empty());
    assert_eq!(prueba.fichas().len(), 1, "la ficha queda como estaba");
}

#[test]
fn reabrir_una_ficha_que_ya_no_existe_lo_dice_y_no_manda_nada() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    let dev = prueba.host_id("dev-01");
    prueba.app.reabrir_ficha(dev, "fantasma");
    assert!(prueba.drenar().is_empty());
    let mensaje = prueba.app.mensaje.as_ref().expect("aviso");
    assert!(mensaje.error);
    assert!(mensaje.texto.contains("fantasma"));
}

// ------------------------------------------------------------- host marcado

#[test]
fn en_un_host_marcado_la_pestana_sin_ficha_crea_la_ficha_con_origen_host() {
    let mut prueba = Prueba::con_hosts(&["dev-01", "web-01"], false);
    let dev = prueba.host_id("dev-01");
    prueba
        .app
        .almacen
        .fijar_sesiones_persistentes(dev, true)
        .expect("marcar el host");
    prueba.app.recargar_inventario().expect("recargar");

    // El servidor eligió el nombre automático: no hay creación pendiente.
    prueba.app.pestana_persistente_abierta(dev, "sobremesa-1");
    let fichas = prueba.fichas();
    assert_eq!(fichas.len(), 1);
    assert_eq!(fichas[0].nombre, "sobremesa-1");
    assert_eq!(fichas[0].origen, OrigenFicha::Host);
    assert_eq!(fichas[0].carpeta, None);
    assert_eq!(fichas[0].comando, None);
    let creadas = prueba.registro("sesion_persistente_creada");
    assert_eq!(creadas.len(), 1);
    assert!(creadas[0].detalle.contains("sobremesa-1"));
    assert!(creadas[0].detalle.contains("host"));

    // Sin duplicar si ya existe.
    prueba.app.pestana_persistente_abierta(dev, "sobremesa-1");
    assert_eq!(prueba.fichas().len(), 1);
    assert_eq!(prueba.registro("sesion_persistente_creada").len(), 1);

    // Otro nombre del mismo host marcado: otra ficha.
    prueba.app.pestana_persistente_abierta(dev, "sobremesa-2");
    assert_eq!(prueba.fichas().len(), 2);
}

#[test]
fn una_pestana_vieja_de_un_host_marcado_no_resucita_una_ficha_borrada() {
    // Revisión adversarial: al abrir una ventana (o al ver una pestaña que otra
    // ventana abrió hace rato), el servidor difunde las pestañas que ya tenía.
    // Si el usuario había borrado la ficha de una de ellas (la sesión sigue en
    // el host), no debe reaparecer con otra anotación «creada».
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    let dev = prueba.host_id("dev-01");
    prueba
        .app
        .almacen
        .fijar_sesiones_persistentes(dev, true)
        .expect("marcar el host");
    prueba.app.recargar_inventario().expect("recargar");

    let mut vieja = info_persistente(3, dev, "dev-01", "sobremesa-1");
    vieja.abierta_en = chrono::Utc::now().timestamp() - 3600;
    prueba.servidor(MensajeServidor::Sesiones { lista: vec![vieja] });
    assert!(
        prueba.fichas().is_empty(),
        "una pestaña abierta hace una hora no es una sesión recién creada"
    );
    assert!(prueba.registro("sesion_persistente_creada").is_empty());

    // La que el servidor acaba de abrir sí crea su ficha.
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![
            info_persistente(3, dev, "dev-01", "sobremesa-1"),
            info_persistente(4, dev, "dev-01", "sobremesa-2"),
        ],
    });
    let fichas = prueba.fichas();
    assert_eq!(fichas.len(), 1, "{fichas:?}");
    assert_eq!(fichas[0].nombre, "sobremesa-2");
    assert_eq!(prueba.registro("sesion_persistente_creada").len(), 1);
}

#[test]
fn si_otra_ventana_guarda_antes_la_ficha_automatica_lo_escrito_en_el_dialogo_no_se_pierde() {
    // Revisión adversarial: en un host marcado, dos ventanas de este equipo ven
    // a la vez la pestaña nueva. La que no la pidió guarda su ficha automática
    // (`origen = host`, vacía); la que la pidió con el diálogo no puede tirar
    // lo que el usuario escribió (comando de reanudación, nota…).
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    let dev = prueba.host_id("dev-01");
    prueba
        .app
        .almacen
        .fijar_sesiones_persistentes(dev, true)
        .expect("marcar el host");
    prueba.app.recargar_inventario().expect("recargar");
    // La etiqueta de este equipo, tal como la guarda el cliente.
    prueba.app.pestana_persistente_abierta(dev, "sonda");
    let equipo = prueba.fichas()[0].creada_por.clone();

    abrir_desde_flota(&mut prueba);
    rellenar_como_la_maqueta(&mut prueba);
    prueba.tecla(KeyCode::Enter);
    prueba.drenar();
    assert_eq!(prueba.app.persistentes.creaciones.len(), 1);
    // La otra ventana llegó antes a la base de datos.
    prueba.sembrar_ficha(
        "dev-01",
        DatosFicha {
            nombre: "agente-api".to_string(),
            carpeta: None,
            comando: None,
            comando_reanudar: None,
            nota: None,
            origen: OrigenFicha::Host,
            creada_por: equipo,
        },
    );
    prueba.app.pestana_persistente_abierta(dev, "agente-api");

    let ficha = prueba
        .fichas()
        .into_iter()
        .find(|ficha| ficha.nombre == "agente-api")
        .expect("la ficha");
    assert_eq!(ficha.carpeta.as_deref(), Some("~/proyectos/api"));
    assert_eq!(ficha.comando.as_deref(), Some("claude"));
    assert_eq!(ficha.comando_reanudar.as_deref(), Some("claude --continue"));
    assert_eq!(ficha.nota.as_deref(), Some("refactor del módulo de pagos"));
    assert_eq!(prueba.fichas().len(), 2, "no se duplica");
    assert!(prueba.app.persistentes.creaciones.is_empty());
    // Una ficha con datos propios (la de otro equipo, por ejemplo) no se toca.
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    let dev = prueba.host_id("dev-01");
    abrir_desde_flota(&mut prueba);
    rellenar_como_la_maqueta(&mut prueba);
    prueba.tecla(KeyCode::Enter);
    prueba.drenar();
    prueba.sembrar_ficha(
        "dev-01",
        DatosFicha {
            nota: Some("la de otro equipo".to_string()),
            ..datos_de_ficha("agente-api")
        },
    );
    prueba.app.pestana_persistente_abierta(dev, "agente-api");
    let ficha = &prueba.fichas()[0];
    assert_eq!(ficha.creada_por, "portatil");
    assert_eq!(ficha.nota.as_deref(), Some("la de otro equipo"));
}

/// La «otra ventana» de la prueba de la carrera: una segunda conexión con su
/// escritura a medias, que confirma cuando la de la `App` la espera.
static OTRA_VENTANA: std::sync::Mutex<Option<rusqlite::Connection>> = std::sync::Mutex::new(None);

fn la_otra_ventana_confirma(_intentos: std::os::raw::c_int) -> bool {
    let mut otra = OTRA_VENTANA.lock().expect("la otra ventana");
    match otra.take() {
        Some(conexion) => {
            conexion.execute_batch("COMMIT").expect("COMMIT");
            true
        }
        None => false,
    }
}

#[test]
fn si_la_otra_ventana_gana_la_carrera_justo_al_insertar_lo_escrito_en_el_dialogo_no_se_pierde() {
    // Revisión adversarial de C1: la otra ventana guarda su ficha automática
    // DESPUÉS de que esta mire si existe y ANTES de su `INSERT`. El choque con
    // `UNIQUE(host_id, nombre)` se toma como éxito silencioso (ni error ni
    // segunda anotación) y, sin volcar lo escrito en esa ficha, el comando de
    // reanudación y la nota del usuario se perdían sin decir nada.
    //
    // La carrera se reproduce sin azar: una segunda conexión tiene abierta su
    // escritura (la ficha, sin confirmar: esta ventana aún no la ve) y la
    // confirma cuando el `INSERT` de esta tiene que esperarla.
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    let dev = prueba.host_id("dev-01");
    prueba
        .app
        .almacen
        .fijar_sesiones_persistentes(dev, true)
        .expect("marcar el host");
    prueba.app.recargar_inventario().expect("recargar");
    // La etiqueta de este equipo, tal como la guarda el cliente.
    prueba.app.pestana_persistente_abierta(dev, "sonda");
    let equipo = prueba.fichas()[0].creada_por.clone();
    assert_eq!(prueba.registro("sesion_persistente_creada").len(), 1);

    abrir_desde_flota(&mut prueba);
    rellenar_como_la_maqueta(&mut prueba);
    prueba.tecla(KeyCode::Enter);
    prueba.drenar();
    assert_eq!(prueba.app.persistentes.creaciones.len(), 1);
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![info_abriendo(7, dev, "dev-01", "agente-api")],
    });

    let ruta = prueba
        .app
        .almacen
        .ruta()
        .expect("el almacén de la prueba es un fichero")
        .to_path_buf();
    let otra = rusqlite::Connection::open(ruta).expect("la otra ventana");
    otra.execute_batch(&format!(
        "BEGIN IMMEDIATE;
         INSERT INTO SESIONES_PERSISTENTES
             (host_id, nombre, origen, creada_por, creada_en, actualizado_en, uuid)
         VALUES ({dev}, 'agente-api', 'host', '{equipo}',
                 '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z',
                 '0b0b0b0b-0b0b-4b0b-8b0b-0b0b0b0b0b0b');"
    ))
    .expect("escritura a medias de la otra ventana");
    *OTRA_VENTANA.lock().expect("la otra ventana") = Some(otra);
    prueba
        .app
        .almacen
        .conexion()
        .busy_handler(Some(la_otra_ventana_confirma))
        .expect("manejador de espera");

    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![info_persistente(7, dev, "dev-01", "agente-api")],
    });
    assert!(
        OTRA_VENTANA.lock().expect("la otra ventana").is_none(),
        "el `INSERT` de esta ventana tuvo que esperar a la otra"
    );

    let ficha = prueba
        .fichas()
        .into_iter()
        .find(|ficha| ficha.nombre == "agente-api")
        .expect("la ficha de la otra ventana");
    assert_eq!(ficha.origen, OrigenFicha::Host, "la ficha es la suya");
    assert_eq!(ficha.carpeta.as_deref(), Some("~/proyectos/api"));
    assert_eq!(ficha.comando.as_deref(), Some("claude"));
    assert_eq!(ficha.comando_reanudar.as_deref(), Some("claude --continue"));
    assert_eq!(ficha.nota.as_deref(), Some("refactor del módulo de pagos"));
    assert_eq!(prueba.fichas().len(), 2, "no se duplica");
    // A quien la guardó le toca anotarla: aquí solo queda la de la sonda.
    assert_eq!(prueba.registro("sesion_persistente_creada").len(), 1);
    assert!(prueba.app.persistentes.creaciones.is_empty());
    let mensaje = prueba
        .app
        .mensaje
        .as_ref()
        .map(|mensaje| mensaje.texto.clone())
        .unwrap_or_default();
    assert!(
        !mensaje.contains("no se pudo guardar su ficha"),
        "{mensaje}"
    );
}

#[test]
fn una_pestana_de_un_host_marcado_con_su_ficha_ya_guardada_ni_la_duplica_ni_pregunta() {
    // Con varias ventanas, todas ven la pestaña nueva: las que llegan tarde no
    // intentan guardar otra ficha (ni piden al servidor que vuelva a mirar).
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    let dev = prueba.host_id("dev-01");
    prueba
        .app
        .almacen
        .fijar_sesiones_persistentes(dev, true)
        .expect("marcar el host");
    prueba.app.recargar_inventario().expect("recargar");
    prueba.sembrar_ficha(
        "dev-01",
        DatosFicha {
            origen: OrigenFicha::Host,
            ..datos_de_ficha("sobremesa-1")
        },
    );
    prueba.drenar();
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![info_abriendo(3, dev, "dev-01", "sobremesa-1")],
    });
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![info_persistente(3, dev, "dev-01", "sobremesa-1")],
    });
    assert_eq!(prueba.fichas().len(), 1);
    assert!(prueba.registro("sesion_persistente_creada").is_empty());
    assert!(
        !prueba.drenar().iter().any(|mensaje| matches!(
            mensaje,
            MensajeCliente::ConsultarSesionesPersistentes { .. }
        )),
        "no hay nada nuevo que pedir al servidor"
    );
}

#[test]
fn en_un_host_sin_marcar_una_pestana_sin_pendiente_no_crea_ficha() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    let dev = prueba.host_id("dev-01");
    prueba.app.pestana_persistente_abierta(dev, "descubierta");
    assert!(prueba.fichas().is_empty());
    assert!(prueba.registro("sesion_persistente_creada").is_empty());
    assert!(prueba.drenar().is_empty());
}

#[test]
fn una_ficha_ya_sincronizada_con_el_mismo_nombre_no_se_duplica_al_crear() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    let dev = prueba.host_id("dev-01");
    abrir_desde_flota(&mut prueba);
    rellenar_como_la_maqueta(&mut prueba);
    prueba.tecla(KeyCode::Enter);
    prueba.drenar();
    // Mientras se abría, otro equipo creó una ficha con ese nombre y llegó por
    // la sincronización.
    prueba.sembrar_ficha("dev-01", datos_de_ficha("agente-api"));
    prueba.app.pestana_persistente_abierta(dev, "agente-api");
    assert_eq!(prueba.fichas().len(), 1);
    assert!(prueba.registro("sesion_persistente_creada").is_empty());
    assert!(prueba.app.persistentes.creaciones.is_empty());
}

// ---------------------------------------------------------------- caducidad

fn pendiente(host_id: i64, nombre: &str, pedida_en: Instant) -> CreacionPendiente {
    CreacionPendiente {
        host_id,
        nombre: nombre.to_string(),
        clase: ClaseCreacion::Nueva,
        carpeta: None,
        comando: None,
        comando_reanudar: None,
        nota: None,
        pedida_en,
    }
}

#[test]
fn las_creaciones_pendientes_caducan_si_nunca_llega_la_pestana_ni_un_fallo() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    let dev = prueba.host_id("dev-01");
    let ahora = Instant::now();
    prueba
        .app
        .persistentes
        .creaciones
        .push(pendiente(dev, "vieja", ahora));

    assert_eq!(
        prueba
            .app
            .caducar_creaciones(ahora + Duration::from_secs(30)),
        0
    );
    assert_eq!(prueba.app.persistentes.creaciones.len(), 1);
    assert_eq!(
        prueba
            .app
            .caducar_creaciones(ahora + CADUCIDAD_CREACION + Duration::from_secs(1)),
        1
    );
    assert!(prueba.app.persistentes.creaciones.is_empty());

    // Y el tick de la app lo hace solo (con la hora de verdad no caduca).
    prueba
        .app
        .persistentes
        .creaciones
        .push(pendiente(dev, "vieja", Instant::now()));
    prueba.app.procesar(Evento::Tick);
    assert_eq!(prueba.app.persistentes.creaciones.len(), 1);
}

#[test]
fn con_la_conexion_en_curso_la_creacion_espera_mas_por_si_hay_un_dialogo_del_host() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    let dev = prueba.host_id("dev-01");
    let ahora = Instant::now();
    prueba
        .app
        .persistentes
        .creaciones
        .push(pendiente(dev, "lenta", ahora));
    prueba.app.aperturas_pendientes.insert(dev);

    // Pasado el minuto, el host sigue conectando (frase, huella): se espera.
    assert_eq!(
        prueba
            .app
            .caducar_creaciones(ahora + CADUCIDAD_CREACION + Duration::from_secs(1)),
        0
    );
    assert_eq!(
        prueba
            .app
            .caducar_creaciones(ahora + CADUCIDAD_CON_APERTURA + Duration::from_secs(1)),
        1
    );
}

#[test]
fn la_creacion_espera_a_un_dialogo_del_host_aunque_la_primera_lista_ya_lo_saque_de_en_curso() {
    // Revisión adversarial: el servidor difunde la pestaña en cuanto empieza a
    // abrirla (`Abriendo`, sin tmux) y esa primera lista quita al host de
    // `aperturas_pendientes`. Mientras el usuario contesta la huella o la
    // frase (hasta 5 min), la creación no puede caducar a los 60 s: la sesión
    // acabaría existiendo sin ficha y sin anotación, y se perderían lo escrito
    // en el diálogo (comando de reanudación, nota…).
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    let dev = prueba.host_id("dev-01");
    abrir_desde_flota(&mut prueba);
    rellenar_como_la_maqueta(&mut prueba);
    prueba.tecla(KeyCode::Enter);
    prueba.drenar();
    assert_eq!(prueba.app.persistentes.creaciones.len(), 1);

    let mut abriendo = info_persistente(7, dev, "dev-01", "agente-api");
    abriendo.estado = EstadoSesionRemota::Abriendo;
    abriendo.tmux = None;
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![abriendo],
    });
    assert!(
        !prueba.app.aperturas_pendientes.contains(&dev),
        "la primera lista saca al host de las aperturas en curso"
    );

    let ahora = Instant::now();
    assert_eq!(
        prueba
            .app
            .caducar_creaciones(ahora + CADUCIDAD_CREACION + Duration::from_secs(60)),
        0,
        "la pestaña sigue abriéndose: se espera al usuario"
    );
    assert_eq!(prueba.app.persistentes.creaciones.len(), 1);

    // El usuario contesta y la pestaña queda abierta con su sesión de tmux.
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![info_persistente(7, dev, "dev-01", "agente-api")],
    });
    let fichas = prueba.fichas();
    assert_eq!(fichas.len(), 1);
    assert_eq!(
        fichas[0].nota.as_deref(),
        Some("refactor del módulo de pagos")
    );
    assert_eq!(prueba.registro("sesion_persistente_creada").len(), 1);

    // Pero no es para siempre: pasados los 6 min, si la pestaña no abre, se tira.
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    let dev = prueba.host_id("dev-01");
    prueba
        .app
        .persistentes
        .creaciones
        .push(pendiente(dev, "colgada", Instant::now()));
    let mut abriendo = info_persistente(8, dev, "dev-01", "colgada");
    abriendo.estado = EstadoSesionRemota::Abriendo;
    abriendo.tmux = None;
    prueba.servidor(MensajeServidor::Sesiones {
        lista: vec![abriendo],
    });
    assert_eq!(
        prueba
            .app
            .caducar_creaciones(Instant::now() + CADUCIDAD_CON_APERTURA + Duration::from_secs(5)),
        1
    );
}

#[test]
fn una_pestana_tardia_sigue_guardando_la_ficha_aunque_la_espera_haya_pasado() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    let dev = prueba.host_id("dev-01");
    // Pedida hace dos minutos, con la conexión ya fuera de «en curso» (la
    // pestaña acaba de aparecer y se quitó): sigue valiendo, porque la
    // sesión ya existe en el host.
    let hace_dos_minutos = Instant::now()
        .checked_sub(Duration::from_secs(120))
        .unwrap_or_else(Instant::now);
    prueba
        .app
        .persistentes
        .creaciones
        .push(pendiente(dev, "tardia", hace_dos_minutos));
    prueba.app.pestana_persistente_abierta(dev, "tardia");
    let fichas = prueba.fichas();
    assert_eq!(fichas.len(), 1);
    assert_eq!(fichas[0].nombre, "tardia");
}

#[test]
fn dos_peticiones_del_mismo_host_y_nombre_dejan_una_sola_pendiente() {
    let mut prueba = Prueba::con_hosts(&["dev-01"], false);
    let dev = prueba.host_id("dev-01");
    prueba.app.reabrir_ficha(dev, "nada");
    prueba.sembrar_ficha("dev-01", datos_de_ficha("agente-api"));
    prueba.app.reabrir_ficha(dev, "agente-api");
    prueba.app.reabrir_ficha(dev, "agente-api");
    assert_eq!(prueba.app.persistentes.creaciones.len(), 1);
}

// ------------------------------------------------------------- instantáneas

/// El diálogo en tres estados (vacío, con un error de validación y con el
/// nombre ocupado) a 40×12, 80×24 y 200×60 (T45).
const TAMANOS: [(u16, u16); 3] = [(40, 12), (80, 24), (200, 60)];

fn con_dialogo_vacio() -> Prueba {
    let mut prueba = Prueba::con_hosts(&["dev-01", "web-01"], false);
    let dev = prueba.host_id("dev-01");
    prueba.app.abrir_dialogo_nueva_sesion(Some(dev));
    prueba.paso();
    prueba
}

#[test]
fn instantaneas_del_dialogo_vacio() {
    for (cols, filas) in TAMANOS {
        let mut prueba = con_dialogo_vacio();
        prueba.redimensionar(cols, filas);
        prueba.instantanea(&format!(
            "sesiones_persistentes_ui__nueva__nueva_vacia_{cols}x{filas}"
        ));
    }
}

#[test]
fn instantaneas_del_dialogo_relleno_como_la_maqueta() {
    for (cols, filas) in TAMANOS {
        let mut prueba = con_dialogo_vacio();
        prueba.redimensionar(cols, filas);
        rellenar_como_la_maqueta(&mut prueba);
        prueba.instantanea(&format!(
            "sesiones_persistentes_ui__nueva__nueva_rellena_{cols}x{filas}"
        ));
    }
}

#[test]
fn instantaneas_del_dialogo_con_un_error_de_validacion() {
    for (cols, filas) in TAMANOS {
        let mut prueba = con_dialogo_vacio();
        prueba.redimensionar(cols, filas);
        prueba.tecla(KeyCode::End);
        prueba.escribir(" uno");
        assert!(prueba.formulario().error().is_some());
        prueba.instantanea(&format!(
            "sesiones_persistentes_ui__nueva__nueva_con_error_{cols}x{filas}"
        ));
    }
}

#[test]
fn instantaneas_del_dialogo_con_el_nombre_ocupado() {
    for (cols, filas) in TAMANOS {
        let mut prueba = Prueba::con_hosts(&["dev-01", "web-01"], false);
        prueba.sembrar_ficha("dev-01", datos_de_ficha("agente-api"));
        let dev = prueba.host_id("dev-01");
        prueba.servidor(lista_con_ficha(dev, "agente-api", EstadoFicha::VivaLibre));
        prueba.app.abrir_dialogo_nueva_sesion(Some(dev));
        prueba.redimensionar(cols, filas);
        prueba.tecla(KeyCode::End);
        for _ in 0..8 {
            prueba.tecla(KeyCode::Backspace);
        }
        prueba.escribir("agente-api");
        prueba.tecla(KeyCode::Enter);
        assert_eq!(prueba.formulario().ocupado(), Some(false));
        assert_eq!(
            prueba.formulario().estado_ocupado(),
            Some(EstadoOcupado::Viva)
        );
        prueba.instantanea(&format!(
            "sesiones_persistentes_ui__nueva__nueva_ocupada_{cols}x{filas}"
        ));
    }
}

#[test]
fn instantaneas_del_dialogo_con_el_nombre_ocupado_sin_comprobar() {
    for (cols, filas) in TAMANOS {
        let mut prueba = Prueba::con_hosts(&["dev-01", "web-01"], false);
        prueba.sembrar_ficha("dev-01", datos_de_ficha("agente-api"));
        let dev = prueba.host_id("dev-01");
        prueba.app.abrir_dialogo_nueva_sesion(Some(dev));
        prueba.redimensionar(cols, filas);
        prueba.tecla(KeyCode::End);
        for _ in 0..8 {
            prueba.tecla(KeyCode::Backspace);
        }
        prueba.escribir("agente-api");
        prueba.tecla(KeyCode::Enter);
        assert_eq!(
            prueba.formulario().estado_ocupado(),
            Some(EstadoOcupado::SinComprobar)
        );
        prueba.instantanea(&format!(
            "sesiones_persistentes_ui__nueva__nueva_ocupada_sin_comprobar_{cols}x{filas}"
        ));
    }
}

#[test]
fn instantaneas_del_dialogo_con_el_desplegable_de_hosts_abierto() {
    for (cols, filas) in TAMANOS {
        let mut prueba = Prueba::con_hosts(&["dev-01", "web-01", "db-01"], false);
        let dev = prueba.host_id("dev-01");
        prueba.app.abrir_dialogo_nueva_sesion(Some(dev));
        prueba.redimensionar(cols, filas);
        prueba.tecla(KeyCode::BackTab);
        prueba.tecla(KeyCode::Char(' '));
        assert!(prueba.formulario().desplegable().abierto);
        prueba.instantanea(&format!(
            "sesiones_persistentes_ui__nueva__nueva_desplegable_{cols}x{filas}"
        ));
    }
}

#[test]
fn el_dialogo_se_ve_tambien_en_ascii() {
    let mut prueba = con_dialogo_vacio();
    prueba.app.tema.ascii = true;
    prueba.redimensionar(80, 24);
    prueba.tecla(KeyCode::End);
    prueba.escribir(" uno");
    let texto = prueba.texto();
    for glifo in ['▾', '✕', '┃', '◆'] {
        assert!(!texto.contains(glifo), "queda «{glifo}» en ASCII:\n{texto}");
    }
    assert!(texto.contains("NUEVA SESIÓN PERSISTENTE"), "{texto}");
}
