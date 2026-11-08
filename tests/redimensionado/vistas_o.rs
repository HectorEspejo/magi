//! Instantáneas de la Fase 13: vista Servicios, detalle, panel de logs,
//! «Toda la flota» y sus diálogos a 40×12, 80×24 y 200×60 (T45).
//!
//! Todo con App::de_prueba (AppPrueba): sin hilos, red ni servidor. Los
//! listados se inyectan como los mandaría el servidor, respondiendo a la
//! petición que el cliente acaba de enviar.

use crossterm::event::KeyCode;

use magi::app::Evento;
use magi::modelo::{ResultadoSondeo, Sondeo};
use magi::protocolo::{MensajeCliente, MensajeServidor};
use magi::ui::Vista;
use magi_nucleo::servicios::{
    Contenedor, Detalle, EstadoContenedor, FallosContenedores, FallosSondeo, FallosUnidades,
    LineaLog, Listado, Motor, MotorEstado, MotorNombre, PermisoDiario, Privilegio, ResultadoAccion,
    ResultadoServicio, Salud, Systemd, Timer, Unidad,
};

use crate::arnes::AppPrueba;
use crate::semilla::Sembrado;

/// Los tres tamaños de referencia de la fase.
const TAMANOS: [(u16, u16); 3] = [(40, 12), (80, 24), (200, 60)];

/// Instantánea con los filtros de reloj (antigüedad del listado y rutas).
fn instantanea(prueba: &AppPrueba, nombre: &str) {
    let texto = prueba.texto();
    let mut ajustes = insta::Settings::clone_current();
    ajustes.set_snapshot_path("../snapshots");
    ajustes.set_prepend_module_to_snapshot(false);
    ajustes.set_omit_expression(true);
    ajustes.add_filter(r"/tmp/magi[A-Za-z0-9]{6}", "[tmp]");
    ajustes.add_filter(r"hace \d+ s", "hace NN s");
    ajustes.add_filter(r"hace \d+ min", "hace NN min");
    ajustes.bind(|| insta::assert_snapshot!(nombre, texto));
}

/// Unidad de prueba con las propiedades que mira la vista.
fn unidad(nombre: &str, activo: &str, arranque: &str, con_recarga: bool) -> Unidad {
    let mut props = std::collections::BTreeMap::new();
    props.insert(
        "Description".to_string(),
        format!("{nombre} del host de prueba"),
    );
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
    props.insert("MemoryCurrent".to_string(), "188743680".to_string());
    props.insert(
        "FragmentPath".to_string(),
        format!("/etc/systemd/system/{nombre}"),
    );
    // 2 h en el estado actual con el reloj de la semilla.
    props.insert(
        "ActiveEnterTimestampMonotonic".to_string(),
        "92800000000".to_string(),
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

fn timer(nombre: &str, servicio: &str) -> Timer {
    let mut props = std::collections::BTreeMap::new();
    props.insert(
        "Description".to_string(),
        format!("{nombre} del host de prueba"),
    );
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

fn contenedor(
    nombre: &str,
    proyecto: Option<&str>,
    estado: EstadoContenedor,
    codigo: Option<i32>,
    salud: Salud,
) -> Contenedor {
    Contenedor {
        id: format!("{nombre}-0123456789ab"),
        nombre: nombre.to_string(),
        imagen: "ghcr.io/ejemplo/app:1.4".to_string(),
        estado,
        codigo_salida: codigo,
        salud,
        puertos: "127.0.0.1:8080->80/tcp".to_string(),
        proyecto: proyecto.map(str::to_string),
        ficheros: proyecto
            .map(|_| vec!["/srv/web/compose.yaml".to_string()])
            .unwrap_or_default(),
        servicio: proyecto.map(|_| nombre.to_string()),
        orden: proyecto.map(|_| "1".to_string()),
        etiquetas: std::collections::BTreeMap::new(),
    }
}

fn reloj() -> magi_nucleo::servicios::RelojHost {
    magi_nucleo::servicios::RelojHost {
        epoch: 1_760_000_000,
        uptime_seg: 100_000,
    }
}

fn listado() -> Listado {
    let mut listado = Listado::vacio(Privilegio::Root);
    listado.systemd = Systemd::Disponible("252".to_string());
    listado.reloj = reloj();
    listado.diario = PermisoDiario::Todo;
    listado.motor = MotorEstado::Disponible(Motor {
        nombre: MotorNombre::Docker,
        version: "27.3".to_string(),
    });
    listado.unidades = vec![
        unidad("copia.service", "failed", "disabled", false),
        vigilada(unidad("nginx.service", "active", "enabled", true)),
        vigilada(unidad("app.service", "active", "enabled", true)),
        vigilada(unidad("postgresql.service", "active", "enabled", false)),
        unidad("cron.service", "active", "enabled", false),
        unidad("fail2ban.service", "active", "enabled", false),
        unidad("tareas.service", "inactive", "disabled", false),
        unidad("ssh.service", "active", "enabled", false),
    ];
    listado.timers = vec![
        timer("limpieza.timer", "limpieza.service"),
        timer("backup.timer", "backup.service"),
    ];
    listado.contenedores = vec![
        contenedor(
            "web-app-1",
            Some("web"),
            EstadoContenedor::EnMarcha,
            None,
            Salud::Sana,
        ),
        contenedor(
            "web-worker-1",
            Some("web"),
            EstadoContenedor::Parado,
            Some(1),
            Salud::Ninguna,
        ),
        contenedor(
            "copias-restic-1",
            Some("copias"),
            EstadoContenedor::Parado,
            Some(0),
            Salud::Ninguna,
        ),
        contenedor(
            "panel",
            None,
            EstadoContenedor::EnMarcha,
            None,
            Salud::Insegura,
        ),
    ];
    listado
}

fn detalle() -> Detalle {
    let mut props = std::collections::BTreeMap::new();
    props.insert("Description".to_string(), "app de prueba".to_string());
    props.insert("ActiveState".to_string(), "active".to_string());
    props.insert("SubState".to_string(), "running".to_string());
    props.insert("CanReload".to_string(), "yes".to_string());
    props.insert("UnitFileState".to_string(), "enabled".to_string());
    props.insert("ExecMainPID".to_string(), "4121".to_string());
    props.insert("MemoryCurrent".to_string(), "188743680".to_string());
    props.insert(
        "FragmentPath".to_string(),
        "/etc/systemd/system/app.service".to_string(),
    );
    props.insert(
        "ActiveEnterTimestampMonotonic".to_string(),
        "92800000000".to_string(),
    );
    Detalle {
        objetivo: magi_nucleo::servicios::Objetivo::unidad("app.service"),
        props,
        necesita: vec!["network.target".to_string(), "postgresql.service".to_string()],
        necesitada: vec!["nginx.service".to_string()],
        texto: "[Unit]\nDescription=app de prueba\nAfter=network.target postgresql.service\n\n[Service]\nUser=usuario\nExecStart=/usr/bin/app\nRestart=always\n".to_string(),
        contenedor: None,
    }
}

/// Abre Servicios con el host de la semilla y responde su listado.
fn abrir_servicios(prueba: &mut AppPrueba, sembrado: &Sembrado) -> i64 {
    let host_id = sembrado.host("hetzner-01");
    prueba.tecla(KeyCode::F(2));
    prueba.app.seleccionar_host_id(host_id);
    prueba.tecla(KeyCode::Char('u'));
    assert_eq!(prueba.app.vista, Vista::Servicios);
    responder_listado(prueba, host_id, listado());
    host_id
}

fn responder_listado(prueba: &mut AppPrueba, host_id: i64, listado: Listado) {
    let peticion = prueba
        .enviados()
        .into_iter()
        .filter_map(|mensaje| match mensaje {
            MensajeCliente::ListarServicios {
                peticion_id,
                host_id: otro,
                ..
            } if otro == host_id => Some(peticion_id),
            _ => None,
        })
        .next_back()
        .expect("faltó ListarServicios");
    prueba.servidor(MensajeServidor::Servicios {
        peticion_id: peticion,
        host_id,
        listado,
    });
}

fn seleccionar(prueba: &mut AppPrueba, host_id: i64, nombre: &str) {
    let elementos = prueba.app.servicios_elementos(host_id);
    let indice = elementos
        .iter()
        .position(|elemento| elemento.nombre() == nombre)
        .unwrap_or_else(|| panic!("{nombre} no está en la lista"));
    prueba
        .app
        .servicios
        .seleccion
        .insert((host_id, prueba.app.servicios.seccion), indice);
}

fn con_vista(cols: u16, filas: u16) -> (AppPrueba, Sembrado, i64) {
    let (mut prueba, sembrado) = AppPrueba::con_semilla(cols, filas);
    let host_id = abrir_servicios(&mut prueba, &sembrado);
    (prueba, sembrado, host_id)
}

#[test]
fn instantaneas_de_la_vista() {
    for (ancho, alto) in TAMANOS {
        let (prueba, _sembrado, _host) = con_vista(ancho, alto);
        instantanea(&prueba, &format!("servicios_vista_{ancho}x{alto}"));
    }
}

#[test]
fn instantaneas_de_flota_con_fallos() {
    let (mut prueba, sembrado) = AppPrueba::con_semilla(100, 30);
    let web = sembrado.host("hetzner-01");
    prueba.evento(Evento::Sondeo(Sondeo {
        host_id: web,
        fecha: (chrono::Utc::now() - chrono::Duration::seconds(60))
            .format("%Y-%m-%dT%H:%M:%S+00:00")
            .to_string(),
        resultado: ResultadoSondeo::Ok,
        nucleos: Some(8),
        carga_1m: Some(0.5),
        mem_total_kb: Some(8_000_000),
        mem_disponible_kb: Some(6_000_000),
        disco_total_kb: Some(100_000_000),
        disco_usado_kb: Some(20_000_000),
        uptime_seg: Some(100_000),
        fallos: Some(FallosSondeo {
            unidades: FallosUnidades {
                total: 1,
                nombres: vec!["copia.service".to_string()],
            },
            contenedores: Some(FallosContenedores {
                motor: Some(MotorNombre::Docker),
                total: 1,
                nombres: vec!["web-worker-1".to_string()],
            }),
        }),
        ..Sondeo::default()
    }));
    prueba.tecla(KeyCode::F(1));
    let indice = prueba
        .app
        .hosts_flota()
        .iter()
        .position(|indice| prueba.app.hosts[*indice].id == web)
        .expect("hetzner-01 en Flota");
    prueba.app.seleccion_flota = indice;
    prueba.tecla(KeyCode::F(1));
    instantanea(&prueba, "flota_fallos_100x30");
}

#[test]
fn instantaneas_del_detalle() {
    for (ancho, alto) in TAMANOS {
        let (mut prueba, _sembrado, host_id) = con_vista(ancho, alto);
        seleccionar(&mut prueba, host_id, "app.service");
        prueba.tecla(KeyCode::Enter);
        let peticion = prueba
            .enviados()
            .into_iter()
            .find_map(|mensaje| match mensaje {
                MensajeCliente::DetalleServicio { peticion_id, .. } => Some(peticion_id),
                _ => None,
            })
            .expect("faltó DetalleServicio");
        prueba.servidor(MensajeServidor::Detalle {
            peticion_id: peticion,
            host_id,
            detalle: detalle(),
        });
        instantanea(&prueba, &format!("servicios_detalle_{ancho}x{alto}"));
    }
}

#[test]
fn instantaneas_del_panel_de_logs() {
    for (ancho, alto) in TAMANOS {
        let (mut prueba, _sembrado, host_id) = con_vista(ancho, alto);
        seleccionar(&mut prueba, host_id, "app.service");
        prueba.tecla(KeyCode::Char('l'));
        let peticion = prueba
            .enviados()
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
                linea(6, "INFO arrancando app de prueba"),
                linea(4, "WARNING Lentitud: GET /api/informes tardó 2,4 s"),
                linea(3, "ERROR Exception in ASGI application"),
                linea(3, "Traceback (most recent call last):"),
                linea(3, "KeyError: 'DATABASE_URL'"),
            ],
            omitidas: 14,
        });
        instantanea(&prueba, &format!("servicios_logs_{ancho}x{alto}"));
    }
}

/// Siembra fallos en el último sondeo de dos hosts y abre «Toda la flota».
fn con_toda_la_flota(cols: u16, filas: u16) -> (AppPrueba, Sembrado) {
    let (mut prueba, sembrado) = AppPrueba::con_semilla(cols, filas);
    let web = sembrado.host("hetzner-01");
    let db = sembrado.host("vps-openclaw");
    let fecha = |segundos: i64| {
        (chrono::Utc::now() - chrono::Duration::seconds(segundos))
            .format("%Y-%m-%dT%H:%M:%S+00:00")
            .to_string()
    };
    let sondeo = |host_id: i64, fallos: FallosSondeo, hace: i64| Sondeo {
        host_id,
        fecha: fecha(hace),
        resultado: ResultadoSondeo::Ok,
        nucleos: Some(8),
        carga_1m: Some(0.5),
        mem_total_kb: Some(8_000_000),
        mem_disponible_kb: Some(6_000_000),
        disco_total_kb: Some(100_000_000),
        disco_usado_kb: Some(20_000_000),
        uptime_seg: Some(100_000),
        fallos: Some(fallos),
        ..Sondeo::default()
    };
    prueba.evento(Evento::Sondeo(sondeo(
        web,
        FallosSondeo {
            unidades: FallosUnidades {
                total: 1,
                nombres: vec!["copia.service".to_string()],
            },
            contenedores: Some(FallosContenedores {
                motor: Some(MotorNombre::Docker),
                total: 2,
                nombres: vec!["web-worker-1".to_string(), "panel".to_string()],
            }),
        },
        40,
    )));
    prueba.evento(Evento::Sondeo(sondeo(
        db,
        FallosSondeo {
            unidades: FallosUnidades {
                total: 1,
                nombres: vec!["exportar.service".to_string()],
            },
            contenedores: None,
        },
        90,
    )));
    prueba.tecla(KeyCode::F(9));
    prueba.tecla(KeyCode::Up);
    assert_eq!(prueba.app.servicios_host_seleccionado(), None);
    (prueba, sembrado)
}

#[test]
fn instantaneas_de_toda_la_flota() {
    for (ancho, alto) in TAMANOS {
        let (prueba, _sembrado) = con_toda_la_flota(ancho, alto);
        instantanea(&prueba, &format!("servicios_flota_{ancho}x{alto}"));
    }
}

#[test]
fn instantaneas_de_la_confirmacion_y_el_resultado() {
    for (ancho, alto) in TAMANOS {
        let (mut prueba, _sembrado, host_id) = con_vista(ancho, alto);
        seleccionar(&mut prueba, host_id, "nginx.service");
        prueba.tecla(KeyCode::Char('G'));
        instantanea(&prueba, &format!("servicios_confirmacion_{ancho}x{alto}"));
        prueba.tecla(KeyCode::Enter);
        let peticion = prueba
            .enviados()
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
                lineas: vec![
                    "21:52:10 app[4121]: Traceback (most recent call last):".to_string(),
                    "21:52:10 app[4121]: KeyError: 'DATABASE_URL'".to_string(),
                    "21:52:10 systemd[1]: app.service: Main process exited, status=1".to_string(),
                ],
                comprobacion: Some("nginx -t".to_string()),
            },
        });
        instantanea(&prueba, &format!("servicios_resultado_{ancho}x{alto}"));
    }
}

#[test]
fn instantaneas_del_dialogo_de_sudo() {
    for (ancho, alto) in TAMANOS {
        let (mut prueba, sembrado) = AppPrueba::con_semilla(ancho, alto);
        let host_id = sembrado.host("hetzner-01");
        prueba.tecla(KeyCode::F(2));
        prueba.app.seleccionar_host_id(host_id);
        prueba.tecla(KeyCode::Char('u'));
        let mut listado = listado();
        listado.privilegio = Privilegio::SudoConClave;
        listado.unidades = vec![unidad("app.service", "failed", "enabled", false)];
        responder_listado(&mut prueba, host_id, listado);
        seleccionar(&mut prueba, host_id, "app.service");
        prueba.tecla(KeyCode::Char('A'));
        instantanea(&prueba, &format!("servicios_sudo_{ancho}x{alto}"));
    }
}

#[test]
fn instantaneas_de_la_deliberacion_de_servicio() {
    for (ancho, alto) in TAMANOS {
        let (mut prueba, _sembrado, host_id) = con_vista(ancho, alto);
        seleccionar(&mut prueba, host_id, "nginx.service");
        prueba.tecla(KeyCode::Char('R'));
        assert!(prueba.app.deliberacion.is_some());
        instantanea(&prueba, &format!("servicios_deliberacion_{ancho}x{alto}"));
    }
}

#[test]
fn instantaneas_del_editor_de_comprobacion() {
    for (ancho, alto) in TAMANOS {
        let (mut prueba, _sembrado, host_id) = con_vista(ancho, alto);
        prueba.tecla(KeyCode::Char('3'));
        seleccionar(&mut prueba, host_id, "web");
        prueba.tecla(KeyCode::Char('c'));
        instantanea(&prueba, &format!("servicios_comprobacion_{ancho}x{alto}"));
    }
}

#[test]
fn redimensionar_conserva_la_seleccion_y_la_peticion() {
    let (mut prueba, _sembrado, host_id) = con_vista(200, 60);
    seleccionar(&mut prueba, host_id, "copia.service");
    for (ancho, alto) in [(80, 24), (40, 12), (200, 60)] {
        prueba.pasar_por(ancho, alto);
        assert_eq!(prueba.app.vista, Vista::Servicios);
        assert_eq!(
            prueba.app.servicios_elemento_seleccionado(host_id),
            Some(magi::app::servicios::Elemento::Unidad(
                "copia.service".to_string()
            ))
        );
    }
}

#[test]
fn la_barra_y_la_ayuda_mencionan_la_vista() {
    let (mut prueba, _sembrado, _host) = con_vista(80, 24);
    let texto = prueba.texto();
    assert!(texto.contains("SERVICIOS"), "texto: {texto}");
    prueba.tecla(KeyCode::Char('?'));
    let texto = prueba.texto();
    assert!(texto.contains("AYUDA · SERVICIOS"), "texto: {texto}");
}
