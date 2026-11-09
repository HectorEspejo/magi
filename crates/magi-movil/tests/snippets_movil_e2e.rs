//! Prueba de extremo a extremo de los snippets del móvil, sin red externa: el
//! servidor SSH de pruebas compartido ejecuta el comando resuelto en dos hosts
//! y se comprueba todo el flujo del informe §4.5 y §7.6.
//!
//! Cubre: lista y detalle de snippets, variables sustituidas con escape,
//! deliberación obligatoria por varios hosts, aprobación en DELIBERACIONES,
//! ejecución con semáforo, salida por host, REGISTRO `snippet_ejecutado`,
//! `usado_veces` y `parar al primer fallo` (los hosts en cola quedan omitidos).

mod comun;

use std::time::Duration;

use magi_movil::api::{
    DecisionDeliberacion, EstadoDeliberacion, EstadoHostEjecucionMovil, PlanDeliberadoMovil,
    ResultadoEjecucionMovil, VariableSnippetMovil,
};
use magi_nucleo::modelo::DatosHost;
use magi_nucleo::snippets::{DatosSnippet, Destino, TIMEOUT_DEFECTO};

use comun::servidor_pruebas::{arrancar, PuertosPrueba, ServidorPruebas};
use comun::{
    consulta, contar_registro, crear_host, crear_snippet, esperar_ejecuciones, PruebaNucleo,
};

/// UN host de pruebas con la etiqueta `web` (destino del snippet).
fn datos_host(nombre: &str, puerto: u16) -> DatosHost {
    DatosHost {
        nombre: nombre.to_string(),
        direccion: "127.0.0.1".to_string(),
        puerto,
        usuario: Some("usuario".to_string()),
        etiquetas: vec!["web".to_string()],
        ..DatosHost::default()
    }
}

/// El snippet de la prueba: una variable fuera de comillas para comprobar el
/// escape, y `$$` para que cada host devuelva una salida distinta (su PID).
fn datos_snippet() -> DatosSnippet {
    DatosSnippet {
        nombre: "saludar".to_string(),
        comando: "printf '%s %s\\n' {{saludo}} $$".to_string(),
        descripcion: "Saluda desde el host".to_string(),
        etiquetas: vec!["pruebas".to_string()],
        critico: false,
        timeout_seg: TIMEOUT_DEFECTO,
        parar_al_fallo: false,
        destinos: vec![Destino::Etiqueta("web".to_string())],
    }
}

struct Montaje {
    prueba: PruebaNucleo,
    _servidor: ServidorPruebas,
    uuid_snippet: String,
    uuid_web_01: String,
    uuid_web_02: String,
}

/// Monta dos hosts `web` contra el servidor de pruebas y el snippet que apunta
/// a su etiqueta.
async fn montar() -> Montaje {
    let servidor = arrancar(PuertosPrueba::efimeros()).await;
    let mut prueba = PruebaNucleo::nueva(&[servidor.linea_known_hosts()]);
    let (_id_01, uuid_web_01) = crear_host(&prueba, datos_host("web-01", servidor.puertos.ssh));
    let (_id_02, uuid_web_02) = crear_host(&prueba, datos_host("web-02", servidor.puertos.ssh));
    let uuid_snippet = crear_snippet(&prueba, datos_snippet());
    prueba.arrancar();
    Montaje {
        prueba,
        _servidor: servidor,
        uuid_snippet,
        uuid_web_01,
        uuid_web_02,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn un_snippet_se_ejecuta_en_dos_hosts_y_deja_salida_y_registro() {
    let montaje = montar().await;
    let nucleo = montaje.prueba.nucleo();

    // Lista y detalle: destinos resueltos y la variable del comando.
    let lista = nucleo.snippets(None).expect("lista de snippets");
    assert_eq!(lista.len(), 1);
    assert_eq!(lista[0].uuid, montaje.uuid_snippet);
    assert_eq!(lista[0].hosts.len(), 2, "{:?}", lista[0]);
    assert_eq!(lista[0].variables.len(), 1);
    assert_eq!(lista[0].variables[0].nombre, "saludo");
    let detalle = nucleo
        .snippet(montaje.uuid_snippet.clone())
        .expect("detalle del snippet");
    assert!(detalle.comando.contains("{{saludo}}"), "{detalle:?}");

    // Sin valor para la variable (no tiene por defecto): error en su campo.
    let sin_valor = nucleo
        .preparar_ejecucion(
            montaje.uuid_snippet.clone(),
            vec![montaje.uuid_web_01.clone(), montaje.uuid_web_02.clone()],
            Vec::new(),
        )
        .expect("preparando sin valor");
    assert!(!sin_valor.errores.is_empty(), "{sin_valor:?}");
    assert_eq!(sin_valor.errores[0].campo, "saludo");

    // Con valor: el núcleo escapa la sustitución y delibera por ser dos hosts.
    let preparacion = nucleo
        .preparar_ejecucion(
            montaje.uuid_snippet.clone(),
            vec![montaje.uuid_web_01.clone(), montaje.uuid_web_02.clone()],
            vec![VariableSnippetMovil {
                nombre: "saludo".to_string(),
                valor_defecto: None,
                valor: Some("uno dos".to_string()),
            }],
        )
        .expect("preparando la ejecución");
    assert!(preparacion.errores.is_empty(), "{preparacion:?}");
    assert!(
        preparacion.comando.contains("'uno dos'"),
        "la variable va escapada: {}",
        preparacion.comando
    );
    assert_eq!(preparacion.hosts.len(), 2);
    assert!(preparacion.delibera, "dos hosts exige deliberar");
    assert!(!preparacion.motivos.is_empty());

    // Deliberación: sin verificaciones activas sale APROBADO y se guarda.
    let tabla = nucleo
        .deliberar(PlanDeliberadoMovil::Snippet {
            preparacion: preparacion.clone(),
        })
        .await
        .expect("deliberando");
    assert_eq!(tabla.filas.len(), 2);
    assert_eq!(tabla.estado, EstadoDeliberacion::Aprobado, "{tabla:?}");
    assert!(tabla.unanime);
    let deliberacion = nucleo
        .resolver_deliberacion(tabla.id, DecisionDeliberacion::Aprobar)
        .expect("resolviendo la deliberación");
    assert!(deliberacion > 0);

    // Ejecución con la deliberación aprobada.
    let ejecucion = nucleo
        .ejecutar_snippet(preparacion, false, Some(deliberacion))
        .await
        .expect("ejecutando el snippet");
    let lista = esperar_ejecuciones(montaje.prueba.oyente(), Duration::from_secs(20), |lista| {
        lista
            .iter()
            .any(|fila| fila.id == ejecucion && fila.resultado == Some(ResultadoEjecucionMovil::Ok))
    })
    .await;
    let fila = lista
        .into_iter()
        .find(|fila| fila.id == ejecucion)
        .expect("ejecución en la lista");
    assert_eq!(fila.hosts.len(), 2);
    for host in &fila.hosts {
        assert_eq!(host.estado, EstadoHostEjecucionMovil::Ok, "{host:?}");
        assert_eq!(host.codigo, Some(0), "{host:?}");
        assert!(host.bytes_stdout > 0, "{host:?}");
        assert!(host.duracion_ms >= 0);
    }
    assert!(fila.terminada_en.is_some());
    assert_eq!(fila.deliberacion_id, Some(deliberacion));

    // La salida de cada host, sin secuencias de escape y distinta por host.
    let salida_01 = nucleo
        .salida_ejecucion(ejecucion, montaje.uuid_web_01.clone())
        .expect("salida de web-01");
    let salida_02 = nucleo
        .salida_ejecucion(ejecucion, montaje.uuid_web_02.clone())
        .expect("salida de web-02");
    assert!(salida_01.stdout.starts_with("uno dos "), "{salida_01:?}");
    assert!(salida_02.stdout.starts_with("uno dos "), "{salida_02:?}");
    assert_ne!(
        salida_01.stdout, salida_02.stdout,
        "cada host devuelve su propio PID"
    );
    assert!(!salida_01.stdout_truncada);
    assert_eq!(salida_01.stderr, "");

    // REGISTRO: una anotación por host y la deliberación aprobada.
    assert_eq!(contar_registro(nucleo, "snippet_ejecutado"), 2);
    assert_eq!(contar_registro(nucleo, "deliberacion_aprobada"), 1);
    assert_eq!(contar_registro(nucleo, "deliberacion_forzada"), 0);

    // La fila de DELIBERACIONES tiene el resultado de la ejecución.
    let (resultado, ejecucion_resultado) = consulta(nucleo, |almacen| {
        almacen
            .conexion()
            .query_row(
                "SELECT resultado, ejecucion_resultado FROM DELIBERACIONES
                  ORDER BY id DESC LIMIT 1",
                [],
                |fila| Ok((fila.get::<_, String>(0)?, fila.get::<_, Option<String>>(1)?)),
            )
            .expect("fila de DELIBERACIONES")
    })
    .expect("hilo escritor");
    assert_eq!(resultado, "aprobada");
    assert_eq!(ejecucion_resultado.as_deref(), Some("ok"));

    // Uso del snippet: campo propio del móvil.
    let detalle = nucleo
        .snippet(montaje.uuid_snippet.clone())
        .expect("detalle tras ejecutar");
    assert_eq!(detalle.usado_veces, 1);
    assert!(detalle.ultimo_uso_en.is_some());
    assert!(nucleo
        .ejecuciones()
        .expect("lista")
        .iter()
        .any(|fila| fila.id == ejecucion));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parar_al_primer_fallo_omite_los_que_esperan() {
    let servidor = arrancar(PuertosPrueba::efimeros()).await;
    let mut prueba = PruebaNucleo::nueva(&[servidor.linea_known_hosts()]);
    // Ocho hosts con el puerto cerrado (fallan nada más conectar) y uno sano
    // en último lugar: el semáforo de 8 los reparte y el noveno queda en cola.
    let mut marcados = Vec::new();
    for indice in 1..=8 {
        let (_, uuid) = crear_host(&prueba, datos_host(&format!("web-0{indice}"), 1));
        marcados.push(uuid);
    }
    let (_, uuid_sano) = crear_host(&prueba, datos_host("web-09", servidor.puertos.ssh));
    marcados.push(uuid_sano.clone());
    let uuid_snippet = crear_snippet(
        &prueba,
        DatosSnippet {
            nombre: "eco".to_string(),
            comando: "echo ok".to_string(),
            destinos: vec![Destino::Etiqueta("web".to_string())],
            ..DatosSnippet::default()
        },
    );
    prueba.arrancar();
    let nucleo = prueba.nucleo();

    let preparacion = nucleo
        .preparar_ejecucion(uuid_snippet, marcados.clone(), Vec::new())
        .expect("preparando");
    assert!(preparacion.errores.is_empty(), "{preparacion:?}");
    assert!(preparacion.delibera, "nueve hosts exige deliberar");
    let tabla = nucleo
        .deliberar(PlanDeliberadoMovil::Snippet {
            preparacion: preparacion.clone(),
        })
        .await
        .expect("deliberando");
    let deliberacion = nucleo
        .resolver_deliberacion(tabla.id, DecisionDeliberacion::Aprobar)
        .expect("aprobando");
    let ejecucion = nucleo
        .ejecutar_snippet(preparacion, true, Some(deliberacion))
        .await
        .expect("ejecutando con parar al fallo");

    let lista = esperar_ejecuciones(prueba.oyente(), Duration::from_secs(20), |lista| {
        lista
            .iter()
            .any(|fila| fila.id == ejecucion && fila.resultado.is_some())
    })
    .await;
    let fila = lista
        .into_iter()
        .find(|fila| fila.id == ejecucion)
        .expect("ejecución terminada");
    assert!(fila.parar_al_fallo, "{fila:?}");
    assert_ne!(
        fila.resultado,
        Some(ResultadoEjecucionMovil::Ok),
        "{fila:?}"
    );
    // Los ocho hosts sin escucha acaban en error y el noveno se omite al no
    // llegar a empezar (estaba en cola cuando saltó el primer fallo).
    let errores = fila
        .hosts
        .iter()
        .filter(|host| host.estado == EstadoHostEjecucionMovil::Error)
        .count();
    let omitidos = fila
        .hosts
        .iter()
        .filter(|host| host.estado == EstadoHostEjecucionMovil::Omitido)
        .count();
    assert_eq!(errores, 8, "{fila:?}");
    assert_eq!(omitidos, 1, "{fila:?}");
    assert_eq!(
        fila.hosts
            .iter()
            .find(|host| host.host_uuid == uuid_sano)
            .map(|host| host.estado),
        Some(EstadoHostEjecucionMovil::Omitido),
        "{fila:?}"
    );
    // Solo se anotan los ocho intentos fallidos, no el omitido.
    assert_eq!(contar_registro(nucleo, "snippet_ejecutado"), 8);
}
