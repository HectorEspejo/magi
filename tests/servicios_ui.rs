//! Pruebas de la vista Servicios (Fase 13) sin hilos, red ni servidor:
//! App::de_prueba y el bucle real sobre un TestBackend.
//!
//! Cubren la apertura con F9 y con u, el orden de §7.1, las secciones y los
//! filtros, el detalle, la confirmación y la deliberación, sudo (3 intentos y
//! memoria), los bloqueos de tecla con motivo, el panel de logs (anillo,
//! pausa, filtros y PararLog) y «Toda la flota». Se inyectan los mensajes del
//! servidor con el peticion_id que el cliente acaba de enviar.

use std::collections::BTreeMap;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use magi::app::servicios::{DialogoServicio, EstadoListado, Foco, PestanaDetalle, Seccion};
use magi::app::{App, Evento};
use magi::config::{Config, Rutas};
use magi::deliberacion::LimitesDeConfig;
use magi::modelo::{DatosHost, Origen, ResultadoSondeo, Sondeo};
use magi::protocolo::{MensajeCliente, MensajeServidor, RANGO_SERVICIOS};
use magi::tema::Tema;
use magi::ui::Vista;
use magi_nucleo::servicios::lectura::ComprobacionSudo;
use magi_nucleo::servicios::{
    Accion, Contenedor, Detalle, EstadoContenedor, FallosContenedores, FallosSondeo,
    FallosUnidades, LineaLog, Listado, Motor, MotorEstado, MotorNombre, PermisoDiario, Privilegio,
    RelojHost, ResultadoAccion, ResultadoServicio, Salud, Systemd, Timer, Unidad,
};

struct Entorno {
    _temporal: tempfile::TempDir,
    rutas: Rutas,
    config: Config,
}

fn entorno() -> Entorno {
    let temporal = tempfile::Builder::new()
        .prefix("magisvc")
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
    /// App de prueba con los hosts dados (nombres) ya en el inventario y el
    /// refresco automático apagado, para que las pruebas sean deterministas.
    fn nueva(hosts: &[&str]) -> Self {
        Self::con_config(hosts, |config| {
            config.servicios.refresco_seg = 0;
        })
    }

    fn con_config(hosts: &[&str], tocar: impl FnOnce(&mut Config)) -> Self {
        let mut entorno = entorno();
        tocar(&mut entorno.config);
        {
            let almacen = magi::almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
            for nombre in hosts {
                almacen
                    .crear_host(
                        &DatosHost {
                            nombre: (*nombre).to_string(),
                            direccion: "192.0.2.10".to_string(),
                            ..DatosHost::default()
                        },
                        Origen::Manual,
                    )
                    .expect("host de prueba");
            }
        }
        let almacen = magi::almacen::abrir(&entorno.rutas.base_datos()).expect("almacén");
        let (mut app, enviados) = App::de_prueba(
            entorno.rutas.clone(),
            entorno.config.clone(),
            Tema::respaldo(),
            almacen,
        )
        .expect("app de prueba");
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        app.iniciar_pintado(&mut terminal).expect("pintado");
        Self {
            app,
            terminal,
            enviados,
            _entorno: entorno,
        }
    }

    fn host_id(&self, nombre: &str) -> i64 {
        self.app
            .hosts
            .iter()
            .find(|host| host.nombre == nombre)
            .map(|host| host.id)
            .unwrap_or_else(|| panic!("host {nombre} no sembrado"))
    }

    fn tecla(&mut self, codigo: KeyCode) {
        self.tecla_con(codigo, KeyModifiers::NONE);
    }

    fn tecla_con(&mut self, codigo: KeyCode, modificadores: KeyModifiers) {
        self.app
            .procesar(Evento::Tecla(KeyEvent::new(codigo, modificadores)));
        self.app
            .paso(&mut self.terminal, Instant::now())
            .expect("paso");
    }

    fn servidor(&mut self, mensaje: MensajeServidor) {
        self.app.procesar(Evento::Servidor(mensaje));
        self.app
            .paso(&mut self.terminal, Instant::now())
            .expect("paso");
    }

    fn drenar(&mut self) -> Vec<MensajeCliente> {
        let mut mensajes = Vec::new();
        while let Ok(mensaje) = self.enviados.try_recv() {
            mensajes.push(mensaje);
        }
        mensajes
    }

    fn peticiones_listar(&mut self) -> Vec<(u64, i64)> {
        self.drenar()
            .into_iter()
            .filter_map(|mensaje| match mensaje {
                MensajeCliente::ListarServicios {
                    peticion_id,
                    host_id,
                    ..
                } => Some((peticion_id, host_id)),
                _ => None,
            })
            .collect()
    }

    /// Responde el listado a la última petición pendiente de ese host.
    fn responder_listado(&mut self, host_id: i64, listado: Listado) -> u64 {
        let peticiones = self.peticiones_listar();
        let Some((peticion_id, _)) = peticiones
            .into_iter()
            .rev()
            .find(|(_, otro)| *otro == host_id)
        else {
            panic!("no había ListarServicios pendiente para el host");
        };
        self.servidor(MensajeServidor::Servicios {
            peticion_id,
            host_id,
            listado,
        });
        peticion_id
    }

    /// Abre la vista Servicios con el host elegido (u desde Hosts).
    fn abrir_con(&mut self, host_id: i64) {
        self.tecla(KeyCode::F(2));
        self.app.seleccionar_host_id(host_id);
        self.tecla(KeyCode::Char('u'));
        assert_eq!(self.app.vista, Vista::Servicios);
    }

    fn listado(&mut self, host_id: i64, listado: Listado) {
        self.abrir_con(host_id);
        self.responder_listado(host_id, listado);
    }

    fn seleccionar_nombre(&mut self, host_id: i64, nombre: &str) {
        let elementos = self.app.servicios_elementos(host_id);
        let indice = elementos
            .iter()
            .position(|elemento| elemento.nombre() == nombre)
            .unwrap_or_else(|| panic!("{nombre} no está en la lista"));
        self.app
            .servicios
            .seleccion
            .insert((host_id, self.app.servicios.seccion), indice);
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
}

// ---------------------------------------------------------------- piezas

fn unidad(nombre: &str, activo: &str, arranque: &str, con_recarga: bool) -> Unidad {
    let mut props = BTreeMap::new();
    props.insert("Description".to_string(), format!("{nombre} de prueba"));
    props.insert("ActiveState".to_string(), activo.to_string());
    props.insert(
        "SubState".to_string(),
        if activo == "failed" {
            "failed".to_string()
        } else {
            "running".to_string()
        },
    );
    props.insert("UnitFileState".to_string(), arranque.to_string());
    props.insert(
        "CanReload".to_string(),
        if con_recarga { "yes" } else { "no" }.to_string(),
    );
    props.insert("LoadState".to_string(), "loaded".to_string());
    props.insert("Result".to_string(), "success".to_string());
    props.insert("ExecMainPID".to_string(), "4121".to_string());
    props.insert("MemoryCurrent".to_string(), "104857600".to_string());
    props.insert(
        "FragmentPath".to_string(),
        format!("/etc/systemd/system/{nombre}"),
    );
    Unidad {
        nombre: nombre.to_string(),
        props,
        vigilada: false,
    }
}

fn vigilada(mut unidad: Unidad) -> Unidad {
    unidad.vigilada = true;
    unidad
}

fn timer(nombre: &str) -> Timer {
    let mut props = BTreeMap::new();
    props.insert("Description".to_string(), format!("{nombre} de prueba"));
    props.insert("ActiveState".to_string(), "active".to_string());
    props.insert(
        "NextElapseUSecRealtime".to_string(),
        "1760000600000000".to_string(),
    );
    props.insert(
        "LastTriggerUSecMonotonic".to_string(),
        "90000000".to_string(),
    );
    Timer {
        nombre: nombre.to_string(),
        props,
        servicio: None,
    }
}

fn contenedor(nombre: &str, proyecto: Option<&str>, estado: EstadoContenedor) -> Contenedor {
    Contenedor {
        id: format!("{nombre}-0123456789ab"),
        nombre: nombre.to_string(),
        imagen: "ghcr.io/ejemplo/app:1.4".to_string(),
        estado,
        codigo_salida: None,
        salud: Salud::Ninguna,
        puertos: "127.0.0.1:8080->80/tcp".to_string(),
        proyecto: proyecto.map(str::to_string),
        ficheros: proyecto
            .map(|_| vec!["/srv/web/compose.yaml".to_string()])
            .unwrap_or_default(),
        servicio: proyecto.map(|_| nombre.to_string()),
        orden: proyecto.map(|_| "1".to_string()),
        etiquetas: BTreeMap::new(),
    }
}

fn reloj() -> RelojHost {
    RelojHost {
        epoch: 1_760_000_000,
        uptime_seg: 100_000,
    }
}

fn listado_base(privilegio: Privilegio) -> Listado {
    let mut listado = Listado::vacio(privilegio);
    listado.systemd = Systemd::Disponible("252".to_string());
    listado.reloj = reloj();
    listado.diario = PermisoDiario::Todo;
    listado
}

fn listado_de_servicios() -> Listado {
    let mut listado = listado_base(Privilegio::Root);
    listado.motor = MotorEstado::Disponible(Motor {
        nombre: MotorNombre::Docker,
        version: "27.3".to_string(),
    });
    listado.unidades = vec![
        unidad("zultimo.service", "active", "enabled", true),
        vigilada(unidad("nginx.service", "active", "enabled", true)),
        unidad("copia.service", "failed", "disabled", false),
        unidad("app.service", "inactive", "disabled", false),
    ];
    listado.timers = vec![timer("limpieza.timer")];
    listado.contenedores = vec![
        contenedor("web-app-1", Some("web"), EstadoContenedor::EnMarcha),
        contenedor("web-worker-1", Some("web"), EstadoContenedor::Parado),
        contenedor("panel", None, EstadoContenedor::EnMarcha),
    ];
    listado
}

fn detalle_de_unidad() -> Detalle {
    let mut props = BTreeMap::new();
    props.insert("Description".to_string(), "app de prueba".to_string());
    props.insert("ActiveState".to_string(), "active".to_string());
    props.insert("SubState".to_string(), "running".to_string());
    props.insert("CanReload".to_string(), "yes".to_string());
    props.insert("ExecMainPID".to_string(), "4121".to_string());
    props.insert("MemoryCurrent".to_string(), "104857600".to_string());
    props.insert(
        "FragmentPath".to_string(),
        "/etc/systemd/system/app.service".to_string(),
    );
    Detalle {
        objetivo: magi_nucleo::servicios::Objetivo::unidad("app.service"),
        props,
        necesita: vec!["network.target".to_string()],
        necesitada: vec!["nginx.service".to_string()],
        texto: "[Unit]\nDescription=app de prueba\n\n[Service]\nExecStart=/usr/bin/app\n"
            .to_string(),
        contenedor: None,
    }
}

fn sondeo(host_id: i64, fallos: Option<FallosSondeo>) -> Sondeo {
    let fecha = (chrono::Utc::now() - chrono::Duration::seconds(30))
        .format("%Y-%m-%dT%H:%M:%S+00:00")
        .to_string();
    Sondeo {
        host_id,
        fecha,
        resultado: ResultadoSondeo::Ok,
        nucleos: Some(8),
        carga_1m: Some(0.5),
        carga_5m: Some(0.4),
        carga_15m: Some(0.3),
        mem_total_kb: Some(8_000_000),
        mem_disponible_kb: Some(6_000_000),
        disco_total_kb: Some(100_000_000),
        disco_usado_kb: Some(20_000_000),
        uptime_seg: Some(100_000),
        fallos,
        ..Sondeo::default()
    }
}

fn fallos_de_servicios(nombres: &[&str]) -> FallosSondeo {
    FallosSondeo {
        unidades: FallosUnidades {
            total: nombres.len() as u32,
            nombres: nombres.iter().map(|nombre| (*nombre).to_string()).collect(),
        },
        contenedores: None,
    }
}

// ---------------------------------------------------------------- pruebas

#[test]
fn f9_abre_la_vista_y_pide_el_listado_del_primer_host() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    prueba.tecla(KeyCode::F(9));
    assert_eq!(prueba.app.vista, Vista::Servicios);
    assert_eq!(prueba.app.servicios_host_seleccionado(), Some(host_id));
    let peticiones = prueba.peticiones_listar();
    assert_eq!(peticiones.len(), 1, "una petición al entrar");
    assert_eq!(peticiones[0].1, host_id);
    assert!(peticiones[0].0 >= RANGO_SERVICIOS);
}

#[test]
fn u_desde_hosts_abre_servicios_con_ese_host() {
    let mut prueba = Prueba::nueva(&["web-01", "db-01"]);
    let db = prueba.host_id("db-01");
    prueba.tecla(KeyCode::F(2));
    prueba.app.seleccionar_host_id(db);
    prueba.tecla(KeyCode::Char('u'));
    assert_eq!(prueba.app.vista, Vista::Servicios);
    assert_eq!(prueba.app.servicios_host_seleccionado(), Some(db));
    assert_eq!(prueba.app.servicios.foco, Foco::Lista);
    assert_eq!(prueba.peticiones_listar(), vec![(RANGO_SERVICIOS, db)]);
}

#[test]
fn el_orden_pone_fallidos_y_vigilados_arriba_y_fallidos_primero() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    prueba.listado_de_prueba(host_id);
    let nombres: Vec<String> = prueba
        .app
        .servicios_elementos(host_id)
        .iter()
        .map(|elemento| elemento.nombre().to_string())
        .collect();
    assert_eq!(
        nombres,
        vec![
            "copia.service",
            "nginx.service",
            "app.service",
            "zultimo.service"
        ]
    );
    // La línea separadora va entre los de arriba y el resto.
    let filas = prueba.app.servicios_filas(host_id);
    let separadores = filas
        .iter()
        .filter(|fila| matches!(fila, magi::app::servicios::Fila::Separador))
        .count();
    assert_eq!(separadores, 1);
}

impl Prueba {
    fn listado_de_prueba(&mut self, host_id: i64) {
        self.listado(host_id, listado_de_servicios());
    }
}

#[test]
fn las_secciones_cambian_con_1_2_3_y_tab_cambia_de_panel() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    prueba.listado_de_prueba(host_id);
    prueba.tecla(KeyCode::Char('2'));
    assert_eq!(prueba.app.servicios.seccion, Seccion::Timers);
    assert_eq!(
        prueba.app.servicios_elementos(host_id)[0].nombre(),
        "limpieza.timer"
    );
    prueba.tecla(KeyCode::Char('3'));
    assert_eq!(prueba.app.servicios.seccion, Seccion::Contenedores);
    assert_eq!(
        prueba.app.servicios_elementos(host_id).len(),
        4,
        "dos contenedores del proyecto, el proyecto y el suelto"
    );
    prueba.tecla(KeyCode::Tab);
    assert_eq!(prueba.app.servicios.foco, Foco::Hosts);
    prueba.tecla(KeyCode::Tab);
    assert_eq!(prueba.app.servicios.foco, Foco::Lista);
}

#[test]
fn el_filtro_deja_solo_lo_que_coincide_en_la_seccion_visible() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    prueba.listado_de_prueba(host_id);
    prueba.tecla(KeyCode::Char('/'));
    assert!(prueba.app.servicios.filtro_activo);
    for caracter in "ngi".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    prueba.tecla(KeyCode::Enter);
    assert_eq!(
        prueba.app.servicios_elementos(host_id)[0].nombre(),
        "nginx.service"
    );
    assert_eq!(prueba.app.servicios_elementos(host_id).len(), 1);
    // Esc con el filtro escrito lo limpia.
    prueba.tecla(KeyCode::Esc);
    assert_eq!(prueba.app.servicios_elementos(host_id).len(), 4);
}

#[test]
fn el_detalle_pide_el_detalle_y_cambia_de_pestana() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    prueba.listado_de_prueba(host_id);
    prueba.seleccionar_nombre(host_id, "app.service");
    prueba.tecla(KeyCode::Enter);
    let peticion = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::DetalleServicio {
                peticion_id,
                host_id: otro,
                objetivo,
                ..
            } => Some((peticion_id, otro, objetivo)),
            _ => None,
        })
        .expect("faltó DetalleServicio");
    assert_eq!(peticion.1, host_id);
    assert_eq!(peticion.2.nombre, "app.service");
    let mut detalle = detalle_de_unidad();
    detalle
        .props
        .insert("UnitFileState".to_string(), "enabled".to_string());
    prueba.servidor(MensajeServidor::Detalle {
        peticion_id: peticion.0,
        host_id,
        detalle,
    });
    let detalle = prueba.app.servicios.detalle.as_ref().expect("detalle");
    assert_eq!(detalle.pestaña, PestanaDetalle::Estado);
    assert!(detalle.detalle.is_some());
    prueba.tecla(KeyCode::Tab);
    assert_eq!(
        prueba.app.servicios.detalle.as_ref().unwrap().pestaña,
        PestanaDetalle::Dependencias
    );
    prueba.tecla(KeyCode::Tab);
    assert_eq!(
        prueba.app.servicios.detalle.as_ref().unwrap().pestaña,
        PestanaDetalle::Unidad
    );
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.servicios.detalle.is_none());
}

#[test]
fn recargar_pide_confirmacion_y_manda_la_accion() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    prueba.listado_de_prueba(host_id);
    prueba.seleccionar_nombre(host_id, "nginx.service");
    prueba.tecla(KeyCode::Char('G'));
    assert!(
        matches!(
            prueba.app.servicios.dialogo,
            Some(DialogoServicio::Confirmar(_))
        ),
        "recargar debe pedir confirmación"
    );
    prueba.tecla(KeyCode::Enter);
    let accion = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::AccionServicio {
                peticion_id,
                host_id: otro,
                accion,
                deliberacion,
                ..
            } => Some((peticion_id, otro, accion, deliberacion)),
            _ => None,
        })
        .expect("faltó AccionServicio");
    assert_eq!(accion.1, host_id);
    assert_eq!(accion.2, Accion::Recargar);
    assert!(accion.3.is_none(), "recargar no delibera");
    prueba.servidor(MensajeServidor::AccionServicioHecha {
        peticion_id: accion.0,
        resultado: ResultadoServicio {
            resultado: ResultadoAccion::Hecho,
            estado: "active".to_string(),
            segundos: 1.2,
            mensaje: String::new(),
            lineas: Vec::new(),
            comprobacion: Some("nginx -t".to_string()),
        },
    });
    assert!(matches!(
        prueba.app.servicios.dialogo,
        Some(DialogoServicio::Resultado(_))
    ));
    assert!(
        !prueba.peticiones_listar().is_empty(),
        "tras la acción se refresca el listado"
    );
}

#[test]
fn una_tecla_que_no_vale_avisa_sin_abrir_dialogo() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    let mut listado = listado_base(Privilegio::Root);
    listado.unidades = vec![unidad("app.service", "active", "enabled", false)];
    prueba.listado(host_id, listado);
    prueba.seleccionar_nombre(host_id, "app.service");
    prueba.tecla(KeyCode::Char('G'));
    assert!(prueba.app.servicios.dialogo.is_none());
    let mensaje = prueba.app.mensaje.as_ref().expect("aviso");
    assert!(
        mensaje.error && mensaje.texto.contains("no admite recargar"),
        "aviso inesperado: {}",
        mensaje.texto
    );
}

#[test]
fn parar_pasa_por_la_deliberacion_y_ctrl_k_la_guarda_y_ejecuta() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    prueba.listado_de_prueba(host_id);
    prueba.seleccionar_nombre(host_id, "nginx.service");
    prueba.tecla(KeyCode::Char('P'));
    assert!(
        prueba.app.deliberacion.is_some(),
        "parar debe abrir la deliberación"
    );
    prueba.tecla_con(KeyCode::Char('k'), KeyModifiers::CONTROL);
    assert!(prueba.app.deliberacion.is_none());
    let accion = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::AccionServicio {
                accion,
                deliberacion,
                objetivo,
                ..
            } => Some((accion, deliberacion, objetivo)),
            _ => None,
        })
        .expect("faltó AccionServicio tras la deliberación");
    assert_eq!(accion.0, Accion::Parar);
    let deliberacion = accion.1.expect("la acción debe llevar deliberación");
    assert!(deliberacion.id > 0);
    assert_eq!(accion.2.nombre, "nginx.service");
}

#[test]
fn con_sudo_con_clave_pide_la_contrasena_y_la_recuerda_en_memoria() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    let mut listado = listado_base(Privilegio::SudoConClave);
    listado.unidades = vec![unidad("app.service", "failed", "enabled", false)];
    prueba.listado(host_id, listado);
    prueba.seleccionar_nombre(host_id, "app.service");
    prueba.tecla(KeyCode::Char('A'));
    assert!(matches!(
        prueba.app.servicios.dialogo,
        Some(DialogoServicio::Sudo(_))
    ));
    for caracter in "clave".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    prueba.tecla(KeyCode::Enter);
    let peticion = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::ComprobarSudo { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("faltó ComprobarSudo");
    prueba.servidor(MensajeServidor::SudoComprobado {
        peticion_id: peticion,
        resultado: ComprobacionSudo::Ok,
    });
    assert!(prueba.app.servicios.sudo(host_id).is_some());
    // Con la contraseña en memoria, la siguiente acción ya no la pide.
    assert!(matches!(
        prueba.app.servicios.dialogo,
        Some(DialogoServicio::Confirmar(_))
    ));
    prueba.tecla(KeyCode::Esc);
    prueba.tecla(KeyCode::Char('A'));
    assert!(matches!(
        prueba.app.servicios.dialogo,
        Some(DialogoServicio::Confirmar(_))
    ));
}

#[test]
fn tres_rechazos_de_sudo_cierran_el_dialogo_sin_accion() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    let mut listado = listado_base(Privilegio::SudoConClave);
    listado.unidades = vec![unidad("app.service", "failed", "enabled", false)];
    prueba.listado(host_id, listado);
    prueba.seleccionar_nombre(host_id, "app.service");
    prueba.tecla(KeyCode::Char('A'));
    for intento in 0..3 {
        prueba.tecla(KeyCode::Char('x'));
        prueba.tecla(KeyCode::Enter);
        let peticion = prueba
            .drenar()
            .into_iter()
            .find_map(|mensaje| match mensaje {
                MensajeCliente::ComprobarSudo { peticion_id, .. } => Some(peticion_id),
                _ => None,
            })
            .unwrap_or_else(|| panic!("faltó ComprobarSudo en el intento {intento}"));
        prueba.servidor(MensajeServidor::SudoComprobado {
            peticion_id: peticion,
            resultado: ComprobacionSudo::Rechazada,
        });
    }
    assert!(prueba.app.servicios.dialogo.is_none());
    assert!(prueba.app.servicios.sudo(host_id).is_none());
    assert!(
        prueba
            .drenar()
            .into_iter()
            .all(|mensaje| !matches!(mensaje, MensajeCliente::AccionServicio { .. })),
        "no se ejecuta nada tras 3 rechazos"
    );
}

#[test]
fn sin_sudo_lo_dice_y_no_ofrece_la_accion() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    let mut listado = listado_base(Privilegio::SinSudo);
    listado.unidades = vec![unidad("app.service", "failed", "enabled", false)];
    prueba.listado(host_id, listado);
    prueba.seleccionar_nombre(host_id, "app.service");
    prueba.tecla(KeyCode::Char('A'));
    assert!(prueba.app.servicios.dialogo.is_none());
    let mensaje = prueba.app.mensaje.as_ref().expect("aviso");
    assert!(mensaje.texto.contains("sudo"), "aviso: {}", mensaje.texto);
}

#[test]
fn el_panel_de_logs_guarda_un_anillo_filtra_pausa_y_para_al_cerrar() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    prueba.listado_de_prueba(host_id);
    prueba.seleccionar_nombre(host_id, "app.service");
    prueba.tecla(KeyCode::Char('l'));
    let peticion = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::SeguirLog { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("faltó SeguirLog");
    let linea = |prioridad: u8, texto: &str| LineaLog {
        hora: Some(1_760_000_000),
        prioridad,
        stderr: false,
        texto: texto.to_string(),
    };
    prueba.servidor(MensajeServidor::LineasLog {
        peticion_id: peticion,
        lineas: vec![
            linea(6, "arrancando"),
            linea(4, "aviso 502"),
            linea(3, "error 502"),
        ],
        omitidas: 2,
    });
    let logs = prueba.app.servicios.logs.as_ref().expect("panel");
    assert_eq!(logs.total, 3);
    assert!(!logs.abriendo);
    assert_eq!(logs.anillo.front().unwrap().omitidas_antes, 2);
    // Filtro por texto.
    prueba.tecla(KeyCode::Char('/'));
    for caracter in "502".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    prueba.tecla(KeyCode::Enter);
    assert_eq!(
        prueba.app.servicios.logs.as_ref().unwrap().visibles().len(),
        2
    );
    // Nivel aviso: la de error y la de aviso, no la de info.
    prueba.tecla(KeyCode::Char('n'));
    prueba.tecla(KeyCode::Char('n'));
    assert_eq!(
        prueba.app.servicios.logs.as_ref().unwrap().visibles().len(),
        2
    );
    // AC: con el filtro 502 y el nivel en aviso, una línea de info con 502 no
    // se ve, pero la de aviso con 502 sí.
    prueba.servidor(MensajeServidor::LineasLog {
        peticion_id: peticion,
        lineas: vec![linea(6, "info con 502 no visible")],
        omitidas: 0,
    });
    let visibles = prueba.app.servicios.logs.as_ref().unwrap().visibles();
    assert!(visibles.iter().all(|anillo| anillo.linea.prioridad <= 4));
    assert_eq!(visibles.len(), 2);
    // Pausa: siguen entrando al anillo y se cuentan como nuevas.
    prueba.tecla(KeyCode::Char(' '));
    assert!(prueba.app.servicios.logs.as_ref().unwrap().pausado);
    prueba.servidor(MensajeServidor::LineasLog {
        peticion_id: peticion,
        lineas: vec![linea(3, "error 502 otra vez")],
        omitidas: 0,
    });
    let logs = prueba.app.servicios.logs.as_ref().unwrap();
    assert_eq!(logs.total, 5);
    assert_eq!(logs.nuevas, 1);
    // Fin: vuelve al final y sigue.
    prueba.tecla(KeyCode::End);
    assert!(!prueba.app.servicios.logs.as_ref().unwrap().pausado);
    // Esc cierra y manda PararLog.
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.servicios.logs.is_none());
    assert!(prueba
        .drenar()
        .into_iter()
        .any(|mensaje| matches!(mensaje, MensajeCliente::PararLog { .. })));
}

#[test]
fn el_fin_del_log_remoto_conserva_las_lineas() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    prueba.listado_de_prueba(host_id);
    prueba.seleccionar_nombre(host_id, "app.service");
    prueba.tecla(KeyCode::Char('l'));
    let peticion = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::SeguirLog { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("faltó SeguirLog");
    prueba.servidor(MensajeServidor::LineasLog {
        peticion_id: peticion,
        lineas: vec![LineaLog {
            hora: Some(1_760_000_000),
            prioridad: 6,
            stderr: false,
            texto: "una línea".to_string(),
        }],
        omitidas: 0,
    });
    prueba.servidor(MensajeServidor::FinLog {
        peticion_id: peticion,
        motivo: magi_nucleo::servicios::MotivoFinLog::Terminado,
    });
    let logs = prueba.app.servicios.logs.as_ref().expect("panel");
    assert_eq!(logs.total, 1, "las líneas se quedan");
    assert!(logs.motivo_fin.is_some());
}

#[test]
fn al_salir_de_la_vista_se_para_el_log_y_se_olvida_el_sudo() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    let mut listado = listado_base(Privilegio::SudoConClave);
    listado.unidades = vec![unidad("app.service", "failed", "enabled", false)];
    prueba.listado(host_id, listado);
    prueba.seleccionar_nombre(host_id, "app.service");
    prueba.tecla(KeyCode::Char('l'));
    prueba.servidor(MensajeServidor::LineasLog {
        peticion_id: RANGO_SERVICIOS + 2,
        lineas: Vec::new(),
        omitidas: 0,
    });
    prueba.app.servicios.memoria_sudo.insert(
        host_id,
        magi::app::servicios::MemoriaSudo {
            contrasena: zeroize::Zeroizing::new("clave".to_string()),
            ultimo_uso: Instant::now(),
        },
    );
    prueba.tecla(KeyCode::F(2));
    assert_eq!(prueba.app.vista, Vista::Hosts);
    assert!(prueba.app.servicios.logs.is_none());
    assert!(prueba.app.servicios.memoria_sudo.is_empty());
    assert!(prueba
        .drenar()
        .into_iter()
        .any(|mensaje| matches!(mensaje, MensajeCliente::PararLog { .. })));
}

#[test]
fn toda_la_flota_junta_los_fallos_de_cada_ultimo_sondeo() {
    let mut prueba = Prueba::nueva(&["web-01", "db-01"]);
    let web = prueba.host_id("web-01");
    let db = prueba.host_id("db-01");
    prueba.app.procesar(Evento::Sondeo(sondeo(
        web,
        Some(FallosSondeo {
            unidades: FallosUnidades {
                total: 1,
                nombres: vec!["copia.service".to_string()],
            },
            contenedores: Some(FallosContenedores {
                motor: Some(MotorNombre::Docker),
                total: 1,
                nombres: vec!["panel".to_string()],
            }),
        }),
    )));
    prueba.app.procesar(Evento::Sondeo(sondeo(
        db,
        Some(fallos_de_servicios(&["exportar.service"])),
    )));
    prueba.tecla(KeyCode::F(9));
    // De la columna de hosts a «Toda la flota».
    prueba.tecla(KeyCode::Up);
    assert_eq!(prueba.app.servicios_host_seleccionado(), None);
    let fallos = prueba.app.servicios_fallos_flota();
    assert_eq!(fallos.len(), 3);
    let pos_web = fallos
        .iter()
        .position(|fallo| fallo.host_nombre == "web-01")
        .expect("web-01");
    let pos_db = fallos
        .iter()
        .position(|fallo| fallo.host_nombre == "db-01")
        .expect("db-01");
    assert_eq!(fallos[pos_web].fallo.nombre, "copia.service");
    assert_eq!(fallos[pos_web + 1].fallo.nombre, "panel");
    assert_eq!(fallos[pos_db].fallo.nombre, "exportar.service");
    // Enter en el fallo de db-01 abre su host y pide el listado.
    prueba.app.servicios.seleccion_flota = pos_db;
    prueba.tecla(KeyCode::Enter);
    assert_eq!(prueba.app.servicios_host_seleccionado(), Some(db));
    let peticiones = prueba.peticiones_listar();
    let peticion = peticiones
        .iter()
        .rev()
        .find(|(_, host)| *host == db)
        .map(|(peticion_id, _)| *peticion_id)
        .expect("faltó el listado de db-01");
    // Al llegar el listado, el elemento queda seleccionado.
    let mut listado = listado_base(Privilegio::Root);
    listado.unidades = vec![unidad("exportar.service", "failed", "enabled", false)];
    prueba.servidor(MensajeServidor::Servicios {
        peticion_id: peticion,
        host_id: db,
        listado,
    });
    assert_eq!(
        prueba.app.servicios_elemento_seleccionado(db),
        Some(magi::app::servicios::Elemento::Unidad(
            "exportar.service".to_string()
        ))
    );
}

#[test]
fn las_respuestas_de_peticiones_viejas_se_descartan() {
    let mut prueba = Prueba::nueva(&["web-01", "db-01"]);
    prueba.tecla(KeyCode::F(9));
    let primero = prueba
        .app
        .servicios_host_seleccionado()
        .expect("host elegido al entrar");
    let primera = prueba.peticiones_listar();
    assert_eq!(primera.len(), 1);
    assert_eq!(primera[0].1, primero);
    // Se pide el listado de otro host: la respuesta del primero ya no vale.
    prueba.tecla(KeyCode::Down);
    let segundo = prueba
        .app
        .servicios_host_seleccionado()
        .expect("segundo host");
    assert_ne!(primero, segundo);
    let segunda = prueba.peticiones_listar();
    assert_eq!(segunda.len(), 1);
    assert_eq!(segunda[0].1, segundo);
    let mut listado_viejo = listado_base(Privilegio::Root);
    listado_viejo.avisos = vec!["viejo".to_string()];
    prueba.servidor(MensajeServidor::Servicios {
        peticion_id: primera[0].0,
        host_id: primero,
        listado: listado_viejo,
    });
    assert!(
        prueba.app.servicios_listado(primero).is_none(),
        "una respuesta que ya no está en vuelo se descarta"
    );
    // La buena sí se aplica.
    prueba.servidor(MensajeServidor::Servicios {
        peticion_id: segunda[0].0,
        host_id: segundo,
        listado: listado_de_servicios(),
    });
    assert!(prueba.app.servicios_listado(segundo).is_some());
}

#[test]
fn el_refresco_no_se_solapa_y_se_salta_si_el_host_no_responde() {
    let mut prueba = Prueba::con_config(&["web-01"], |config| {
        config.servicios.refresco_seg = 5;
    });
    let host_id = prueba.host_id("web-01");
    prueba.listado_de_prueba(host_id);
    // Se envejece el listado para que toque refrescar.
    if let Some(listado) = prueba.app.servicios.listados.get_mut(&host_id) {
        listado.recibido = Instant::now() - std::time::Duration::from_secs(30);
    }
    prueba.app.procesar(Evento::Tick);
    prueba.app.procesar(Evento::Tick);
    let peticiones = prueba.peticiones_listar();
    assert_eq!(peticiones.len(), 1, "dos ticks no piden dos listados");
    // Si la respuesta es un error, el host queda «no responde» y el refresco
    // automático se detiene hasta que el usuario pulse r.
    prueba.servidor(MensajeServidor::Error {
        mensaje: "web-01 no respondió en 10 s".to_string(),
        peticion_id: Some(peticiones[0].0),
    });
    assert!(matches!(
        prueba
            .app
            .servicios
            .listados
            .get(&host_id)
            .map(|l| &l.estado),
        Some(EstadoListado::Desconectado)
    ));
    assert!(prueba
        .app
        .servicios
        .listados
        .get(&host_id)
        .and_then(|listado| listado.aviso.clone())
        .is_some());
    if let Some(listado) = prueba.app.servicios.listados.get_mut(&host_id) {
        listado.recibido = Instant::now() - std::time::Duration::from_secs(60);
    }
    prueba.app.procesar(Evento::Tick);
    assert!(prueba.peticiones_listar().is_empty());
    // Con r se reintenta.
    prueba.tecla(KeyCode::Char('r'));
    assert_eq!(prueba.peticiones_listar().len(), 1);
}

#[test]
fn la_confirmacion_y_el_resultado_se_pintan() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    prueba.listado_de_prueba(host_id);
    prueba.seleccionar_nombre(host_id, "nginx.service");
    prueba.tecla(KeyCode::Char('G'));
    let texto = prueba.texto();
    assert!(texto.contains("CONFIRMACIÓN"), "texto: {texto}");
    assert!(texto.contains("nginx -t"), "texto: {texto}");
    prueba.tecla(KeyCode::Enter);
    let peticion = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::AccionServicio { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("faltó AccionServicio");
    prueba.servidor(MensajeServidor::AccionServicioHecha {
        peticion_id: peticion,
        resultado: ResultadoServicio {
            resultado: ResultadoAccion::Fallido,
            estado: "failed".to_string(),
            segundos: 4.1,
            mensaje: "no volvió a active".to_string(),
            lineas: vec!["KeyError: DATABASE_URL".to_string()],
            comprobacion: None,
        },
    });
    let texto = prueba.texto();
    assert!(texto.contains("FALLIDO"), "texto: {texto}");
    assert!(texto.contains("KeyError"), "texto: {texto}");
}

// ---------------------------------------------- acciones y comprobaciones

/// Un timer que activa un servicio.
fn timer_con_servicio(nombre: &str, servicio: &str) -> Timer {
    let mut props = BTreeMap::new();
    props.insert("Description".to_string(), format!("{nombre} de prueba"));
    props.insert("ActiveState".to_string(), "active".to_string());
    props.insert(
        "NextElapseUSecRealtime".to_string(),
        "1760000600000000".to_string(),
    );
    props.insert(
        "LastTriggerUSecMonotonic".to_string(),
        "90000000000".to_string(),
    );
    Timer {
        nombre: nombre.to_string(),
        props,
        servicio: Some(servicio.to_string()),
    }
}

#[test]
fn la_a_de_un_timer_lanza_su_servicio_con_confirmacion() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    let mut listado = listado_base(Privilegio::Root);
    listado.unidades = vec![unidad("limpieza.service", "inactive", "enabled", false)];
    listado.timers = vec![timer_con_servicio("limpieza.timer", "limpieza.service")];
    prueba.listado(host_id, listado);
    prueba.tecla(KeyCode::Char('2'));
    prueba.seleccionar_nombre(host_id, "limpieza.timer");
    prueba.tecla(KeyCode::Char('A'));
    assert!(
        matches!(
            prueba.app.servicios.dialogo,
            Some(DialogoServicio::Confirmar(_))
        ),
        "lanzar un timer pide confirmación"
    );
    prueba.tecla(KeyCode::Enter);
    let accion = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::AccionServicio {
                accion, objetivo, ..
            } => Some((accion, objetivo)),
            _ => None,
        })
        .expect("faltó AccionServicio del timer");
    assert_eq!(accion.0, Accion::Lanzar);
    assert_eq!(accion.1.tipo, magi_nucleo::servicios::TipoObjetivo::Timer);
    assert_eq!(accion.1.nombre, "limpieza.timer");
}

#[test]
fn una_unidad_critica_avisa_en_rojo_y_nombra_los_hosts_que_van_a_traves() {
    let mut prueba = Prueba::nueva(&["web-01", "db-01"]);
    let host_id = prueba.host_id("web-01");
    let db = prueba.host_id("db-01");
    // db-01 va a través de web-01.
    prueba
        .app
        .almacen
        .fijar_salto(db, Some(host_id))
        .expect("guardar salto");
    prueba.app.recargar_inventario().expect("inventario");
    let mut listado = listado_base(Privilegio::Root);
    listado.unidades = vec![unidad("ssh.service", "active", "enabled", true)];
    prueba.listado(host_id, listado);
    prueba.seleccionar_nombre(host_id, "ssh.service");
    prueba.tecla(KeyCode::Char('G'));
    let confirmacion = match &prueba.app.servicios.dialogo {
        Some(DialogoServicio::Confirmar(confirmacion)) => confirmacion,
        otro => panic!("no hay confirmación: {otro:?}"),
    };
    let critica = confirmacion.critica.as_deref().expect("aviso de crítica");
    assert!(
        critica.contains("puede cortar tu conexión con web-01"),
        "{critica}"
    );
    assert!(critica.contains("db-01"), "{critica}");
    let texto = prueba.texto();
    assert!(texto.contains("puede cortar tu conexión"), "{texto}");
}

#[test]
fn sin_docker_ni_podman_la_seccion_de_contenedores_lo_dice() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    let mut listado = listado_base(Privilegio::Root);
    listado.motor = MotorEstado::No;
    listado.unidades = vec![unidad("app.service", "active", "enabled", false)];
    prueba.listado(host_id, listado);
    prueba.tecla(KeyCode::Char('3'));
    let texto = prueba.texto();
    assert!(texto.contains("no hay Docker ni Podman"), "{texto}");
}

#[test]
fn una_segunda_accion_sobre_el_mismo_objetivo_se_rechaza() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    prueba.listado_de_prueba(host_id);
    prueba.seleccionar_nombre(host_id, "app.service");
    // Arrancar app.service (está inactivo): confirmación y envío.
    prueba.tecla(KeyCode::Char('A'));
    prueba.tecla(KeyCode::Enter);
    let primera = prueba.peticiones_accion();
    assert_eq!(primera.len(), 1);
    // La acción sigue en vuelo: otra sobre el mismo objetivo se rechaza.
    prueba.tecla(KeyCode::Char('A'));
    assert!(prueba.app.servicios.dialogo.is_none());
    let mensaje = prueba.app.mensaje.as_ref().expect("aviso");
    assert!(
        mensaje.texto.contains("hay otra acción en curso"),
        "aviso: {}",
        mensaje.texto
    );
    assert_eq!(prueba.peticiones_accion().len(), 0);
}

impl Prueba {
    fn peticiones_accion(&mut self) -> Vec<u64> {
        self.drenar()
            .into_iter()
            .filter_map(|mensaje| match mensaje {
                MensajeCliente::AccionServicio { peticion_id, .. } => Some(peticion_id),
                _ => None,
            })
            .collect()
    }
}

#[test]
fn la_comprobacion_propia_se_crea_edita_y_borra_con_confirmacion() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    let mut listado = listado_base(Privilegio::Root);
    listado.unidades = vec![unidad("app.service", "active", "enabled", true)];
    prueba.listado(host_id, listado);
    prueba.seleccionar_nombre(host_id, "app.service");
    // c abre el editor con el objetivo ya fijado.
    prueba.tecla(KeyCode::Char('c'));
    assert!(matches!(
        prueba.app.servicios.dialogo,
        Some(DialogoServicio::Comprobacion(_))
    ));
    for caracter in "echo ok".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    prueba.tecla(KeyCode::Enter);
    let guardada = prueba
        .app
        .almacen
        .obtener_comprobacion(
            host_id,
            magi_nucleo::servicios::TipoObjetivo::Unidad,
            "app.service",
        )
        .expect("consulta")
        .expect("comprobación guardada");
    assert_eq!(guardada.comando, "echo ok");
    let mensaje = prueba.app.mensaje.as_ref().expect("aviso");
    assert!(
        mensaje.texto.contains("MAGI 1.2"),
        "aviso de formato: {}",
        mensaje.texto
    );
    // La confirmación de la acción enseña que se comprobará.
    prueba.tecla(KeyCode::Char('G'));
    let texto = prueba.texto();
    assert!(texto.contains("comprobación propia"), "{texto}");
    prueba.tecla(KeyCode::Esc);
    // c de nuevo: el editor llega con lo guardado; x borra con confirmación.
    prueba.tecla(KeyCode::Char('c'));
    match &prueba.app.servicios.dialogo {
        Some(DialogoServicio::Comprobacion(editor)) => {
            assert_eq!(editor.comando.texto, "echo ok");
            assert!(editor.existente.is_some());
        }
        otro => panic!("no hay editor: {otro:?}"),
    }
    prueba.tecla(KeyCode::Char('x'));
    assert!(matches!(
        prueba.app.servicios.dialogo,
        Some(DialogoServicio::BorrarComprobacion { .. })
    ));
    prueba.tecla(KeyCode::Enter);
    assert!(prueba
        .app
        .almacen
        .obtener_comprobacion(
            host_id,
            magi_nucleo::servicios::TipoObjetivo::Unidad,
            "app.service"
        )
        .expect("consulta")
        .is_none());
}

#[test]
fn la_ficha_crea_edita_y_borra_comprobaciones_propias() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    // Se crea una desde la vista para tener algo que enseñar en la ficha.
    let mut listado = listado_base(Privilegio::Root);
    listado.unidades = vec![unidad("app.service", "failed", "enabled", false)];
    prueba.listado(host_id, listado);
    prueba.seleccionar_nombre(host_id, "app.service");
    prueba.tecla(KeyCode::Char('c'));
    for caracter in "true".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    prueba.tecla(KeyCode::Enter);
    // Ficha del host: el bloque trae la comprobación.
    prueba.tecla(KeyCode::F(2));
    prueba.app.seleccionar_host_id(host_id);
    prueba.tecla(KeyCode::Char('e'));
    assert_eq!(prueba.app.vista, Vista::Ficha);
    {
        let ficha = prueba.app.ficha.as_mut().expect("ficha");
        assert_eq!(ficha.comprobaciones.len(), 1);
        assert_eq!(ficha.comprobaciones[0].comando, "true");
        ficha.campo = magi::app::CampoFicha::Comprobaciones;
    }
    // e en el bloque abre el editor; Esc vuelve.
    prueba.tecla(KeyCode::Char('e'));
    assert!(matches!(
        prueba.app.servicios.dialogo,
        Some(DialogoServicio::Comprobacion(_))
    ));
    prueba.tecla(KeyCode::Esc);
    // x pide confirmación y borra.
    prueba.tecla(KeyCode::Char('x'));
    assert!(matches!(
        prueba.app.servicios.dialogo,
        Some(DialogoServicio::BorrarComprobacion { .. })
    ));
    prueba.tecla(KeyCode::Enter);
    let ficha = prueba.app.ficha.as_ref().expect("ficha");
    assert!(ficha.comprobaciones.is_empty());
}

#[test]
fn la_tecla_de_sudo_pide_la_contrasena_y_la_recuerda() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    let mut listado = listado_base(Privilegio::SudoConClave);
    listado.unidades = vec![unidad("app.service", "active", "enabled", false)];
    prueba.listado(host_id, listado);
    prueba.tecla(KeyCode::Char('!'));
    assert!(matches!(
        prueba.app.servicios.dialogo,
        Some(DialogoServicio::Sudo(_))
    ));
    for caracter in "clave".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    prueba.tecla(KeyCode::Enter);
    let peticion = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::ComprobarSudo { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("faltó ComprobarSudo");
    prueba.servidor(MensajeServidor::SudoComprobado {
        peticion_id: peticion,
        resultado: ComprobacionSudo::Ok,
    });
    assert!(prueba.app.servicios.sudo(host_id).is_some());
    // Con ! se refresca el listado para ver lo que exigía permisos.
    assert!(!prueba.peticiones_listar().is_empty());
    // Caducada (6 min sin uso), la siguiente acción la vuelve a pedir.
    prueba
        .app
        .servicios
        .memoria_sudo
        .get_mut(&host_id)
        .expect("memoria")
        .ultimo_uso = Instant::now() - std::time::Duration::from_secs(6 * 60);
    prueba.seleccionar_nombre(host_id, "app.service");
    prueba.tecla(KeyCode::Char('P'));
    assert!(matches!(
        prueba.app.servicios.dialogo,
        Some(DialogoServicio::Sudo(_))
    ));
}

#[test]
fn la_barra_marca_las_acciones_en_curso_de_cualquier_ventana() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    prueba.listado_de_prueba(host_id);
    prueba.tecla(KeyCode::F(2));
    assert_eq!(prueba.app.vista, Vista::Hosts);
    prueba.servidor(MensajeServidor::AccionesServicio {
        lista: vec![magi_nucleo::servicios::InfoAccion {
            id: RANGO_SERVICIOS + 7,
            host_id,
            host_nombre: "web-01".to_string(),
            objetivo: magi_nucleo::servicios::Objetivo::unidad("app.service"),
            accion: Accion::Reiniciar,
            fase: magi_nucleo::servicios::FaseAccion::Esperando,
            desde_ms: 0,
            solicitante: 2,
        }],
    });
    let texto = prueba.texto();
    assert!(
        texto.contains("reiniciar app.service en web-01"),
        "texto: {texto}"
    );
}

#[test]
fn si_se_cae_el_servidor_el_listado_se_queda_y_se_puede_reconectar() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    prueba.listado_de_prueba(host_id);
    let antes = prueba
        .app
        .servicios_listado(host_id)
        .map(|listado| listado.recibido_epoch);
    prueba.app.procesar(Evento::ServidorCaido);
    assert!(prueba.app.servidor_caido);
    // El aviso modal del servidor caído se cierra con n (seguir sin sesiones).
    prueba.tecla(KeyCode::Char('n'));
    let despues = prueba
        .app
        .servicios_listado(host_id)
        .expect("el listado se conserva");
    assert_eq!(antes, Some(despues.recibido_epoch));
    assert_eq!(despues.estado, EstadoListado::Desconectado);
    let texto = prueba.texto();
    assert!(texto.contains("desconectado · r reconectar"), "{texto}");
    // r relanza la conexión con el servidor (sin bloquear la UI).
    prueba.tecla(KeyCode::Char('r'));
    let mensaje = prueba.app.mensaje.as_ref().expect("aviso de reconexión");
    assert!(
        mensaje
            .texto
            .contains("reconectando con el servidor de sesiones"),
        "{}",
        mensaje.texto
    );
}

#[test]
fn la_ficha_crea_una_comprobacion_desde_su_bloque() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    prueba.listado_de_prueba(host_id);
    prueba.tecla(KeyCode::F(2));
    prueba.app.seleccionar_host_id(host_id);
    prueba.tecla(KeyCode::Char('e'));
    {
        let ficha = prueba.app.ficha.as_mut().expect("ficha");
        ficha.campo = magi::app::CampoFicha::Comprobaciones;
    }
    // n desde el bloque: objetivo y comando a mano.
    prueba.tecla(KeyCode::Char('n'));
    match &prueba.app.servicios.dialogo {
        Some(DialogoServicio::Comprobacion(editor)) => assert!(editor.objetivo.is_none()),
        otro => panic!("no hay editor: {otro:?}"),
    }
    for caracter in "web".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    prueba.tecla(KeyCode::Tab);
    for caracter in "compose config -q".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    prueba.tecla(KeyCode::Enter);
    let guardada = prueba
        .app
        .almacen
        .obtener_comprobacion(
            host_id,
            magi_nucleo::servicios::TipoObjetivo::Proyecto,
            "web",
        )
        .expect("consulta")
        .expect("comprobación del proyecto");
    assert_eq!(guardada.comando, "compose config -q");
    let ficha = prueba.app.ficha.as_ref().expect("ficha");
    assert_eq!(ficha.comprobaciones.len(), 1);
    assert_eq!(ficha.comprobaciones[0].objetivo.nombre, "web");
}

#[test]
fn el_anillo_de_logs_respeta_lineas_log() {
    let mut prueba = Prueba::con_config(&["web-01"], |config| {
        config.servicios.lineas_log = 1000;
        config.servicios.refresco_seg = 0;
    });
    let host_id = prueba.host_id("web-01");
    prueba.listado_de_prueba(host_id);
    prueba.seleccionar_nombre(host_id, "app.service");
    prueba.tecla(KeyCode::Char('l'));
    let peticion = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::SeguirLog { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("faltó SeguirLog");
    let lineas: Vec<LineaLog> = (0..1001)
        .map(|indice| LineaLog {
            hora: Some(1_760_000_000),
            prioridad: 6,
            stderr: false,
            texto: format!("linea {indice}"),
        })
        .collect();
    prueba.servidor(MensajeServidor::LineasLog {
        peticion_id: peticion,
        lineas,
        omitidas: 0,
    });
    let logs = prueba.app.servicios.logs.as_ref().expect("panel");
    assert_eq!(logs.total, 1001);
    assert_eq!(logs.anillo.len(), 1000, "el anillo no crece sin límite");
    assert_eq!(
        logs.anillo
            .front()
            .map(|anillo| anillo.linea.texto.as_str()),
        Some("linea 1"),
        "se descarta la más vieja"
    );
}

#[test]
fn la_shell_del_contenedor_y_el_seguimiento_abren_pestanas_con_orden() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    prueba.listado_de_prueba(host_id);
    prueba.tecla(KeyCode::Char('3'));
    prueba.seleccionar_nombre(host_id, "web-app-1");
    prueba.tecla(KeyCode::Char('S'));
    let orden = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::AbrirSesion { orden, .. } => orden,
            _ => None,
        })
        .expect("faltó AbrirSesion de la shell");
    match orden {
        magi_nucleo::servicios::OrdenPestana::ShellContenedor { motor, id } => {
            assert_eq!(motor, MotorNombre::Docker);
            assert_eq!(id, "web-app-1-0123456789ab");
        }
        otro => panic!("orden inesperada: {otro:?}"),
    }
    // t desde el panel de logs: seguimiento del contenedor en segundo plano.
    prueba.seleccionar_nombre(host_id, "web-app-1");
    prueba.tecla(KeyCode::Char('l'));
    let peticion = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::SeguirLog { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("faltó SeguirLog");
    prueba.servidor(MensajeServidor::LineasLog {
        peticion_id: peticion,
        lineas: vec![LineaLog {
            hora: Some(1_760_000_000),
            prioridad: 6,
            stderr: false,
            texto: "linea".to_string(),
        }],
        omitidas: 0,
    });
    prueba.tecla(KeyCode::Char('t'));
    let orden = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::AbrirSesion { orden, .. } => orden,
            _ => None,
        })
        .expect("faltó AbrirSesion del seguimiento");
    assert!(matches!(
        orden,
        magi_nucleo::servicios::OrdenPestana::SeguirContenedor { .. }
    ));
    assert!(
        prueba.app.servicios.logs.is_some(),
        "el panel sigue abierto"
    );
    assert_eq!(prueba.app.vista, Vista::Servicios);
}

#[test]
fn el_consumo_se_pide_aparte_y_solo_con_los_contenedores_a_la_vista() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    prueba.listado_de_prueba(host_id);
    // Pasar a Contenedores pide el consumo (docker stats, tarda 1-2 s).
    prueba.tecla(KeyCode::Char('3'));
    let peticion = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::PedirConsumo { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("faltó PedirConsumo");
    prueba.servidor(MensajeServidor::Consumo {
        peticion_id: peticion,
        host_id,
        consumos: vec![magi_nucleo::servicios::Consumo {
            id: "web-app-1-0123456789ab".to_string(),
            cpu_pct: Some(2.0),
            memoria_bytes: Some(188_743_680),
            memoria_limite_bytes: None,
        }],
    });
    let consumo = prueba
        .app
        .servicios
        .consumos
        .get(&host_id)
        .expect("consumo guardado");
    assert_eq!(consumo.consumos.len(), 1);
}

#[test]
fn desde_toda_la_flota_se_lanza_una_accion_tras_ir_al_host() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    prueba.app.procesar(Evento::Sondeo(sondeo(
        host_id,
        Some(fallos_de_servicios(&["app.service"])),
    )));
    prueba.tecla(KeyCode::F(9));
    prueba.tecla(KeyCode::Up);
    assert_eq!(prueba.app.servicios_host_seleccionado(), None);
    prueba.tecla(KeyCode::Char('A'));
    assert_eq!(prueba.app.servicios_host_seleccionado(), Some(host_id));
    let mut listado = listado_base(Privilegio::Root);
    listado.unidades = vec![unidad("app.service", "failed", "enabled", false)];
    prueba.responder_listado(host_id, listado);
    assert!(
        matches!(
            prueba.app.servicios.dialogo,
            Some(DialogoServicio::Confirmar(_))
        ),
        "arrancar desde el fallo pide confirmación"
    );
}

// ------------------------------------- requisitos de cierre (Fase 13)

#[test]
fn un_proyecto_sin_ficheros_no_ofrece_acciones_y_lo_dice() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    let mut listado = listado_base(Privilegio::Root);
    listado.motor = MotorEstado::Disponible(Motor {
        nombre: MotorNombre::Docker,
        version: "27.3".to_string(),
    });
    // Contenedores del proyecto, pero sin ficheros de Compose conocidos.
    let mut suelto = contenedor("web-app-1", Some("web"), EstadoContenedor::EnMarcha);
    suelto.ficheros.clear();
    listado.contenedores = vec![suelto];
    listado.proyectos = vec![magi_nucleo::servicios::Proyecto {
        nombre: "web".to_string(),
        ficheros: Vec::new(),
    }];
    prueba.listado(host_id, listado);
    prueba.tecla(KeyCode::Char('3'));
    prueba.seleccionar_nombre(host_id, "web");
    let objetivo = magi_nucleo::servicios::Objetivo::proyecto("web");
    assert!(
        prueba
            .app
            .servicios_acciones_ofrecidas(host_id, &objetivo)
            .is_empty(),
        "sin ficheros no se ofrecen acciones de proyecto"
    );
    prueba.tecla(KeyCode::Char('P'));
    let mensaje = prueba.app.mensaje.as_ref().expect("aviso");
    assert!(
        mensaje.texto.contains("sin ficheros de compose conocidos"),
        "aviso: {}",
        mensaje.texto
    );
}

#[test]
fn la_barra_solo_ensena_las_acciones_que_valen() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    let mut listado = listado_base(Privilegio::Root);
    listado.unidades = vec![unidad("copia.service", "inactive", "disabled", false)];
    prueba.listado(host_id, listado);
    prueba.seleccionar_nombre(host_id, "copia.service");
    let atajos = magi::ui::barra::atajos(&prueba.app, prueba.app.disposicion());
    let hay = |tecla: &str, texto: &str| {
        atajos
            .iter()
            .any(|atajo| atajo.tecla == tecla && atajo.texto == texto)
    };
    assert!(hay("A", "arrancar"), "atajos: {atajos:?}");
    assert!(hay("H", "activar"), "atajos: {atajos:?}");
    assert!(
        !hay("P", "parar") && !hay("G", "recargar"),
        "atajos: {atajos:?}"
    );
    // Pulsar una tecla que no vale sigue avisando con el motivo.
    prueba.tecla(KeyCode::Char('G'));
    let mensaje = prueba.app.mensaje.as_ref().expect("aviso");
    assert!(
        mensaje.texto.contains("no admite recargar"),
        "aviso: {}",
        mensaje.texto
    );
}

#[test]
fn la_barra_no_ofrece_acciones_privilegiadas_sin_sudo() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    let mut listado = listado_base(Privilegio::SinSudo);
    listado.unidades = vec![unidad("copia.service", "failed", "disabled", false)];
    prueba.listado(host_id, listado);
    prueba.seleccionar_nombre(host_id, "copia.service");
    let atajos = magi::ui::barra::atajos(&prueba.app, prueba.app.disposicion());
    assert!(
        atajos
            .iter()
            .all(|atajo| !matches!(atajo.tecla.as_ref(), "A" | "P" | "R" | "G" | "H")),
        "atajos: {atajos:?}"
    );
    prueba.tecla(KeyCode::Char('A'));
    let mensaje = prueba.app.mensaje.as_ref().expect("aviso");
    assert!(
        mensaje.texto.contains("usuario no puede usar sudo"),
        "aviso: {}",
        mensaje.texto
    );
}

#[test]
fn el_detalle_ensena_el_arranque_de_la_unidad() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    prueba.listado_de_prueba(host_id);
    prueba.seleccionar_nombre(host_id, "app.service");
    prueba.tecla(KeyCode::Enter);
    let peticion = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::DetalleServicio { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("faltó DetalleServicio");
    let mut detalle = detalle_de_unidad();
    detalle
        .props
        .insert("UnitFileState".to_string(), "enabled".to_string());
    prueba.servidor(MensajeServidor::Detalle {
        peticion_id: peticion,
        host_id,
        detalle,
    });
    let texto = prueba.texto();
    assert!(
        texto.contains("arranque") && texto.contains("activado"),
        "{texto}"
    );
}

#[test]
fn la_deliberacion_de_una_unidad_critica_avisa_en_rojo() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    let mut listado = listado_base(Privilegio::Root);
    listado.unidades = vec![unidad("ssh.service", "active", "enabled", false)];
    prueba.listado(host_id, listado);
    prueba.seleccionar_nombre(host_id, "ssh.service");
    prueba.tecla(KeyCode::Char('R'));
    assert!(prueba.app.deliberacion.is_some());
    let texto = prueba.texto();
    assert!(
        texto.contains("puede cortar tu conexión con web-01"),
        "texto: {texto}"
    );
    assert!(texto.contains("⚑"), "texto: {texto}");
    prueba.tecla(KeyCode::Esc);
}

#[test]
fn un_error_al_listar_conserva_el_listado_y_pide_reconectar() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    prueba.listado_de_prueba(host_id);
    let antes = prueba
        .app
        .servicios_listado(host_id)
        .map(|listado| listado.recibido_epoch);
    prueba.tecla(KeyCode::Char('r'));
    let peticion = prueba
        .peticiones_listar()
        .into_iter()
        .find(|(_, host)| *host == host_id)
        .map(|(peticion_id, _)| peticion_id)
        .expect("faltó el refresco");
    prueba.servidor(MensajeServidor::Error {
        mensaje: "no se llega a web-01 en 10 s".to_string(),
        peticion_id: Some(peticion),
    });
    let listado = prueba.app.servicios_listado(host_id).expect("se conserva");
    assert_eq!(antes, Some(listado.recibido_epoch));
    assert_eq!(listado.estado, EstadoListado::Desconectado);
    let texto = prueba.texto();
    assert!(texto.contains("desconectado · r reconectar"), "{texto}");
    assert!(texto.contains("no se llega a web-01"), "{texto}");
    // r reintenta el listado (el servidor sigue en pie).
    prueba.tecla(KeyCode::Char('r'));
    assert_eq!(prueba.peticiones_listar().len(), 1);
}

// ------------------------------------- MELCHIOR y deliberación sin comprobaciones

/// Un sondeo nominal salvo por las unidades vigiladas que se indiquen.
fn sondeo_con_servicios(host_id: i64, servicios: &[(&str, &str)]) -> Sondeo {
    let mut mapa = BTreeMap::new();
    for (unidad, estado) in servicios {
        mapa.insert((*unidad).to_string(), (*estado).to_string());
    }
    Sondeo {
        host_id,
        fecha: (chrono::Utc::now() - chrono::Duration::seconds(30))
            .format("%Y-%m-%dT%H:%M:%S+00:00")
            .to_string(),
        resultado: ResultadoSondeo::Ok,
        nucleos: Some(8),
        carga_1m: Some(0.5),
        carga_5m: Some(0.4),
        carga_15m: Some(0.3),
        mem_total_kb: Some(8_000_000),
        mem_disponible_kb: Some(6_000_000),
        disco_total_kb: Some(100_000_000),
        disco_usado_kb: Some(20_000_000),
        uptime_seg: Some(100_000),
        servicios: mapa,
        fallos: None,
        ..Sondeo::default()
    }
}

#[test]
fn melchior_no_cuenta_el_estado_del_propio_objetivo() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    // La salud del host está activa: MELCHIOR-1 mirará el último sondeo.
    prueba
        .app
        .almacen
        .guardar_verificaciones(
            host_id,
            &magi::deliberacion::DatosVerificaciones {
                salud: true,
                ..Default::default()
            },
        )
        .expect("verificaciones de prueba");
    // app.service vigilado y caído: es lo único que deja el host en CARGA.
    let sondeo = sondeo_con_servicios(
        host_id,
        &[("app.service", "failed"), ("nginx.service", "active")],
    );
    prueba.app.procesar(Evento::Sondeo(sondeo.clone()));
    // Sin excluir el objetivo, MELCHIOR-1 rechazaría (el host está en CARGA).
    let limites = magi::deliberacion::Limites::desde_config(&prueba.app.config.deliberacion);
    let umbrales = prueba.app.config.flota.umbrales.clone();
    assert!(
        matches!(
            magi::deliberacion::salud::con_ultimo(Some(&sondeo), limites.salud_max, &umbrales),
            magi::deliberacion::salud::DecisionSalud::Rechaza(_)
        ),
        "sin excluir el objetivo el sondeo da CARGA"
    );
    // Reiniciar app.service: deliberación con MELCHIOR-1 sin el objetivo.
    let mut listado = listado_base(Privilegio::Root);
    listado.unidades = vec![
        unidad("app.service", "failed", "enabled", false),
        unidad("nginx.service", "active", "enabled", true),
    ];
    prueba.listado(host_id, listado);
    prueba.seleccionar_nombre(host_id, "app.service");
    prueba.tecla(KeyCode::Char('R'));
    let abierta = prueba.app.deliberacion.as_ref().expect("deliberación");
    let fila = abierta.filas.first().expect("una fila de host");
    assert!(
        fila.salud.aprueba(),
        "MELCHIOR-1 debía aprobar sin contar app.service: {:?}",
        fila.salud
    );
    assert!(
        fila.salud.detalle().unwrap_or("").contains("NOMINAL"),
        "detalle: {:?}",
        fila.salud.detalle()
    );
}

#[test]
fn sin_comprobaciones_activas_la_deliberacion_lo_dice_y_pide_ctrl_k() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    prueba.listado_de_prueba(host_id);
    prueba.seleccionar_nombre(host_id, "nginx.service");
    prueba.tecla(KeyCode::Char('P'));
    assert!(prueba.app.deliberacion.is_some());
    let texto = prueba.texto();
    assert!(texto.contains("sin comprobaciones"), "texto: {texto}");
    assert!(texto.contains("ctrl+k ejecutar"), "texto: {texto}");
    // ↵ no ejecuta (solo Ctrl+K): no se manda ninguna acción y sigue abierta.
    prueba.tecla(KeyCode::Enter);
    assert!(prueba.app.deliberacion.is_some());
    assert!(
        prueba
            .drenar()
            .into_iter()
            .all(|mensaje| !matches!(mensaje, MensajeCliente::AccionServicio { .. })),
        "↵ no debe ejecutar"
    );
    // Ctrl+K sí: se guarda la deliberación y se manda la acción.
    prueba.tecla_con(KeyCode::Char('k'), KeyModifiers::CONTROL);
    assert!(prueba.app.deliberacion.is_none());
    assert!(
        prueba
            .drenar()
            .into_iter()
            .any(|mensaje| matches!(mensaje, MensajeCliente::AccionServicio { .. })),
        "Ctrl+K debe ejecutar"
    );
}

// ------------------------------- revisión adversarial (H1, M3, B1 y B6)

#[test]
fn sin_sudo_con_docker_directo_se_gestionan_contenedores_y_proyectos() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    let mut listado = listado_base(Privilegio::SinSudo);
    listado.motor = MotorEstado::Disponible(Motor {
        nombre: MotorNombre::Docker,
        version: "27.3".to_string(),
    });
    listado.contenedores = vec![
        contenedor("web-app-1", Some("web"), EstadoContenedor::EnMarcha),
        contenedor("web-worker-1", Some("web"), EstadoContenedor::Parado),
    ];
    listado.proyectos = vec![magi_nucleo::servicios::Proyecto {
        nombre: "web".to_string(),
        ficheros: vec!["/srv/web/compose.yaml".to_string()],
    }];
    prueba.listado(host_id, listado);
    prueba.tecla(KeyCode::Char('3'));
    // H1: con el grupo docker y sin sudo, el contenedor parado se arranca.
    prueba.seleccionar_nombre(host_id, "web-worker-1");
    let objetivo = magi_nucleo::servicios::Objetivo::contenedor("web-worker-1");
    let ofrecidas = prueba.app.servicios_acciones_ofrecidas(host_id, &objetivo);
    assert!(
        ofrecidas.contains(&Accion::Arrancar),
        "acciones ofrecidas: {ofrecidas:?}"
    );
    prueba.tecla(KeyCode::Char('A'));
    assert!(
        matches!(
            prueba.app.servicios.dialogo,
            Some(DialogoServicio::Confirmar(_))
        ),
        "arrancar un contenedor pide confirmación"
    );
    prueba.tecla(KeyCode::Enter);
    let sudo = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::AccionServicio { sudo, .. } => Some(sudo),
            _ => None,
        })
        .expect("faltó AccionServicio del contenedor");
    assert!(sudo.is_none(), "sin sudo directo no se manda contraseña");
    // Y el proyecto entero también se ofrece sin sudo.
    prueba.seleccionar_nombre(host_id, "web");
    let proyecto = magi_nucleo::servicios::Objetivo::proyecto("web");
    let ofrecidas = prueba.app.servicios_acciones_ofrecidas(host_id, &proyecto);
    assert!(
        ofrecidas.contains(&Accion::Parar) && ofrecidas.contains(&Accion::Reiniciar),
        "acciones del proyecto: {ofrecidas:?}"
    );
    prueba.tecla(KeyCode::Char('P'));
    assert!(
        prueba.app.deliberacion.is_some(),
        "parar el proyecto pasa por deliberación"
    );
    prueba.tecla(KeyCode::Esc);
}

#[test]
fn sin_motor_disponible_siguen_sin_ofrecerse_las_acciones_de_contenedor() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    let mut listado = listado_base(Privilegio::SinSudo);
    listado.motor = MotorEstado::SinPermiso;
    listado.contenedores = vec![contenedor("panel", None, EstadoContenedor::EnMarcha)];
    prueba.listado(host_id, listado);
    prueba.tecla(KeyCode::Char('3'));
    prueba.seleccionar_nombre(host_id, "panel");
    let objetivo = magi_nucleo::servicios::Objetivo::contenedor("panel");
    assert!(prueba
        .app
        .servicios_acciones_ofrecidas(host_id, &objetivo)
        .is_empty());
    prueba.tecla(KeyCode::Char('P'));
    let mensaje = prueba.app.mensaje.as_ref().expect("aviso");
    assert!(
        mensaje.texto.contains("sin permiso para el motor"),
        "aviso: {}",
        mensaje.texto
    );
}

#[test]
fn el_anillo_de_logs_tiene_presupuesto_de_bytes() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    prueba.listado_de_prueba(host_id);
    prueba.seleccionar_nombre(host_id, "app.service");
    prueba.tecla(KeyCode::Char('l'));
    let peticion = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::SeguirLog { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("faltó SeguirLog");
    // 600 líneas de 16 KiB son 9,8 MiB: por encima del presupuesto de 8 MiB.
    let grande = "x".repeat(magi_nucleo::servicios::limites::LINEA_LOG_MAX_BYTES);
    let lineas: Vec<LineaLog> = (0..600)
        .map(|_| LineaLog {
            hora: Some(1_760_000_000),
            prioridad: 6,
            stderr: false,
            texto: grande.clone(),
        })
        .collect();
    prueba.servidor(MensajeServidor::LineasLog {
        peticion_id: peticion,
        lineas,
        omitidas: 0,
    });
    let logs = prueba.app.servicios.logs.as_ref().expect("panel");
    assert_eq!(logs.total, 600);
    assert!(
        logs.bytes <= magi::app::servicios::PRESUPUESTO_LOG_BYTES,
        "el anillo ocupa {} bytes",
        logs.bytes
    );
    assert!(
        logs.omitidas_locales > 0,
        "debería haber descartado líneas por presupuesto"
    );
    assert!(
        logs.anillo.iter().any(|anillo| anillo.omitidas_antes > 0),
        "alguna línea lleva la marca de las omitidas"
    );
    assert!(prueba.texto().contains("líneas omitidas"));
}

#[test]
fn la_contrasena_de_sudo_no_caduca_con_los_refrescos() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    let mut listado = listado_base(Privilegio::SudoConClave);
    listado.unidades = vec![unidad("app.service", "failed", "enabled", false)];
    let listado_ok = {
        let mut ok = listado_base(Privilegio::SudoConClave);
        ok.unidades = vec![unidad("app.service", "failed", "enabled", false)];
        ok
    };
    prueba.listado(host_id, listado);
    // Se da la contraseña con !.
    prueba.tecla(KeyCode::Char('!'));
    for caracter in "clave".chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
    prueba.tecla(KeyCode::Enter);
    let peticion = prueba
        .drenar()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::ComprobarSudo { peticion_id, .. } => Some(peticion_id),
            _ => None,
        })
        .expect("faltó ComprobarSudo");
    prueba.servidor(MensajeServidor::SudoComprobado {
        peticion_id: peticion,
        resultado: ComprobacionSudo::Ok,
    });
    assert!(prueba.app.servicios.sudo(host_id).is_some());
    // El propio ! refresca el listado: se responde para no dejarlo en vuelo.
    let refrescos = prueba.peticiones_listar();
    for (peticion, _) in refrescos {
        prueba.servidor(MensajeServidor::Servicios {
            peticion_id: peticion,
            host_id,
            listado: listado_ok.clone(),
        });
    }
    // Dentro de los 5 min, un refresco la usa pero NO renueva el uso.
    {
        let memoria = prueba
            .app
            .servicios
            .memoria_sudo
            .get_mut(&host_id)
            .expect("memoria");
        memoria.ultimo_uso = Instant::now() - std::time::Duration::from_secs(4 * 60);
    }
    let antes = prueba
        .app
        .servicios
        .memoria_sudo
        .get(&host_id)
        .map(|memoria| memoria.ultimo_uso)
        .expect("memoria");
    prueba.tecla(KeyCode::Char('r'));
    let listados: Vec<(u64, bool)> = prueba
        .drenar()
        .into_iter()
        .filter_map(|mensaje| match mensaje {
            MensajeCliente::ListarServicios {
                peticion_id, sudo, ..
            } => Some((peticion_id, sudo.is_some())),
            _ => None,
        })
        .collect();
    assert!(
        listados.iter().any(|(_, con_sudo)| *con_sudo),
        "una lectura puede usar la contraseña vigente"
    );
    assert_eq!(
        prueba
            .app
            .servicios
            .memoria_sudo
            .get(&host_id)
            .map(|memoria| memoria.ultimo_uso),
        Some(antes),
        "la lectura no debe renovar el último uso"
    );
    for (peticion, _) in &listados {
        prueba.servidor(MensajeServidor::Servicios {
            peticion_id: *peticion,
            host_id,
            listado: listado_ok.clone(),
        });
    }
    // A los 6 min sin uso, el refresco ya no la adjunta (y la olvida).
    {
        let memoria = prueba
            .app
            .servicios
            .memoria_sudo
            .get_mut(&host_id)
            .expect("memoria");
        memoria.ultimo_uso = Instant::now() - std::time::Duration::from_secs(6 * 60);
    }
    prueba.tecla(KeyCode::Char('r'));
    let listados: Vec<(u64, bool)> = prueba
        .drenar()
        .into_iter()
        .filter_map(|mensaje| match mensaje {
            MensajeCliente::ListarServicios {
                peticion_id, sudo, ..
            } => Some((peticion_id, sudo.is_some())),
            _ => None,
        })
        .collect();
    assert!(!listados.is_empty(), "faltó el listado del refresco");
    assert!(
        listados.iter().all(|(_, con_sudo)| !*con_sudo),
        "caducada no se manda en las lecturas"
    );
    assert!(prueba.app.servicios.sudo(host_id).is_none());
    for (peticion, _) in &listados {
        prueba.servidor(MensajeServidor::Servicios {
            peticion_id: *peticion,
            host_id,
            listado: listado_ok.clone(),
        });
    }
    // Una acción la vuelve a pedir.
    prueba.seleccionar_nombre(host_id, "app.service");
    prueba.tecla(KeyCode::Char('A'));
    assert!(matches!(
        prueba.app.servicios.dialogo,
        Some(DialogoServicio::Sudo(_))
    ));
}

#[test]
fn la_deliberacion_ensena_la_comprobacion_de_configuracion() {
    let mut prueba = Prueba::nueva(&["web-01"]);
    let host_id = prueba.host_id("web-01");
    let mut listado = listado_base(Privilegio::Root);
    listado.unidades = vec![unidad("nginx.service", "active", "enabled", true)];
    prueba.listado(host_id, listado);
    prueba.seleccionar_nombre(host_id, "nginx.service");
    prueba.tecla(KeyCode::Char('R'));
    assert!(prueba.app.deliberacion.is_some());
    let texto = prueba.texto();
    assert!(
        texto.contains("Antes de reiniciar: nginx -t"),
        "texto: {texto}"
    );
    prueba.tecla(KeyCode::Esc);
}
