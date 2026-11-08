//! Pruebas de la lectura de la Fase 13: salidas grabadas de varias
//! distribuciones y motores, límites de la entrada no fiable, nombres
//! validados, acciones permitidas y validadores.

use magi_nucleo::servicios::lectura::{self, ErrorLectura};
use magi_nucleo::servicios::{
    acciones, limites, orden, validadores, Accion, EstadoArranque, EstadoContenedor, Listado,
    MotorEstado, MotorNombre, Objetivo, Privilegio, ResultadoAccion, Salud, Systemd, TipoObjetivo,
};

/// Salida grabada de un script de prueba.
fn fijo(nombre: &str) -> String {
    let ruta = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fijos/servicios")
        .join(nombre);
    std::fs::read_to_string(&ruta).unwrap_or_else(|error| panic!("falta {nombre}: {error}"))
}

fn unidad_de<'a>(listado: &'a Listado, nombre: &str) -> &'a magi_nucleo::servicios::Unidad {
    listado
        .unidades
        .iter()
        .find(|unidad| unidad.nombre == nombre)
        .unwrap_or_else(|| panic!("no está {nombre}"))
}

fn contenedor_de<'a>(listado: &'a Listado, nombre: &str) -> &'a magi_nucleo::servicios::Contenedor {
    listado
        .contenedores
        .iter()
        .find(|contenedor| contenedor.nombre == nombre)
        .unwrap_or_else(|| panic!("no está el contenedor {nombre}"))
}

fn vigiladas_debian() -> Vec<String> {
    vec![
        "nginx.service".to_string(),
        "app.service".to_string(),
        "postgresql.service".to_string(),
    ]
}

#[test]
fn listado_debian12_lee_systemd_propiedades_y_privilegios() {
    let salida = fijo("listar_debian12.txt");
    let listado = lectura::listado(&salida, &vigiladas_debian()).expect("listado válido");
    assert_eq!(listado.systemd, Systemd::Disponible("252".to_string()));
    assert_eq!(listado.privilegio, Privilegio::SudoConClave);
    assert_eq!(listado.diario, magi_nucleo::servicios::PermisoDiario::Todo);
    assert_eq!(listado.reloj.epoch, 1794141500);
    assert_eq!(listado.reloj.uptime_seg, 86412);
    assert_eq!(
        listado.motor,
        MotorEstado::Disponible(magi_nucleo::servicios::Motor {
            nombre: MotorNombre::Docker,
            version: "27.3.1".to_string(),
        })
    );
    assert!(!listado.incompleto);
    assert!(listado.avisos.is_empty());

    // Las plantillas no se listan; sus instancias sí.
    assert!(!listado
        .unidades
        .iter()
        .any(|unidad| unidad.nombre == "app@.service"));
    assert!(listado
        .unidades
        .iter()
        .any(|unidad| unidad.nombre == "app@1.service"));

    let nginx = unidad_de(&listado, "nginx.service");
    assert_eq!(nginx.activo(), "active");
    assert_eq!(nginx.subestado(), "running");
    assert!(nginx.puede_recargar());
    assert!(nginx.vigilada);
    assert_eq!(nginx.descripcion(), "A high performance web server");
    assert_eq!(nginx.pid_principal(), Some(1201));
    assert_eq!(nginx.memoria_bytes(), Some(8_806_400));
    assert_eq!(nginx.fichero(), Some("/lib/systemd/system/nginx.service"));
    assert_eq!(nginx.arranque(), EstadoArranque::Activado);
    let desde = nginx.desde_epoch(listado.reloj).expect("desde");
    assert_eq!(desde, 1794056286);

    let copia = unidad_de(&listado, "copia.service");
    assert_eq!(copia.activo(), "failed");
    assert_eq!(copia.resultado(), "exit-code");
    assert_eq!(copia.tipo(), "oneshot");
    assert!(!copia.vigilada);

    let tareas = unidad_de(&listado, "tareas.service");
    assert_eq!(texto_estado(tareas), "inactivo");
    assert_eq!(tareas.arranque(), EstadoArranque::Desactivado);

    // Unidades instaladas sin cargar: se pinta inactiva y se puede arrancar.
    let ssh = unidad_de(&listado, "ssh.service");
    assert!(!ssh.vigilada);

    // Timers con su servicio y sus horas.
    assert_eq!(listado.timers.len(), 2);
    let copia_timer = listado
        .timers
        .iter()
        .find(|timer| timer.nombre == "copia.timer")
        .expect("copia.timer");
    assert_eq!(copia_timer.servicio.as_deref(), Some("copia.service"));
    assert_eq!(copia_timer.proxima_epoch(), Some(1_794_220_800));
    assert_eq!(
        copia_timer.ultima_epoch(listado.reloj),
        Some(1794141500 - (86412 - 1600))
    );
    let logrotate = listado
        .timers
        .iter()
        .find(|timer| timer.nombre == "logrotate.timer")
        .expect("logrotate.timer");
    assert_eq!(logrotate.servicio, None);
    assert_eq!(
        logrotate.servicio_activado().as_deref(),
        Some("logrotate.service")
    );

    // Contenedores: etiquetas de Compose, salud, código de salida y puertos.
    assert_eq!(listado.contenedores.len(), 4);
    let app = contenedor_de(&listado, "web-app-1");
    assert_eq!(app.id, "a1b2c3d4e5f6");
    assert_eq!(app.imagen, "nginx:1.27.3");
    assert_eq!(app.estado, EstadoContenedor::EnMarcha);
    assert_eq!(app.salud, Salud::Sana);
    assert_eq!(app.proyecto.as_deref(), Some("web"));
    assert_eq!(app.servicio.as_deref(), Some("app"));
    assert_eq!(app.orden.as_deref(), Some("1"));
    // La etiqueta config_files lleva una coma dentro del valor: las dos rutas.
    assert_eq!(
        app.ficheros,
        vec![
            "/srv/web/docker-compose.yml".to_string(),
            "/srv/web/docker-compose.override.yml".to_string()
        ]
    );
    assert!(app.puertos.contains("8080"));
    assert!(app.admite_shell());
    assert!(!app.caido());

    let worker = contenedor_de(&listado, "web-worker-1");
    assert_eq!(worker.estado, EstadoContenedor::Parado);
    assert_eq!(worker.codigo_salida, Some(1));
    assert!(worker.caido());
    assert!(!worker.admite_shell());

    let panel = contenedor_de(&listado, "panel");
    assert_eq!(panel.salud, Salud::Insegura);
    assert!(panel.caido());
    assert_eq!(panel.proyecto, None);

    // Proyecto Compose con sus ficheros.
    assert_eq!(listado.proyectos.len(), 1);
    assert_eq!(listado.proyectos[0].nombre, "web");
    assert_eq!(listado.proyectos[0].ficheros.len(), 2);
    assert!(listado.proyectos[0].con_ficheros());
}

fn texto_estado(unidad: &magi_nucleo::servicios::Unidad) -> &'static str {
    acciones::texto_estado_unidad(unidad)
}

#[test]
fn listado_ubuntu2404_lee_otra_version_y_compose() {
    let salida = fijo("listar_ubuntu2404.txt");
    let listado = lectura::listado(&salida, &[]).expect("listado válido");
    assert_eq!(listado.systemd, Systemd::Disponible("255".to_string()));
    assert_eq!(listado.privilegio, Privilegio::SudoSinClave);
    assert_eq!(
        listado
            .motor
            .disponible()
            .map(|motor| motor.version.as_str()),
        Some("27.5.1")
    );
    let caddy = unidad_de(&listado, "caddy.service");
    assert!(caddy.puede_recargar());
    assert!(caddy
        .prop("ExecStart")
        .contains("--config /etc/caddy/Caddyfile"));
    let httpd = unidad_de(&listado, "httpd.service");
    assert_eq!(httpd.prop("LoadState"), "not-found");
    assert!(!httpd.enmascarada());
    assert!(!acciones::unidad_cargada(httpd));
    assert_eq!(listado.contenedores.len(), 2);
    let migrador = contenedor_de(&listado, "tienda-migrador-1");
    assert_eq!(migrador.estado, EstadoContenedor::Parado);
    assert_eq!(migrador.codigo_salida, Some(0));
    assert!(!migrador.caido());
    assert_eq!(listado.proyectos.len(), 1);
    assert_eq!(listado.proyectos[0].nombre, "tienda");
    assert_eq!(
        listado.proyectos[0].ficheros,
        vec!["/opt/tienda/compose.yaml".to_string()]
    );
}

#[test]
fn listado_rocky9_con_podman_sin_sudo() {
    let salida = fijo("listar_rocky9.txt");
    let listado = lectura::listado(&salida, &[]).expect("listado válido");
    assert_eq!(listado.privilegio, Privilegio::Root);
    assert_eq!(
        listado.motor,
        MotorEstado::Disponible(magi_nucleo::servicios::Motor {
            nombre: MotorNombre::Podman,
            version: "5.2.3".to_string(),
        })
    );
    // El contenedor infra de los pods de Podman no se lista.
    assert!(!listado
        .contenedores
        .iter()
        .any(|contenedor| contenedor.nombre == "abc-infra"));
    assert_eq!(listado.contenedores.len(), 2);
    let redis = contenedor_de(&listado, "prueba-redis");
    assert_eq!(redis.codigo_salida, Some(137));
    assert_eq!(redis.estado, EstadoContenedor::Parado);
    assert!(redis.caido());
    let db = contenedor_de(&listado, "datos-db-1");
    assert_eq!(db.salud, Salud::Sana);
    assert_eq!(db.id, "9a8b7c6d5e4f");
    assert_eq!(listado.proyectos.len(), 1);
}

#[test]
fn listado_sin_systemd_solo_tiene_contenedores() {
    let listado = lectura::listado(&fijo("listar_sin_systemd.txt"), &[]).expect("listado");
    assert_eq!(listado.systemd, Systemd::Ausente);
    assert!(listado.unidades.is_empty());
    assert!(listado.timers.is_empty());
    assert_eq!(listado.contenedores.len(), 1);
    assert_eq!(listado.contenedores[0].nombre, "web");
}

#[test]
fn listado_con_systemd_antiguo_no_lista_unidades() {
    let listado = lectura::listado(&fijo("listar_antiguo.txt"), &[]).expect("listado");
    assert_eq!(listado.systemd, Systemd::Antiguo("219".to_string()));
    assert!(listado.unidades.is_empty());
    assert!(listado.timers.is_empty());
    assert_eq!(listado.motor, MotorEstado::No);
}

#[test]
fn listado_sin_permiso_para_docker() {
    let listado = lectura::listado(&fijo("listar_sin_permiso.txt"), &[]).expect("listado");
    assert_eq!(listado.motor, MotorEstado::SinPermiso);
    assert!(listado.contenedores.is_empty());
    assert_eq!(
        listado.diario,
        magi_nucleo::servicios::PermisoDiario::Propio
    );
    assert_eq!(listado.unidades.len(), 1);
}

fn bloque_unidad(nombre: &str, estado: &str) -> String {
    format!(
        "MAGI_BLOQUE service
Id={nombre}
Names={nombre}
Description=Unidad {nombre}
LoadState=loaded
ActiveState={estado}
SubState=dead
Type=simple
UnitFileState=enabled
CanReload=no
MAGI_FIN_BLOQUE
"
    )
}

#[test]
fn cinco_mil_unidades_se_recortan_a_dos_mil_con_aviso() {
    let mut salida = String::from(
        "MAGI_PRIV=root
MAGI_SYSTEMD=252
",
    );
    for indice in 0..5000 {
        salida.push_str(&bloque_unidad(
            &format!("unidad{indice}.service"),
            "inactive",
        ));
    }
    salida.push_str(
        "MAGI_DIARIO=todo
MAGI_RELOJ=1794141500 100
MAGI_MOTOR=no
MAGI_FIN
",
    );
    let listado = lectura::listado(&salida, &[]).expect("listado");
    assert_eq!(listado.unidades.len(), limites::MAX_UNIDADES);
    assert!(listado.incompleto);
    assert_eq!(listado.unidades[0].nombre, "unidad0.service");
    assert_eq!(
        listado.unidades[limites::MAX_UNIDADES - 1].nombre,
        "unidad1999.service"
    );
}

#[test]
fn una_salida_de_mas_de_dos_mebibytes_se_recorta_sin_panico() {
    let mut salida = String::from(
        "MAGI_SYSTEMD=252
MAGI_BLOQUE service
Id=uno.service
",
    );
    salida.push_str("Description=");
    salida.push_str(&"á".repeat(limites::LISTADO_MAX_BYTES / 2 + 1000));
    let listado = lectura::listado(&salida, &[]).expect("listado");
    assert!(listado.incompleto);
    // El carácter partido por el corte no revienta la lectura.
    assert!(!listado.incompleto || listado.unidades.len() <= limites::MAX_UNIDADES);
}

#[test]
fn listado_con_basura_no_provoca_panico() {
    let salida = concat!(
        "MAGI_PRIV=privilegio inventado
",
        "MAGI_SYSTEMD=versión rara
",
        "MAGI_DIARIO=otro
",
        "MAGI_RELOJ=no es un número
",
        "MAGI_MOTOR=orquestador 1.0
",
        "MAGI_CONTENEDOR {esto no es json}
",
        "MAGI_CONTENEDOR {}
",
        "MAGI_PROYECTO {\"nombre\":\"MAL Nombre\"}
",
        "MAGI_PROYECTO sin json
",
        "MAGI_BLOQUE servicio
Id=
=valor
clave sin igual
MAGI_FIN_BLOQUE
",
        "MAGI_AVISO aviso de prueba
",
        "MAGI_FIN
"
    );
    let listado = lectura::listado(salida, &[]).expect("listado degradado");
    assert_eq!(listado.systemd, Systemd::Ausente);
    assert_eq!(listado.motor, MotorEstado::No);
    assert!(listado.contenedores.is_empty());
    assert!(listado.proyectos.is_empty());
    assert_eq!(listado.avisos, vec!["aviso de prueba".to_string()]);
}

#[test]
fn consumo_de_docker_y_podman() {
    let docker = lectura::consumo(&fijo("consumo_docker27.txt")).expect("consumo docker");
    assert_eq!(docker.len(), 2);
    assert_eq!(docker[0].id, "a1b2c3d4e5f6");
    assert_eq!(docker[0].cpu_pct, Some(0.12));
    assert_eq!(docker[0].memoria_bytes, Some(12_897_485));
    assert_eq!(docker[0].memoria_limite_bytes, Some(8_160_437_862));
    assert_eq!(docker[1].cpu_pct, Some(2.5));

    let podman = lectura::consumo(&fijo("consumo_podman5.txt")).expect("consumo podman");
    assert_eq!(podman.len(), 1);
    assert_eq!(podman[0].id, "9a8b7c6d5e4f");
    assert_eq!(podman[0].cpu_pct, Some(0.45));
    assert_eq!(podman[0].memoria_bytes, Some(120_500_000));
    assert_eq!(podman[0].memoria_limite_bytes, Some(2_000_000_000));

    // Basura: ni una entrada, ni un pánico.
    assert!(lectura::consumo(
        "MAGI_CONSUMO {}
MAGI_CONSUMO [1,2]
MAGI_FIN
"
    )
    .expect("consumo degradado")
    .is_empty());
}

#[test]
fn detalle_de_una_unidad_con_dependencias_y_texto() {
    let objetivo = Objetivo::unidad("nginx.service");
    let detalle =
        lectura::detalle(&fijo("detalle_nginx_debian12.txt"), &objetivo).expect("detalle");
    assert_eq!(detalle.objetivo, objetivo);
    assert_eq!(
        detalle.props.get("ActiveState").map(String::as_str),
        Some("active")
    );
    assert_eq!(
        detalle.props.get("CanReload").map(String::as_str),
        Some("yes")
    );
    assert_eq!(
        detalle.necesita,
        vec![
            "system.slice".to_string(),
            "network.target".to_string(),
            "nginx.pid".to_string(),
        ]
    );
    assert_eq!(detalle.necesitada, vec!["multi-user.target".to_string()]);
    assert!(detalle.texto.contains("ExecReload="));
    assert!(!detalle.texto.contains("MAGI_FIN"));
    assert!(detalle.contenedor.is_none());
}

#[test]
fn detalle_de_un_contenedor_lee_el_inspect() {
    let objetivo = Objetivo::contenedor("a1b2c3d4e5f6");
    let detalle =
        lectura::detalle(&fijo("detalle_contenedor_docker27.txt"), &objetivo).expect("detalle");
    let contenedor = detalle.contenedor.expect("inspect leído");
    assert!(contenedor.id.starts_with("a1b2c3d4e5f6"));
    assert_eq!(contenedor.nombre, "web-app-1");
    assert_eq!(contenedor.imagen, "nginx:1.27.3");
    assert_eq!(contenedor.estado, EstadoContenedor::EnMarcha);
    assert_eq!(contenedor.codigo_salida, Some(0));
    assert_eq!(contenedor.reinicios, 2);
    assert_eq!(contenedor.salud, Salud::Sana);
    assert_eq!(contenedor.puertos.len(), 1);
    assert!(contenedor.puertos[0].contains("8080"));
    assert_eq!(contenedor.montajes.len(), 1);
    assert!(contenedor.montajes[0].contains("/srv/web/datos"));
    assert_eq!(contenedor.redes, vec!["web_default".to_string()]);
    assert_eq!(contenedor.proyecto.as_deref(), Some("web"));
    assert_eq!(contenedor.orden.as_deref(), Some("1"));
    assert!(contenedor.texto.contains("Id"));
}

#[test]
fn detalle_con_inspect_malo_devuelve_error_sin_panico() {
    let objetivo = Objetivo::contenedor("a1b2c3d4e5f6");
    assert!(lectura::detalle_contenedor("no es json", &objetivo).is_err());
    assert!(lectura::detalle_contenedor("[]", &objetivo).is_err());
    assert!(lectura::detalle_contenedor("{}", &objetivo).is_ok());
}

#[test]
fn acciones_leidas_de_los_fijos() {
    let objetivo = Objetivo::unidad("app.service");
    let hecho = lectura::accion(&fijo("accion_hecho.txt"), &objetivo).expect("hecho");
    assert_eq!(hecho.resultado, ResultadoAccion::Hecho);
    assert_eq!(hecho.estado, "active");
    assert_eq!(hecho.segundos, 4.0);
    assert!(hecho.mensaje.is_empty());
    assert!(hecho.lineas.is_empty());

    let fallido = lectura::accion(&fijo("accion_fallido.txt"), &objetivo).expect("fallido");
    assert_eq!(fallido.resultado, ResultadoAccion::Fallido);
    assert_eq!(fallido.estado, "failed");
    assert_eq!(fallido.segundos, 3.0);
    assert!(fallido.mensaje.contains("Job for app.service failed"));
    assert_eq!(fallido.lineas.len(), 5);
    assert!(fallido.lineas[2].contains("KeyError"));

    let rechazada = lectura::accion(
        &fijo("accion_rechazada.txt"),
        &Objetivo::unidad("nginx.service"),
    )
    .expect("rechazada");
    assert_eq!(rechazada.resultado, ResultadoAccion::RechazadaConfiguracion);
    assert_eq!(rechazada.estado, "");
    assert_eq!(rechazada.lineas.len(), 2);
    assert!(
        lectura::accion(&fijo("accion_sin_confirmar.txt"), &objetivo)
            .expect("sin confirmar")
            .resultado
            == ResultadoAccion::SinConfirmar
    );

    assert_eq!(
        lectura::accion(
            "MAGI_FASE=ejecutando
MAGI_FIN
",
            &objetivo
        )
        .err(),
        Some(ErrorLectura("falta MAGI_RESULTADO".to_string()))
    );
    // La fase y la comprobación se leen para difundirlas en el servidor.
    assert_eq!(
        lectura::fase("MAGI_FASE=comprobando"),
        Some(magi_nucleo::servicios::FaseAccion::Comprobando)
    );
    assert_eq!(lectura::fase("otra cosa"), None);
    assert_eq!(
        lectura::estado_comprobacion("MAGI_COMPROBACION=ok"),
        Some(Ok(()))
    );
    assert_eq!(
        lectura::estado_comprobacion("MAGI_COMPROBACION=fallo 127"),
        Some(Err(127))
    );
}

#[test]
fn lineas_del_diario_en_json() {
    let lineas: Vec<_> = fijo("journal_debian12.jsonl")
        .lines()
        .map(lectura::linea_diario)
        .collect();
    assert_eq!(lineas.len(), 4);
    let primera = lineas[0].as_ref().expect("primera");
    assert_eq!(primera.hora, Some(1_794_139_824));
    assert_eq!(primera.prioridad, 6);
    assert!(!primera.stderr);
    assert!(primera.texto.contains("Started"));
    let error = lineas[1].as_ref().expect("error");
    assert_eq!(error.prioridad, 3);
    assert!(error.es_error());
    assert!(error.stderr);
    assert!(lineas[2].as_ref().expect("aviso").es_aviso());
    assert_eq!(lineas[3].as_ref().expect("debug").prioridad, 7);

    // MESSAGE como lista de bytes, prioridades raras y basura.
    let ubuntu: Vec<_> = fijo("journal_ubuntu2404.jsonl")
        .lines()
        .map(lectura::linea_diario)
        .collect();
    assert_eq!(ubuntu[0].as_ref().expect("bytes").texto, "Tiempo agotado");
    assert_eq!(
        ubuntu[1].as_ref().expect("texto").texto,
        "Plazo agotado al llamar a https://api.ejemplo.org/pagos"
    );
    assert!(ubuntu[2].is_none(), "una línea que no es JSON se descarta");
    assert_eq!(ubuntu[3].as_ref().expect("sin hora").hora, None);
    // Una prioridad fuera de rango se queda en el valor por defecto.
    assert_eq!(ubuntu[4].as_ref().expect("prioridad rara").prioridad, 6);
    assert!(ubuntu[5].is_none(), "sin MESSAGE no hay línea");
}

#[test]
fn lineas_de_log_de_un_contenedor() {
    let lineas: Vec<_> = fijo("logs_docker27.txt")
        .lines()
        .map(lectura::linea_contenedor)
        .collect();
    assert_eq!(lineas.len(), 4);
    let salida = lineas[0].as_ref().expect("stdout");
    assert!(!salida.stderr);
    assert_eq!(salida.prioridad, 6);
    assert!(salida.hora.is_some());
    assert!(salida.texto.contains("GET /"));
    let error = lineas[2].as_ref().expect("stderr");
    assert!(error.stderr);
    assert_eq!(error.prioridad, 3);
    assert!(error.texto.contains("upstream timed out"));
    let sin_hora = lineas[3].as_ref().expect("sin hora");
    assert_eq!(sin_hora.hora, None);
    assert_eq!(sin_hora.texto, "linea sin marca de tiempo");

    // El despachador elige el parser según el objetivo.
    assert!(lectura::linea_de_log("MAGI_LOG out - hola", true).is_some());
    assert!(lectura::linea_de_log("MAGI_LOG out - hola", false).is_none());
}

#[test]
fn privilegio_y_sudo_se_leen_y_se_rechazan_los_malos() {
    let (uid, privilegio) = lectura::privilegio(
        "MAGI_UID=1000
MAGI_PRIV=sudo_con_clave
MAGI_FIN
",
    )
    .expect("bueno");
    assert_eq!(uid, 1000);
    assert_eq!(privilegio, Privilegio::SudoConClave);
    for malo in [
        "",
        "MAGI_FIN
",
        "MAGI_UID=x
MAGI_PRIV=root
",
        "MAGI_UID=0
MAGI_PRIV=otro
",
    ] {
        assert!(lectura::privilegio(malo).is_err(), "debía fallar: {malo}");
    }
    assert_eq!(
        lectura::sudo(
            "MAGI_SUDO=ok
MAGI_FIN
"
        ),
        Ok(lectura::ComprobacionSudo::Ok)
    );
    assert_eq!(
        lectura::sudo(
            "MAGI_SUDO=rechazada
MAGI_FIN
"
        ),
        Ok(lectura::ComprobacionSudo::Rechazada)
    );
    assert_eq!(
        lectura::sudo(
            "MAGI_SUDO=sin_sudo
MAGI_FIN
"
        ),
        Ok(lectura::ComprobacionSudo::SinSudo)
    );
    assert!(lectura::sudo(
        "MAGI_SUDO=quizá
"
    )
    .is_err());
    assert!(lectura::sudo("").is_err());
}

// ------------------------------------------------------------------ nombres
#[test]
fn nombres_de_unidad_validos_e_invalidos() {
    for bueno in [
        "nginx.service",
        "app@1.service",
        "wg-quick@wg0.service",
        "copia.timer",
        "a-1_2@.service",
    ] {
        assert!(orden::nombre_unidad_valido(bueno), "debía valer: {bueno}");
    }
    for malo in [
        "nginx.service; rm -rf /",
        "nginx",
        "..",
        "nginx.service ",
        "Nginx Service.service",
        "ñ.service",
        "",
        "nginx.socket",
        "nginx.service
",
    ] {
        assert!(!orden::nombre_unidad_valido(malo), "no debía valer: {malo}");
    }
    // El AC: un objetivo con punto y coma se rechaza en Rust antes de ejecutar.
    let inyectado = Objetivo::unidad("nginx.service; rm -rf /");
    let error = orden::validar_objetivo(&inyectado).expect_err("se rechaza");
    assert!(error.contains("nombre no admitido"), "mensaje: {error}");
    assert!(orden::validar_objetivo(&Objetivo::unidad("nginx.service")).is_ok());
}

#[test]
fn nombres_de_contenedor_motor_y_proyecto() {
    assert!(orden::nombre_contenedor_valido("a1b2c3d4e5f6"));
    assert!(orden::nombre_contenedor_valido(
        "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d"
    ));
    assert!(orden::nombre_contenedor_valido("web-app-1"));
    assert!(orden::nombre_contenedor_valido("Web_1"));
    assert!(!orden::nombre_contenedor_valido(".web"));
    assert!(!orden::nombre_contenedor_valido("-web"));
    assert!(!orden::nombre_contenedor_valido("web app"));
    assert!(!orden::nombre_contenedor_valido("web/1"));
    assert!(!orden::nombre_contenedor_valido(""));
    assert!(orden::nombre_proyecto_valido("web"));
    assert!(orden::nombre_proyecto_valido("web-2"));
    assert!(!orden::nombre_proyecto_valido("Web"));
    assert!(!orden::nombre_proyecto_valido("-web"));
    assert!(!orden::nombre_proyecto_valido("web dos"));
    assert!(orden::ruta_compose_valida("/srv/web/docker-compose.yml"));
    assert!(!orden::ruta_compose_valida("srv/web/docker-compose.yml"));
    assert!(!orden::ruta_compose_valida("/srv/web/../etc/passwd"));
    assert!(!orden::ruta_compose_valida("/srv/web/docker compose.yml"));
}

#[test]
fn patrones_de_unidades_criticas() {
    assert!(orden::patron_critica_valido("sshd.service"));
    assert!(orden::patron_critica_valido("wg-quick@*.service"));
    assert!(!orden::patron_critica_valido(""));
    assert!(!orden::patron_critica_valido("ssh service"));
    assert!(!orden::patron_critica_valido("a".repeat(257).as_str()));
    assert!(orden::coincide_patron("sshd.service", "sshd.service"));
    assert!(!orden::coincide_patron("sshd.service", "ssh.service"));
    assert!(orden::coincide_patron(
        "wg-quick@*.service",
        "wg-quick@wg0.service"
    ));
    assert!(!orden::coincide_patron(
        "wg-quick@*.service",
        "wg-quick@wg0.timer"
    ));
    assert!(orden::coincide_patron("*.service", "anything.service"));
    assert!(orden::coincide_patron("*", "lo que sea"));
    let criticas = limites::CRITICAS_POR_DEFECTO
        .iter()
        .map(|patron| patron.to_string())
        .collect::<Vec<_>>();
    assert!(acciones::es_critica(&criticas, "ssh.service"));
    assert!(acciones::es_critica(&criticas, "wg-quick@wg0.service"));
    assert!(!acciones::es_critica(&criticas, "app.service"));
    assert_eq!(
        orden::validar_criticas(&["mal patron".to_string()]).len(),
        1
    );
}

#[test]
fn composicion_de_las_ordenes_remotas() {
    let suborden = orden::Suborden::Privilegio;
    let comando = orden::comando(&suborden, Privilegio::SudoConClave);
    assert!(comando.starts_with("sh -c '"));
    assert!(comando.contains("magi-servicios"));
    assert!(
        comando.contains("privilegio 'sudo_con_clave'")
            || comando.contains("privilegio sudo_con_clave")
    );
    assert!(orden::comando(&orden::Suborden::Sudo, Privilegio::Root)
        .contains("magi-servicios sudo root"));

    let accion = orden::Suborden::Accion {
        objetivo: Objetivo::unidad("app.service"),
        accion: Accion::Reiniciar,
        espera_seg: 30,
        comprobacion: orden::Comprobacion::Propia("echo 'hola'".to_string()),
    };
    let argumentos = orden::argumentos(&accion);
    assert_eq!(
        argumentos,
        vec![
            "unidad".to_string(),
            "app.service".to_string(),
            "reiniciar".to_string(),
            "30".to_string(),
            "propia".to_string(),
            "echo 'hola'".to_string(),
        ]
    );
    let comando = orden::comando(&accion, Privilegio::SudoSinClave);
    assert!(comando.contains("accion"));
    assert!(comando.contains("'echo '\\''hola'\\'''") || comando.contains("hola"));
    assert_eq!(
        orden::argumentos(&orden::Suborden::Listar {
            motores: "docker,podman".to_string()
        }),
        vec!["docker,podman".to_string()]
    );
    assert_eq!(
        orden::argumentos(&orden::Suborden::Consumo {
            motor: MotorNombre::Podman
        }),
        vec!["podman".to_string()]
    );
    assert_eq!(
        orden::argumentos(&orden::Suborden::Detalle(Objetivo::unidad("app.service"))),
        vec!["unidad".to_string(), "app.service".to_string()]
    );
    assert_eq!(
        orden::argumentos(&orden::Suborden::Logs {
            objetivo: Objetivo::contenedor("a1b2c3d4e5f6"),
            lineas_iniciales: 200,
        }),
        vec![
            "contenedor".to_string(),
            "a1b2c3d4e5f6".to_string(),
            "200".to_string()
        ]
    );
    assert!(orden::entrecomillar("a'b").contains("a'\\''b"));
    assert_eq!(orden::motores_para_listar(&[]), "-");
    assert_eq!(
        orden::motores_para_listar(&[MotorNombre::Docker, MotorNombre::Podman]),
        "docker,podman"
    );
    assert!(orden::motores_validos("docker,podman"));
    assert!(!orden::motores_validos("lxc"));
}

#[test]
fn validacion_de_los_subordenes_antes_de_mandarlos() {
    assert!(orden::validar_suborden(&orden::Suborden::Privilegio).is_ok());
    assert!(orden::validar_suborden(&orden::Suborden::Listar {
        motores: "lxc".to_string()
    })
    .is_err());
    assert!(orden::validar_suborden(&orden::Suborden::Accion {
        objetivo: Objetivo::unidad("nginx.service; rm -rf /"),
        accion: Accion::Reiniciar,
        espera_seg: 30,
        comprobacion: orden::Comprobacion::Ninguna,
    })
    .is_err());
    assert!(orden::validar_suborden(&orden::Suborden::Accion {
        objetivo: Objetivo::unidad("nginx.service"),
        accion: Accion::Reiniciar,
        espera_seg: 0,
        comprobacion: orden::Comprobacion::Ninguna,
    })
    .is_err());
    assert!(orden::validar_suborden(&orden::Suborden::Accion {
        objetivo: Objetivo::unidad("nginx.service"),
        accion: Accion::Reiniciar,
        espera_seg: 30,
        comprobacion: orden::Comprobacion::Propia("echo hola".to_string()),
    })
    .is_ok());
    assert!(orden::validar_suborden(&orden::Suborden::Logs {
        objetivo: Objetivo::unidad("nginx.service"),
        lineas_iniciales: 5001,
    })
    .is_err());
}

#[test]
fn comprobacion_propia_validada() {
    assert!(orden::validar_comando_propio("nginx -t").is_ok());
    assert!(orden::validar_comando_propio("").is_err());
    assert!(orden::validar_comando_propio("   ").is_err());
    assert!(orden::validar_comando_propio(
        "echo hola
rm -rf /"
    )
    .is_err());
    assert!(orden::validar_comando_propio("echo hola\r").is_err());
    assert!(orden::validar_comando_propio(&"a".repeat(1025)).is_err());
    assert!(orden::validar_comando_propio(&"a".repeat(1024)).is_ok());
    assert!(orden::aviso_comando_propio("sudo nginx -t").is_some());
    assert!(orden::aviso_comando_propio("nginx -t").is_none());
}

#[test]
fn ordenes_de_las_pestanas_con_orden() {
    let shell = magi_nucleo::servicios::OrdenPestana::ShellContenedor {
        motor: MotorNombre::Docker,
        id: "a1b2c3d4e5f6".to_string(),
    };
    let con_clave = orden::comando_pestana(&shell, Privilegio::SudoConClave).expect("orden");
    assert!(con_clave.starts_with("sudo -- docker exec -it "));
    assert!(con_clave.contains("command -v bash"));
    let raiz = orden::comando_pestana(&shell, Privilegio::Root).expect("orden");
    assert!(raiz.starts_with("docker exec -it "));
    let podman = magi_nucleo::servicios::OrdenPestana::ShellContenedor {
        motor: MotorNombre::Podman,
        id: "a1b2c3d4e5f6".to_string(),
    };
    assert!(!orden::comando_pestana(&podman, Privilegio::SudoConClave)
        .expect("orden")
        .contains("sudo"));
    let diario = magi_nucleo::servicios::OrdenPestana::SeguirDiario {
        unidad: "app.service".to_string(),
    };
    assert_eq!(
        orden::comando_pestana(&diario, Privilegio::SudoSinClave).as_deref(),
        Some("sudo -n -- journalctl -fu 'app.service'")
    );
    let seguir = magi_nucleo::servicios::OrdenPestana::SeguirContenedor {
        motor: MotorNombre::Docker,
        id: "a1b2c3d4e5f6".to_string(),
    };
    assert!(orden::comando_pestana(&seguir, Privilegio::SinSudo)
        .expect("orden")
        .starts_with("docker logs -f "));
    assert!(orden::comando_pestana(&diario, Privilegio::Root).is_some());
    let invalido = magi_nucleo::servicios::OrdenPestana::SeguirDiario {
        unidad: "app.service; rm -rf /".to_string(),
    };
    assert_eq!(orden::comando_pestana(&invalido, Privilegio::Root), None);
    let contenedor_malo = magi_nucleo::servicios::OrdenPestana::ShellContenedor {
        motor: MotorNombre::Docker,
        id: "no vale".to_string(),
    };
    assert_eq!(
        orden::comando_pestana(&contenedor_malo, Privilegio::Root),
        None
    );
}

// ------------------------------------------------------------------ acciones
fn props(pares: &[(&str, &str)]) -> std::collections::BTreeMap<String, String> {
    pares
        .iter()
        .map(|(clave, valor)| (clave.to_string(), valor.to_string()))
        .collect()
}

fn unidad_de_prueba(
    nombre: &str,
    activo: &str,
    extra: &[(&str, &str)],
) -> magi_nucleo::servicios::Unidad {
    let mut propiedades = props(&[
        ("Id", nombre),
        ("LoadState", "loaded"),
        ("ActiveState", activo),
        ("SubState", "running"),
        ("Type", "simple"),
        ("Result", "success"),
    ]);
    for (clave, valor) in extra {
        propiedades.insert(clave.to_string(), valor.to_string());
    }
    magi_nucleo::servicios::Unidad {
        nombre: nombre.to_string(),
        props: propiedades,
        vigilada: false,
    }
}

fn contenedor_de_prueba(
    nombre: &str,
    estado: EstadoContenedor,
    salud: Salud,
    codigo: Option<i32>,
) -> magi_nucleo::servicios::Contenedor {
    magi_nucleo::servicios::Contenedor {
        id: "a1b2c3d4e5f6".to_string(),
        nombre: nombre.to_string(),
        imagen: "nginx:1.27.3".to_string(),
        estado,
        codigo_salida: codigo,
        salud,
        puertos: String::new(),
        proyecto: None,
        ficheros: Vec::new(),
        servicio: None,
        orden: None,
        etiquetas: std::collections::BTreeMap::new(),
    }
}

#[test]
fn acciones_permitidas_por_estado_de_la_unidad() {
    let recargable = unidad_de_prueba(
        "nginx.service",
        "active",
        &[("CanReload", "yes"), ("UnitFileState", "enabled")],
    );
    assert_eq!(
        acciones::acciones_de_unidad(&recargable),
        vec![
            Accion::Parar,
            Accion::Reiniciar,
            Accion::Recargar,
            Accion::Desactivar
        ]
    );
    let sin_recarga = unidad_de_prueba(
        "app.service",
        "active",
        &[("CanReload", "no"), ("UnitFileState", "enabled")],
    );
    assert!(!acciones::acciones_de_unidad(&sin_recarga).contains(&Accion::Recargar));
    let parada = unidad_de_prueba(
        "tareas.service",
        "inactive",
        &[("UnitFileState", "disabled")],
    );
    assert_eq!(
        acciones::acciones_de_unidad(&parada),
        vec![Accion::Arrancar, Accion::Reiniciar, Accion::Activar]
    );
    let fallida = unidad_de_prueba("copia.service", "failed", &[("UnitFileState", "static")]);
    assert_eq!(
        acciones::acciones_de_unidad(&fallida),
        vec![Accion::Arrancar, Accion::Reiniciar]
    );
    let enmascarada = unidad_de_prueba(
        "prohibida.service",
        "inactive",
        &[("UnitFileState", "masked"), ("LoadState", "masked")],
    );
    assert!(acciones::acciones_de_unidad(&enmascarada).is_empty());
    let cambiando = unidad_de_prueba("arrancando.service", "activating", &[]);
    assert!(acciones::acciones_de_unidad(&cambiando).contains(&Accion::Parar));
    assert!(!acciones::acciones_de_unidad(&cambiando).contains(&Accion::Reiniciar));
    let no_encontrada = unidad_de_prueba(
        "inventada.service",
        "inactive",
        &[("LoadState", "not-found")],
    );
    assert!(acciones::acciones_de_unidad(&no_encontrada).is_empty());
}

#[test]
fn acciones_de_timer_contenedor_y_proyecto() {
    let timer = magi_nucleo::servicios::Timer {
        nombre: "copia.timer".to_string(),
        props: props(&[("LoadState", "loaded"), ("ActiveState", "active")]),
        servicio: Some("copia.service".to_string()),
    };
    assert_eq!(
        acciones::acciones_de_timer(&timer, Some("inactive")),
        vec![Accion::Lanzar]
    );
    assert!(acciones::acciones_de_timer(&timer, Some("active")).is_empty());
    assert!(!acciones::acciones_de_timer(&timer, None).is_empty());

    let en_marcha =
        contenedor_de_prueba("web-app-1", EstadoContenedor::EnMarcha, Salud::Sana, None);
    assert_eq!(
        acciones::acciones_de_contenedor(&en_marcha),
        vec![Accion::Parar, Accion::Reiniciar]
    );
    let parado = contenedor_de_prueba(
        "web-app-1",
        EstadoContenedor::Parado,
        Salud::Ninguna,
        Some(0),
    );
    assert_eq!(
        acciones::acciones_de_contenedor(&parado),
        vec![Accion::Arrancar, Accion::Reiniciar]
    );
    let creado = contenedor_de_prueba("prueba", EstadoContenedor::Creado, Salud::Ninguna, None);
    assert_eq!(
        acciones::acciones_de_contenedor(&creado),
        vec![Accion::Arrancar]
    );
    let pausado = contenedor_de_prueba("pausa", EstadoContenedor::Pausado, Salud::Ninguna, None);
    assert!(acciones::acciones_de_contenedor(&pausado).is_empty());
    let muerto = contenedor_de_prueba("muerto", EstadoContenedor::Muerto, Salud::Ninguna, None);
    assert!(acciones::acciones_de_contenedor(&muerto).is_empty());

    let proyecto = vec![
        contenedor_de_prueba("a", EstadoContenedor::EnMarcha, Salud::Ninguna, None),
        contenedor_de_prueba("b", EstadoContenedor::Parado, Salud::Ninguna, Some(0)),
    ];
    assert_eq!(
        acciones::acciones_de_proyecto(&proyecto),
        vec![Accion::Arrancar, Accion::Parar, Accion::Reiniciar]
    );
}

#[test]
fn deliberacion_y_comprobacion_por_accion() {
    for (tipo, accion, delibera) in [
        (TipoObjetivo::Unidad, Accion::Arrancar, false),
        (TipoObjetivo::Unidad, Accion::Parar, true),
        (TipoObjetivo::Unidad, Accion::Reiniciar, true),
        (TipoObjetivo::Unidad, Accion::Recargar, false),
        (TipoObjetivo::Unidad, Accion::Activar, false),
        (TipoObjetivo::Unidad, Accion::Desactivar, true),
        (TipoObjetivo::Timer, Accion::Lanzar, false),
        (TipoObjetivo::Contenedor, Accion::Arrancar, false),
        (TipoObjetivo::Contenedor, Accion::Parar, true),
        (TipoObjetivo::Contenedor, Accion::Reiniciar, true),
        (TipoObjetivo::Proyecto, Accion::Arrancar, false),
        (TipoObjetivo::Proyecto, Accion::Parar, true),
        (TipoObjetivo::Proyecto, Accion::Reiniciar, true),
    ] {
        let objetivo = Objetivo {
            tipo,
            nombre: "x.service".to_string(),
        };
        assert_eq!(
            acciones::exige_deliberacion(&objetivo, accion),
            delibera,
            "tipo {tipo:?} accion {accion:?}"
        );
    }
    for (tipo, accion, comprueba) in [
        (TipoObjetivo::Unidad, Accion::Reiniciar, true),
        (TipoObjetivo::Unidad, Accion::Recargar, true),
        (TipoObjetivo::Unidad, Accion::Arrancar, false),
        (TipoObjetivo::Unidad, Accion::Parar, false),
        (TipoObjetivo::Timer, Accion::Lanzar, false),
        (TipoObjetivo::Contenedor, Accion::Reiniciar, false),
        (TipoObjetivo::Proyecto, Accion::Reiniciar, true),
        (TipoObjetivo::Proyecto, Accion::Parar, false),
    ] {
        let objetivo = Objetivo {
            tipo,
            nombre: "x".to_string(),
        };
        assert_eq!(
            acciones::comprueba_configuracion(&objetivo, accion),
            comprueba,
            "tipo {tipo:?} accion {accion:?}"
        );
    }
    assert!(acciones::es_privilegiada(Accion::Reiniciar));
}

#[test]
fn exito_segun_el_estado_final() {
    let objetivo_unidad = Objetivo::unidad("app.service");
    let activa = unidad_de_prueba("app.service", "active", &[]);
    assert!(acciones::exito(
        &objetivo_unidad,
        Accion::Reiniciar,
        Some(&activa),
        None,
        &[]
    ));
    let parada = unidad_de_prueba("app.service", "inactive", &[]);
    assert!(acciones::exito(
        &objetivo_unidad,
        Accion::Parar,
        Some(&parada),
        None,
        &[]
    ));
    assert!(!acciones::exito(
        &objetivo_unidad,
        Accion::Reiniciar,
        Some(&parada),
        None,
        &[]
    ));
    let oneshot = unidad_de_prueba(
        "copia.service",
        "inactive",
        &[("Type", "oneshot"), ("Result", "success")],
    );
    assert!(acciones::exito(
        &objetivo_unidad,
        Accion::Arrancar,
        Some(&oneshot),
        None,
        &[]
    ));
    let oneshot_fallido = unidad_de_prueba(
        "copia.service",
        "inactive",
        &[("Type", "oneshot"), ("Result", "exit-code")],
    );
    assert!(!acciones::exito(
        &objetivo_unidad,
        Accion::Arrancar,
        Some(&oneshot_fallido),
        None,
        &[]
    ));
    let activada = unidad_de_prueba("app.service", "active", &[("UnitFileState", "enabled")]);
    assert!(acciones::exito(
        &objetivo_unidad,
        Accion::Activar,
        Some(&activada),
        None,
        &[]
    ));
    let objetivo_timer = Objetivo {
        tipo: TipoObjetivo::Timer,
        nombre: "copia.timer".to_string(),
    };
    assert!(acciones::exito(
        &objetivo_timer,
        Accion::Lanzar,
        Some(&activa),
        None,
        &[]
    ));

    let objetivo_contenedor = Objetivo::contenedor("web-app-1");
    let en_marcha =
        contenedor_de_prueba("web-app-1", EstadoContenedor::EnMarcha, Salud::Sana, None);
    assert!(acciones::exito(
        &objetivo_contenedor,
        Accion::Reiniciar,
        None,
        Some(&en_marcha),
        &[]
    ));
    let enfermo = contenedor_de_prueba(
        "web-app-1",
        EstadoContenedor::EnMarcha,
        Salud::Insegura,
        None,
    );
    assert!(!acciones::exito(
        &objetivo_contenedor,
        Accion::Arrancar,
        None,
        Some(&enfermo),
        &[]
    ));
    let parado = contenedor_de_prueba(
        "web-app-1",
        EstadoContenedor::Parado,
        Salud::Ninguna,
        Some(0),
    );
    assert!(acciones::exito(
        &objetivo_contenedor,
        Accion::Parar,
        None,
        Some(&parado),
        &[]
    ));

    let objetivo_proyecto = Objetivo::proyecto("web");
    let proyecto_en_marcha = vec![
        contenedor_de_prueba("a", EstadoContenedor::EnMarcha, Salud::Ninguna, None),
        contenedor_de_prueba("b", EstadoContenedor::EnMarcha, Salud::Ninguna, None),
    ];
    assert!(acciones::exito(
        &objetivo_proyecto,
        Accion::Reiniciar,
        None,
        None,
        &proyecto_en_marcha
    ));
    assert!(!acciones::exito(
        &objetivo_proyecto,
        Accion::Parar,
        None,
        None,
        &proyecto_en_marcha
    ));
    let proyecto_parado = vec![contenedor_de_prueba(
        "a",
        EstadoContenedor::Parado,
        Salud::Ninguna,
        Some(0),
    )];
    assert!(acciones::exito(
        &objetivo_proyecto,
        Accion::Parar,
        None,
        None,
        &proyecto_parado
    ));
}

#[test]
fn motivos_y_textos_de_estado() {
    let objetivo = Objetivo::unidad("nginx.service");
    let permitidas = vec![Accion::Arrancar];
    assert_eq!(
        acciones::motivo_no_permitida(&objetivo, Accion::Arrancar, &permitidas),
        None
    );
    assert_eq!(
        acciones::motivo_no_permitida(&objetivo, Accion::Recargar, &permitidas).as_deref(),
        Some("nginx.service no admite recargar")
    );
    assert_eq!(
        acciones::motivo_no_permitida(&objetivo, Accion::Parar, &permitidas).as_deref(),
        Some("nginx.service ya está parado")
    );
    let activa = unidad_de_prueba("nginx.service", "active", &[]);
    assert_eq!(acciones::texto_estado_unidad(&activa), "activo");
    let fallida = unidad_de_prueba("app.service", "failed", &[]);
    assert_eq!(acciones::texto_estado_unidad(&fallida), "fallido");
    let enmascarada = unidad_de_prueba(
        "app.service",
        "inactive",
        &[("LoadState", "masked"), ("UnitFileState", "masked")],
    );
    assert_eq!(acciones::texto_estado_unidad(&enmascarada), "enmascarada");

    let marcha = contenedor_de_prueba("a", EstadoContenedor::EnMarcha, Salud::Sana, None);
    assert_eq!(acciones::texto_estado_contenedor(&marcha), "en marcha");
    let caido = contenedor_de_prueba("a", EstadoContenedor::Parado, Salud::Ninguna, Some(1));
    assert_eq!(acciones::texto_estado_contenedor(&caido), "salió (1)");
    let sano_parado = contenedor_de_prueba("a", EstadoContenedor::Parado, Salud::Ninguna, Some(0));
    assert_eq!(
        acciones::texto_estado_contenedor(&sano_parado),
        "parado (0)"
    );
    let enfermo = contenedor_de_prueba("a", EstadoContenedor::EnMarcha, Salud::Insegura, None);
    assert_eq!(acciones::texto_estado_contenedor(&enfermo), "unhealthy");
    assert!(acciones::estado_unidad_final("active"));
    assert!(!acciones::estado_unidad_final("activating"));
    assert!(acciones::timer_servicio_activo("reloading"));
    assert!(!acciones::timer_servicio_activo("failed"));
    assert!(acciones::arranque_conmutable(EstadoArranque::Activado));
    assert!(!acciones::arranque_conmutable(EstadoArranque::Estatico));
    assert!(acciones::contenedor_caido(&caido));
    assert!(acciones::unidad_utilizable(&activa));
    assert!(!acciones::unidad_utilizable(&enmascarada));
}

// ------------------------------------------------------------------ validadores
#[test]
fn validadores_conocidos_y_propios() {
    assert_eq!(
        validadores::conocida(&Objetivo::unidad("nginx.service")),
        Some(validadores::Conocida::Nginx)
    );
    assert_eq!(
        validadores::conocida(&Objetivo::unidad("caddy.service")),
        Some(validadores::Conocida::Caddy)
    );
    assert_eq!(
        validadores::conocida(&Objetivo::unidad("apache2.service")),
        Some(validadores::Conocida::Apache)
    );
    assert_eq!(
        validadores::conocida(&Objetivo::unidad("httpd.service")),
        Some(validadores::Conocida::Apache)
    );
    assert_eq!(
        validadores::conocida(&Objetivo::unidad("haproxy.service")),
        Some(validadores::Conocida::Haproxy)
    );
    assert_eq!(
        validadores::conocida(&Objetivo::proyecto("web")),
        Some(validadores::Conocida::Compose)
    );
    assert_eq!(
        validadores::conocida(&Objetivo::unidad("app.service")),
        None
    );
    assert_eq!(
        validadores::conocida(&Objetivo::contenedor("web-app-1")),
        None
    );

    let propio =
        validadores::para_objetivo(&Objetivo::unidad("nginx.service"), Some("/srv/comprobar"));
    assert_eq!(
        propio,
        validadores::Comprobacion::Propia("/srv/comprobar".to_string())
    );
    assert_eq!(
        validadores::para_objetivo(&Objetivo::unidad("nginx.service"), None),
        validadores::Comprobacion::Conocida(validadores::Conocida::Nginx)
    );
    assert_eq!(
        validadores::para_objetivo(&Objetivo::unidad("app.service"), Some("   ")),
        validadores::Comprobacion::Ninguna
    );
    assert_eq!(
        validadores::para_objetivo(&Objetivo::contenedor("web-app-1"), Some("echo hola")),
        validadores::Comprobacion::Ninguna
    );
    assert_eq!(
        validadores::descripcion(&validadores::Comprobacion::Conocida(
            validadores::Conocida::Nginx
        ))
        .as_deref(),
        Some("nginx -t")
    );
    assert_eq!(
        validadores::descripcion(&validadores::Comprobacion::Propia("x".to_string())).as_deref(),
        Some("la comprobación propia")
    );
    assert_eq!(
        validadores::descripcion(&validadores::Comprobacion::Ninguna),
        None
    );
    assert!(validadores::aplica(TipoObjetivo::Unidad));
    assert!(validadores::aplica(TipoObjetivo::Proyecto));
    assert!(!validadores::aplica(TipoObjetivo::Contenedor));
    assert!(validadores::requiere_privilegios());
    assert_eq!(validadores::Conocida::Compose.texto(), "config -q");
    assert_eq!(validadores::Conocida::Haproxy.nombre(), "HAProxy");
}

#[test]
fn los_fallos_del_sondeo_guardan_veinte_nombres_y_el_total() {
    let unidades: Vec<String> = (0..25)
        .map(|indice| format!("fallo{indice}.service"))
        .collect();
    let contenedores = Some((
        MotorNombre::Docker,
        vec!["web-worker-1".to_string(), "panel".to_string()],
    ));
    let fallos = lectura::fallos(&unidades, contenedores);
    assert_eq!(fallos.unidades.total, 25);
    assert_eq!(fallos.unidades.nombres.len(), 20);
    let contenedores = fallos.contenedores.as_ref().expect("contenedores");
    assert_eq!(contenedores.total, 2);
    assert_eq!(contenedores.motor, Some(MotorNombre::Docker));
    assert_eq!(fallos.total(), 27);
    assert!(!fallos.vacio());
    assert_eq!(fallos.lista().len(), 22);
}
