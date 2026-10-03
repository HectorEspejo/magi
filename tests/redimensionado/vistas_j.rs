//! SINCRONIZAR y vista previa del plan (Fase 8).
//! Instantáneas a 40×12, 80×24 y 200×60 (T45).
//!
//! El flujo se prueba como lo haría el usuario: `S` en Archivos con los
//! paneles de `vistas_d` (hogar local y `/var/www/cooperapp`), el árbol
//! remoto lo contesta la prueba con `Arbol` y el local es el hogar de verdad,
//! recorrido en su hilo (la prueba espera su evento con `bombear`).

use std::fs;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyModifiers};

use magi::app::sincronizar::{
    DialogoSincronizar, EstadoMagiignore, FocoSincronizar, FormularioSincronizar,
    PlanSincronizacion, SolicitudSincronizacion, VistaPrevia,
};
use magi::app::Dialogo;
use magi::archivos::plan::{Cambio, MotivoOmision, Plan};
use magi::archivos::TipoEntrada;
use magi::protocolo::{
    Direccion, ElementoTransferencia, EntradaArbol, EstadoTransferencia, EtiquetaTransferencia,
    InfoTransferencia, MensajeCliente, MensajeServidor, Politica,
};
use magi::snippets::MotivoDeliberacion;
use magi::tema::Tema;
use magi::ui::componentes::CampoTexto;
use magi::ui::disposicion::Lista;
use magi::ui::Vista;

use super::vistas_g::FabricaDialogo;
use crate::arnes::{self, AppPrueba};
use crate::semilla::Sembrado;

/// Los tres tamaños de referencia.
const TAMANOS: [(u16, u16); 3] = [(40, 12), (80, 24), (200, 60)];
const RUTA_REMOTA: &str = "/var/www/cooperapp";
const DIA: i64 = 86_400;

/// Diálogos de esta vista para las pruebas comunes de `vistas_g` (enteros a
/// 40×12 y sin glifos Unicode en ASCII).
pub(crate) fn dialogos() -> Vec<(&'static str, FabricaDialogo)> {
    vec![
        ("SINCRONIZAR", Box::new(formulario)),
        ("SINCRONIZAR", Box::new(vista_previa)),
    ]
}

// ---------------------------------------------------------------- fábricas

/// El diálogo de la maqueta §6.2: subida, `*.log` en las extras y «guardar
/// como web-prod».
fn formulario() -> Dialogo {
    let mut formulario = FormularioSincronizar::nuevo(
        1,
        1,
        "hetzner-01".to_string(),
        Direccion::Subida,
        "/home/hector/proyectos/cooperapp".to_string(),
        RUTA_REMOTA.to_string(),
        "~/proyectos/cooperapp".to_string(),
        format!("hetzner-01:{RUTA_REMOTA}"),
        EstadoMagiignore::Patrones(4),
    );
    formulario.extras = CampoTexto::nuevo("*.log");
    formulario.guardar = true;
    formulario.nombre = CampoTexto::nuevo("web-prod");
    formulario.foco = FocoSincronizar::Extras;
    Dialogo::Sincronizar(DialogoSincronizar::Formulario(Box::new(formulario)))
}

/// El plan de la maqueta §6.3, con los cuatro signos: dos ficheros nuevos,
/// dos que cambian, dos que sobran (con «borrar») y un enlace a directorio.
/// Las fechas van relativas a ahora para que «hace 2 días» no cambie.
fn plan_maqueta() -> PlanSincronizacion {
    let ahora = magi::modelo::fecha_ahora_epoca();
    PlanSincronizacion {
        solicitud: SolicitudSincronizacion {
            host_id: 1,
            host_nombre: "hetzner-01".to_string(),
            guardada: Some((1, "web-prod".to_string())),
            direccion: Direccion::Subida,
            ruta_local: "/home/hector/proyectos/cooperapp".to_string(),
            ruta_remota: RUTA_REMOTA.to_string(),
            borrar: true,
            extras: vec!["*.log".to_string()],
        },
        plan: Plan {
            cambios: vec![
                Cambio::Actualizar {
                    ruta: "config.yaml".to_string(),
                    tamano: 2_048,
                    mtime: ahora - 3 * DIA,
                    permisos: Some(0o640),
                    tamano_destino: 1_024,
                    mtime_destino: ahora - 3 * DIA,
                },
                Cambio::Actualizar {
                    ruta: "main.py".to_string(),
                    tamano: 14_336,
                    mtime: ahora - 60,
                    permisos: Some(0o644),
                    tamano_destino: 12_288,
                    mtime_destino: ahora - 2 * DIA - 100,
                },
                Cambio::Omitir {
                    ruta: "static/compartido".to_string(),
                    motivo: MotivoOmision::EnlaceADirectorio,
                },
                Cambio::Crear {
                    ruta: "static/img/logo.svg".to_string(),
                    tamano: 14_336,
                    permisos: Some(0o644),
                },
                Cambio::Crear {
                    ruta: "static/js/app.js".to_string(),
                    tamano: 90_112,
                    permisos: Some(0o644),
                },
                Cambio::Borrar {
                    ruta: "static/old.css".to_string(),
                    es_dir: false,
                    tamano: 3_072,
                },
                Cambio::Borrar {
                    ruta: "tmp".to_string(),
                    es_dir: true,
                    tamano: 0,
                },
            ],
        },
        excluidos: 37,
    }
}

fn vista_previa() -> Dialogo {
    Dialogo::Sincronizar(DialogoSincronizar::VistaPrevia(Box::new(
        VistaPrevia::nueva(
            plan_maqueta(),
            "~/proyectos/cooperapp".to_string(),
            format!("hetzner-01:{RUTA_REMOTA}"),
            vec![MotivoDeliberacion::Borrar(2)],
        ),
    )))
}

// ---------------------------------------------------------------- utilidades

/// Archivos abierta (como en `vistas_d`) con el panel local en el hogar.
fn con_archivos(cols: u16, filas: u16, tema: Tema) -> (AppPrueba, Sembrado) {
    let (mut prueba, sembrado) = AppPrueba::con_semilla_y_tema(cols, filas, tema);
    super::vistas_d::abrir_archivos(&mut prueba, &sembrado);
    prueba.enviados();
    (prueba, sembrado)
}

/// Bombea eventos (los del hilo del árbol local) hasta que se cumple `hecho`.
fn esperar(prueba: &mut AppPrueba, hecho: impl Fn(&AppPrueba) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !hecho(prueba) {
        assert!(
            Instant::now() < limite,
            "no llegó a tiempo:\n{}",
            prueba.texto()
        );
        prueba
            .app
            .bombear(&mut prueba.terminal, Duration::from_millis(20))
            .unwrap();
    }
}

fn en_vista_previa(prueba: &AppPrueba) -> bool {
    matches!(
        prueba.app.dialogo,
        Some(Dialogo::Sincronizar(DialogoSincronizar::VistaPrevia(_)))
    )
}

fn texto_del_mensaje(prueba: &AppPrueba) -> String {
    prueba
        .app
        .mensaje
        .as_ref()
        .map(|mensaje| mensaje.texto.clone())
        .unwrap_or_default()
}

fn escribir(prueba: &mut AppPrueba, texto: &str) {
    for caracter in texto.chars() {
        prueba.tecla(KeyCode::Char(caracter));
    }
}

/// El `ListarArbol` enviado: `peticion_id`, exclusiones, `usar_magiignore` y
/// extras.
fn listar_arbol(prueba: &mut AppPrueba) -> (u64, Vec<String>, bool, Vec<String>) {
    prueba
        .enviados()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::ListarArbol {
                peticion_id,
                ruta,
                exclusiones,
                usar_magiignore,
                exclusiones_extra,
                ..
            } => {
                assert_eq!(ruta, RUTA_REMOTA);
                Some((peticion_id, exclusiones, usar_magiignore, exclusiones_extra))
            }
            _ => None,
        })
        .expect("la App pide el árbol remoto")
}

/// Una `Transferir` enviada, con todo lo que lleva.
struct Enviada {
    direccion: Direccion,
    elementos: Vec<ElementoTransferencia>,
    politica: Politica,
    borrar_origen: bool,
    peticion_id: Option<u64>,
    borrar_al_terminar: Vec<String>,
    deliberacion: Option<magi::protocolo::DeliberacionLanzada>,
    sincronizacion: Option<magi::protocolo::SincronizacionLanzada>,
    etiqueta: Option<EtiquetaTransferencia>,
}

fn transferir(prueba: &mut AppPrueba) -> Option<Enviada> {
    prueba
        .enviados()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::Transferir {
                direccion,
                elementos,
                politica,
                borrar_origen,
                peticion_id,
                borrar_al_terminar,
                deliberacion,
                sincronizacion,
                etiqueta,
                ..
            } => Some(Enviada {
                direccion,
                elementos,
                politica,
                borrar_origen,
                peticion_id,
                borrar_al_terminar,
                deliberacion,
                sincronizacion,
                etiqueta,
            }),
            _ => None,
        })
}

fn fichero(ruta: &str, tamano: u64, mtime: i64) -> EntradaArbol {
    EntradaArbol {
        ruta: ruta.to_string(),
        tipo: TipoEntrada::Fichero,
        tamano,
        mtime,
        permisos: Some(0o100600),
        propietario: None,
        enlace_a_dir: false,
    }
}

fn directorio(ruta: &str) -> EntradaArbol {
    EntradaArbol {
        ruta: ruta.to_string(),
        tipo: TipoEntrada::Directorio,
        tamano: 4_096,
        mtime: 0,
        permisos: Some(0o040755),
        propietario: None,
        enlace_a_dir: false,
    }
}

/// mtime real de un fichero del hogar (lo fija `vistas_d`).
fn mtime_local(prueba: &AppPrueba, nombre: &str) -> i64 {
    let ruta = prueba.app.rutas.hogar.join(nombre);
    magi::archivos::local::mtime_de(&fs::metadata(ruta).unwrap())
}

/// El árbol remoto de las pruebas de subida: sin `app/`, `main.py` igual que
/// el local, `config.yaml` con otro tamaño y permisos propios, `viejo.css`
/// que sobra y `.env`, que sobra pero está excluido por las extras (el
/// servidor ya no lo mandaría; si lo hace, el cliente lo poda igual).
fn arbol_remoto(prueba: &AppPrueba) -> Vec<EntradaArbol> {
    let mut config = fichero("config.yaml", 1_024, mtime_local(prueba, "config.yaml"));
    config.permisos = Some(0o100640);
    vec![
        directorio("static"),
        fichero("main.py", 14_336, mtime_local(prueba, "main.py")),
        config,
        fichero("viejo.css", 3_072, 1_700_000_000),
        fichero(".env", 70, 1_700_000_000),
    ]
}

/// `S` sobre el panel local, extras `*.tmp .env`, «borrar» si se pide y `↵`;
/// contesta el árbol remoto y espera la vista previa. Devuelve el
/// `ListarArbol` que se envió.
fn planificar_subida(
    prueba: &mut AppPrueba,
    borrar: bool,
) -> (u64, Vec<String>, bool, Vec<String>) {
    prueba.tecla(KeyCode::Char('S'));
    assert!(
        matches!(
            prueba.app.dialogo,
            Some(Dialogo::Sincronizar(DialogoSincronizar::Formulario(_)))
        ),
        "S abre SINCRONIZAR:\n{}",
        prueba.texto()
    );
    if borrar {
        prueba.tecla(KeyCode::Char(' '));
    }
    prueba.tecla(KeyCode::Tab);
    escribir(prueba, "*.tmp .env");
    prueba.tecla(KeyCode::Enter);
    let pedido = listar_arbol(prueba);
    let entradas = arbol_remoto(prueba);
    prueba.servidor(MensajeServidor::Arbol {
        peticion_id: pedido.0,
        entradas,
        magiignore: None,
        excluidos: 0,
        fin: true,
    });
    esperar(prueba, en_vista_previa);
    pedido
}

fn rutas_relativas(elementos: &[ElementoTransferencia], raiz: &str) -> Vec<String> {
    elementos
        .iter()
        .map(|elemento| {
            elemento
                .destino
                .strip_prefix(&format!("{raiz}/"))
                .unwrap_or(&elemento.destino)
                .to_string()
        })
        .collect()
}

// ---------------------------------------------------------------- flujo

/// S → diálogo → ↵ → `ListarArbol` con las exclusiones en orden (`[archivos]
/// excluir`, el `.magiignore` local y las extras aparte) → vista previa →
/// aviso de sensibles → `Transferir` plana con permisos y sincronización.
#[test]
fn s_planifica_una_subida_y_la_lanza_sin_deliberar() {
    let (mut prueba, sembrado) = con_archivos(200, 60, Tema::respaldo());
    let hogar = prueba.app.rutas.hogar.clone();
    fs::write(hogar.join(".magiignore"), "# notas\nnotas.md\n").unwrap();
    let (_, exclusiones, usar_magiignore, extras) = planificar_subida(&mut prueba, false);

    let mut esperadas = prueba.app.config.archivos.excluir.clone();
    esperadas.push("notas.md".to_string());
    assert_eq!(
        exclusiones, esperadas,
        "por defecto y después el .magiignore"
    );
    assert!(!usar_magiignore, "en una subida el .magiignore es el local");
    assert_eq!(extras, vec!["*.tmp".to_string(), ".env".to_string()]);

    let texto = prueba.texto();
    assert!(texto.contains("SINCRONIZAR · ad hoc"), "{texto}");
    assert!(texto.contains("+ 2 crear"), "{texto}");
    assert!(
        texto.contains("3 excluidos"),
        "notas.md, .env y .magiignore:\n{texto}"
    );
    assert!(texto.contains("~ config.yaml"), "{texto}");
    assert!(texto.contains("(era 1 kB)"), "{texto}");
    assert!(texto.contains("− 0 borrar"), "{texto}");
    assert!(!texto.contains("requerirá deliberación"), "{texto}");

    // ↵: sin sensibles (`.env` excluido) ni deliberación, se lanza.
    prueba.tecla(KeyCode::Enter);
    let enviada = transferir(&mut prueba).expect("Transferir");
    assert_eq!(enviada.direccion, Direccion::Subida);
    assert_eq!(enviada.politica, Politica::Sobrescribir);
    assert!(!enviada.borrar_origen);
    assert!(enviada.borrar_al_terminar.is_empty(), "sin «borrar»");
    assert!(enviada.deliberacion.is_none());
    assert_eq!(
        enviada.etiqueta,
        Some(EtiquetaTransferencia::Sincronizacion)
    );
    assert_eq!(
        rutas_relativas(&enviada.elementos, RUTA_REMOTA),
        vec!["README.md", "app", "config.yaml"],
        "plana, en orden, sin lo igual ni lo excluido"
    );
    let por_ruta = |relativa: &str| {
        enviada
            .elementos
            .iter()
            .find(|elemento| elemento.destino == format!("{RUTA_REMOTA}/{relativa}"))
            .unwrap()
            .clone()
    };
    let config = por_ruta("config.yaml");
    assert_eq!(
        config.origen,
        hogar.join("config.yaml").display().to_string()
    );
    assert_eq!(
        config.permisos,
        Some(0o640),
        "al actualizar, los del destino"
    );
    let readme = por_ruta("README.md");
    assert_eq!(readme.permisos, Some(0o644), "al crear, los del origen");
    assert!(por_ruta("app").es_directorio);
    let sincronizacion = enviada.sincronizacion.expect("sincronización");
    assert_eq!((sincronizacion.id, sincronizacion.nombre), (None, None));
    assert_eq!(
        (
            sincronizacion.creados,
            sincronizacion.actualizados,
            sincronizacion.omitidos
        ),
        (2, 1, 0)
    );
    assert_eq!(sincronizacion.raiz_origen, hogar.display().to_string());
    assert_eq!(sincronizacion.raiz_destino, RUTA_REMOTA);

    // El servidor la encola y, al terminar, la ventana lo dice.
    prueba.servidor(arnes::bienvenida(1, Vec::new()));
    let peticion_id = enviada.peticion_id.expect("peticion_id");
    prueba.servidor(MensajeServidor::Hecho {
        peticion_id,
        detalle: None,
    });
    assert!(texto_del_mensaje(&prueba).contains("encolada"));
    let mut fila = transferencia(EstadoTransferencia::Hecha, sembrado.host("hetzner-01"));
    fila.solicitante = 1;
    fila.peticion_id = Some(peticion_id);
    prueba.servidor(MensajeServidor::Transferencias { lista: vec![fila] });
    assert!(
        texto_del_mensaje(&prueba).contains("sincronización «ad hoc» hecha"),
        "{}",
        texto_del_mensaje(&prueba)
    );
    assert_eq!(prueba.app.sincronizar.lanzadas(), 0);
}

/// Con «borrar» la deliberación se abre y no sale nada hasta `Ctrl+K`; lo
/// que se borra son rutas absolutas del destino y nunca lo excluido.
#[test]
fn con_borrar_delibera_y_solo_ctrl_k_lanza() {
    let (mut prueba, _) = con_archivos(200, 60, Tema::respaldo());
    planificar_subida(&mut prueba, true);
    let texto = prueba.texto();
    assert!(
        texto.contains("borrará 1 elemento en el destino · requerirá deliberación MAGI"),
        "{texto}"
    );
    assert!(texto.contains("− viejo.css"), "{texto}");
    let Some(Dialogo::Sincronizar(DialogoSincronizar::VistaPrevia(vista))) = &prueba.app.dialogo
    else {
        unreachable!()
    };
    assert_eq!(
        vista.plan.plan.borrados().collect::<Vec<_>>(),
        vec!["viejo.css"],
        ".env está excluido: nunca se borra"
    );

    prueba.tecla(KeyCode::Enter);
    let abierta = prueba.app.deliberacion.as_ref().expect("deliberación");
    assert!(abierta
        .motivos
        .contains(&"borrar en destino (1)".to_string()));
    assert_eq!(
        abierta.plan.accion(),
        "sync ad hoc → hetzner-01: 3 ficheros, 1 borrados"
    );
    assert!(abierta.plan.snippet_id().is_none());
    // `↵` no ejecuta en el diálogo MAGI.
    prueba.tecla(KeyCode::Enter);
    assert!(transferir(&mut prueba).is_none());

    prueba.tecla_con(KeyCode::Char('k'), KeyModifiers::CONTROL);
    assert!(prueba.app.deliberacion.is_none());
    let enviada = transferir(&mut prueba).expect("Transferir tras Ctrl+K");
    assert_eq!(
        enviada.borrar_al_terminar,
        vec![format!("{RUTA_REMOTA}/viejo.css")]
    );
    let deliberacion = enviada.deliberacion.expect("deliberación lanzada");
    assert!(!deliberacion.forzada);
    let fila = prueba
        .app
        .almacen
        .obtener_deliberacion(deliberacion.id)
        .unwrap();
    assert_eq!(fila.snippet_id, None);

    // Un rechazo del servidor cierra la deliberación con «error».
    prueba.servidor(MensajeServidor::Error {
        peticion_id: enviada.peticion_id,
        mensaje: "no hay canal".to_string(),
    });
    let fila = prueba
        .app
        .almacen
        .obtener_deliberacion(deliberacion.id)
        .unwrap();
    assert_eq!(
        fila.ejecucion_resultado,
        Some(magi::deliberacion::EjecucionResultado::Error)
    );
}

/// Una subida a un host con verificaciones activas también delibera.
#[test]
fn una_subida_a_un_host_con_verificaciones_delibera() {
    let (mut prueba, sembrado) = con_archivos(200, 60, Tema::respaldo());
    prueba
        .app
        .almacen
        .guardar_verificaciones(
            sembrado.host("hetzner-01"),
            &magi::deliberacion::DatosVerificaciones {
                tests: true,
                tests_comando: Some("true".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
    planificar_subida(&mut prueba, false);
    assert!(prueba.texto().contains("requerirá deliberación MAGI"));
    prueba.tecla(KeyCode::Enter);
    let abierta = prueba.app.deliberacion.as_ref().expect("deliberación");
    assert!(abierta
        .motivos
        .contains(&"verificaciones en hetzner-01".to_string()));
    assert!(transferir(&mut prueba).is_none());
}

/// En una subida, lo que se crea o actualiza y casa con `[archivos] avisar`
/// pide confirmación antes de encolar.
#[test]
fn una_subida_con_sensibles_avisa_antes_de_encolar() {
    let (mut prueba, _) = con_archivos(200, 60, Tema::respaldo());
    prueba.tecla(KeyCode::Char('S'));
    prueba.tecla(KeyCode::Enter);
    let (peticion_id, ..) = listar_arbol(&mut prueba);
    prueba.servidor(MensajeServidor::Arbol {
        peticion_id,
        entradas: Vec::new(),
        magiignore: None,
        excluidos: 0,
        fin: true,
    });
    esperar(&mut prueba, en_vista_previa);
    prueba.tecla(KeyCode::Enter);
    assert!(
        matches!(prueba.app.dialogo, Some(Dialogo::Confirmar { .. })),
        "{}",
        prueba.texto()
    );
    assert!(prueba.texto().contains(".env"), "{}", prueba.texto());
    assert!(transferir(&mut prueba).is_none());
    prueba.tecla(KeyCode::Char('s'));
    assert!(transferir(&mut prueba).is_some());
}

/// En una bajada el `.magiignore` lo lee el servidor: el recuento del diálogo
/// sale del temporal, que se borra; el destino local se filtra con el texto
/// del primer `Arbol` y lo excluido no se borra.
#[test]
fn una_bajada_filtra_el_destino_local_con_el_magiignore_remoto() {
    let (mut prueba, _) = con_archivos(200, 60, Tema::respaldo());
    prueba.tecla(KeyCode::Tab);
    prueba.tecla(KeyCode::Char('S'));
    let (peticion_id, ruta) = prueba
        .enviados()
        .into_iter()
        .find_map(|mensaje| match mensaje {
            MensajeCliente::DescargarTemporal {
                peticion_id,
                ruta,
                edicion,
                ..
            } => {
                assert!(!edicion);
                Some((peticion_id, ruta))
            }
            _ => None,
        })
        .expect("se trae el .magiignore remoto");
    assert_eq!(ruta, format!("{RUTA_REMOTA}/.magiignore"));
    let temporal = prueba.app.rutas.runtime.join("magiignore-prueba");
    fs::create_dir_all(temporal.parent().unwrap()).unwrap();
    fs::write(&temporal, "*.md\n# comentario\nstatic/\n").unwrap();
    prueba.servidor(MensajeServidor::RutaTemporal {
        ruta: temporal.display().to_string(),
        peticion_id,
    });
    assert!(prueba
        .enviados()
        .iter()
        .any(|mensaje| matches!(mensaje, MensajeCliente::BorrarTemporal { .. })));
    assert!(
        prueba
            .texto()
            .contains(".magiignore del origen: 2 patrones"),
        "{}",
        prueba.texto()
    );

    // Con «borrar»: sobra todo lo local salvo lo que excluye el .magiignore.
    prueba.tecla(KeyCode::Char(' '));
    prueba.tecla(KeyCode::Enter);
    let (peticion_id, exclusiones, usar_magiignore, extras) = listar_arbol(&mut prueba);
    assert_eq!(exclusiones, prueba.app.config.archivos.excluir);
    assert!(usar_magiignore);
    assert!(extras.is_empty());
    prueba.servidor(MensajeServidor::Arbol {
        peticion_id,
        entradas: vec![fichero("main.py", 10, 1_700_000_000)],
        magiignore: Some("*.md\nstatic/\n".to_string()),
        excluidos: 4,
        fin: false,
    });
    assert!(!en_vista_previa(&prueba), "faltan bloques");
    prueba.servidor(MensajeServidor::Arbol {
        peticion_id,
        entradas: vec![fichero("nuevo.txt", 5, 1_700_000_000)],
        magiignore: None,
        excluidos: 4,
        fin: true,
    });
    esperar(&mut prueba, en_vista_previa);
    let texto = prueba.texto();
    assert!(texto.contains("4 excluidos"), "{texto}");
    let Some(Dialogo::Sincronizar(DialogoSincronizar::VistaPrevia(vista))) = &prueba.app.dialogo
    else {
        unreachable!()
    };
    let borrados: Vec<&str> = vista.plan.plan.borrados().collect();
    assert_eq!(
        borrados,
        vec![".env", "config.yaml", "app"],
        "ni README.md, ni notas.md, ni static/"
    );
}

/// Destino dentro del origen (o al revés): se rechaza sin abrir nada.
#[test]
fn destino_dentro_del_origen_se_rechaza() {
    let (mut prueba, _) = con_archivos(200, 60, Tema::respaldo());
    let hogar = prueba.app.rutas.hogar.display().to_string();
    prueba.app.archivos.as_mut().unwrap().remoto.ruta = format!("{hogar}/app");
    prueba.tecla(KeyCode::Char('S'));
    assert!(prueba.app.dialogo.is_none());
    assert!(
        texto_del_mensaje(&prueba).contains("destino dentro del origen"),
        "{}",
        texto_del_mensaje(&prueba)
    );
    // Al revés, desde el remoto: el origen es el padre del hogar.
    let padre = prueba
        .app
        .rutas
        .hogar
        .parent()
        .unwrap()
        .display()
        .to_string();
    prueba.app.archivos.as_mut().unwrap().remoto.ruta = padre;
    prueba.tecla(KeyCode::Tab);
    prueba.tecla(KeyCode::Char('S'));
    assert!(prueba.app.dialogo.is_none());
    assert!(texto_del_mensaje(&prueba).contains("destino dentro del origen"));
    // La misma ruta en los dos lados sí vale.
    prueba.app.archivos.as_mut().unwrap().remoto.ruta = hogar;
    prueba.tecla(KeyCode::Char('S'));
    assert!(prueba.app.dialogo.is_some());
}

/// Un nombre repetido al guardar se queda en el diálogo; uno válido crea la
/// guardada y planifica con ella.
#[test]
fn guardar_como_valida_dentro_del_dialogo() {
    let (mut prueba, sembrado) = con_archivos(200, 60, Tema::respaldo());
    prueba.tecla(KeyCode::Char('S'));
    prueba.tecla(KeyCode::Tab);
    prueba.tecla(KeyCode::Tab);
    prueba.tecla(KeyCode::Char(' '));
    prueba.tecla(KeyCode::Tab);
    escribir(&mut prueba, "mal nombre");
    prueba.tecla(KeyCode::Enter);
    let Some(Dialogo::Sincronizar(DialogoSincronizar::Formulario(formulario))) =
        &prueba.app.dialogo
    else {
        panic!("el error se queda en el diálogo:\n{}", prueba.texto());
    };
    assert!(formulario.error.is_some());
    assert!(prueba
        .app
        .almacen
        .listar_sincronizaciones()
        .unwrap()
        .is_empty());

    for _ in 0.."mal nombre".len() {
        prueba.tecla(KeyCode::Backspace);
    }
    escribir(&mut prueba, "web-prod");
    prueba.tecla(KeyCode::Enter);
    let guardadas = prueba
        .app
        .almacen
        .sincronizaciones_de_host(sembrado.host("hetzner-01"))
        .unwrap();
    assert_eq!(guardadas.len(), 1);
    assert_eq!(guardadas[0].nombre, "web-prod");
    assert_eq!(guardadas[0].direccion, Direccion::Subida);
    let (peticion_id, ..) = listar_arbol(&mut prueba);
    assert!(prueba.app.sincronizar.planificando());
    prueba.servidor(MensajeServidor::Arbol {
        peticion_id,
        entradas: Vec::new(),
        magiignore: None,
        excluidos: 0,
        fin: true,
    });
    esperar(&mut prueba, en_vista_previa);
    assert!(prueba.texto().contains("SINCRONIZAR · web-prod"));

    // Al lanzarla (el servidor la acepta) se escribe su última ejecución.
    prueba.tecla(KeyCode::Enter);
    if matches!(prueba.app.dialogo, Some(Dialogo::Confirmar { .. })) {
        // El aviso de sensibles (`.env`).
        prueba.tecla(KeyCode::Char('s'));
    }
    let enviada = transferir(&mut prueba).expect("Transferir");
    let sincronizacion = enviada.sincronizacion.expect("sincronización");
    assert_eq!(sincronizacion.id, Some(guardadas[0].id));
    assert_eq!(sincronizacion.nombre.as_deref(), Some("web-prod"));
    assert!(guardadas[0].ultima_ejecucion_en.is_none());
    prueba.servidor(MensajeServidor::Hecho {
        peticion_id: enviada.peticion_id.unwrap(),
        detalle: None,
    });
    let guardada = prueba
        .app
        .almacen
        .obtener_sincronizacion(guardadas[0].id)
        .unwrap();
    assert!(guardada.ultima_ejecucion_en.is_some());
}

/// Un plan sin nada que hacer no abre la vista previa: «todo al día».
#[test]
fn un_plan_vacio_dice_todo_al_dia() {
    let (mut prueba, _) = con_archivos(200, 60, Tema::respaldo());
    let hogar = prueba.app.rutas.hogar.clone();
    let vacio = hogar.join("app");
    prueba.app.archivos.as_mut().unwrap().local.ruta = vacio.display().to_string();
    prueba.tecla(KeyCode::Char('S'));
    prueba.tecla(KeyCode::Enter);
    let (peticion_id, ..) = listar_arbol(&mut prueba);
    prueba.servidor(MensajeServidor::Arbol {
        peticion_id,
        entradas: Vec::new(),
        magiignore: None,
        excluidos: 0,
        fin: true,
    });
    esperar(&mut prueba, |prueba| !prueba.app.sincronizar.planificando());
    assert!(prueba.app.dialogo.is_none());
    assert!(
        texto_del_mensaje(&prueba).contains("todo al día"),
        "{}",
        texto_del_mensaje(&prueba)
    );
}

/// Un error del árbol remoto (ruta que ya no existe) termina con mensaje.
#[test]
fn un_error_del_arbol_remoto_abandona_el_plan() {
    let (mut prueba, _) = con_archivos(200, 60, Tema::respaldo());
    prueba.tecla(KeyCode::Char('S'));
    prueba.tecla(KeyCode::Enter);
    let (peticion_id, ..) = listar_arbol(&mut prueba);
    prueba.servidor(MensajeServidor::Error {
        peticion_id: Some(peticion_id),
        mensaje: "/var/www/cooperapp: no existe".to_string(),
    });
    assert!(!prueba.app.sincronizar.planificando());
    assert!(texto_del_mensaje(&prueba).contains("no existe"));
}

/// Si el servidor cae a medio planificar, el plan se abandona (y el árbol
/// local que llegue después se descarta).
#[test]
fn el_servidor_caido_abandona_el_plan() {
    let (mut prueba, _) = con_archivos(200, 60, Tema::respaldo());
    prueba.tecla(KeyCode::Char('S'));
    prueba.tecla(KeyCode::Enter);
    listar_arbol(&mut prueba);
    assert!(prueba.app.sincronizar.planificando());
    prueba.evento(magi::app::Evento::ServidorCaido);
    assert!(!prueba.app.sincronizar.planificando());
    prueba
        .app
        .bombear(&mut prueba.terminal, Duration::from_millis(200))
        .unwrap();
    assert!(!en_vista_previa(&prueba));
}

/// En la vista previa `f` filtra por tipo y `↑` `↓` `PgDn` mueven dentro de
/// lo filtrado; `Esc` cancela sin enviar nada.
#[test]
fn la_vista_previa_filtra_y_se_desplaza() {
    let (mut prueba, _) = AppPrueba::con_semilla(80, 24);
    super::vistas_g::abrir_dialogo(&mut prueba, vista_previa());
    prueba.tecla(KeyCode::Char('f'));
    let texto = prueba.texto();
    assert!(texto.contains("filtro: crear"), "{texto}");
    assert!(!texto.contains("main.py"), "{texto}");
    assert!(texto.contains("static/js/app.js"), "{texto}");
    prueba.tecla(KeyCode::PageDown);
    let ventana = prueba.app.disposicion().lista(Lista::VistaPrevia).unwrap();
    assert_eq!(ventana.total, 2);
    for _ in 0..3 {
        prueba.tecla(KeyCode::Char('f'));
    }
    assert!(prueba.texto().contains("filtro: omitidos"));
    prueba.tecla(KeyCode::Char('f'));
    assert!(prueba.texto().contains("f filtrar"));
    prueba.tecla(KeyCode::Esc);
    assert!(prueba.app.dialogo.is_none());
    assert!(prueba.enviados().is_empty());
}

/// La lista del plan sigue la selección a la vista en cualquier tamaño.
#[test]
fn la_vista_previa_se_desplaza_al_encoger() {
    let (mut prueba, _) = AppPrueba::con_semilla(200, 60);
    super::vistas_g::abrir_dialogo(&mut prueba, vista_previa());
    for _ in 0..6 {
        prueba.tecla(KeyCode::Down);
    }
    for (cols, filas) in [(80, 24), (40, 12), (60, 14), (200, 60)] {
        prueba.pasar_por(cols, filas);
        let ventana = prueba.app.disposicion().lista(Lista::VistaPrevia).unwrap();
        assert!(
            ventana.inicio + ventana.filas > 6,
            "selección fuera de la vista a {cols}×{filas}: {ventana:?}"
        );
    }
}

// ---------------------------------------------------------------- instantáneas

fn transferencia(estado: EstadoTransferencia, host_id: i64) -> InfoTransferencia {
    InfoTransferencia {
        id: 7,
        host_id,
        host_nombre: "hetzner-01".to_string(),
        direccion: Direccion::Subida,
        estado,
        origen: "/home/hector/proyectos/cooperapp".to_string(),
        destino: RUTA_REMOTA.to_string(),
        es_directorio: true,
        ficheros_total: 15,
        ficheros_hechos: 15,
        omitidos: 0,
        bytes_total: 4_404_019,
        bytes_hechos: 4_404_019,
        fichero_actual: None,
        error: None,
        borrar_origen: false,
        solicitante: 99,
        creada_en: 1_757_670_067,
        terminada_en: estado.terminada().then_some(1_757_670_100),
        peticion_id: None,
        etiqueta: Some(EtiquetaTransferencia::Sincronizacion),
        borrados: 1,
        borrados_total: 2,
    }
}

#[test]
fn instantaneas_de_sincronizar_y_vista_previa() {
    for (cols, filas) in TAMANOS {
        let (mut prueba, _) = con_archivos(cols, filas, Tema::respaldo());
        super::vistas_g::abrir_dialogo(&mut prueba, formulario());
        prueba.instantanea(&format!("sincronizar_{cols}x{filas}"));

        let (mut prueba, _) = con_archivos(cols, filas, Tema::respaldo());
        super::vistas_g::abrir_dialogo(&mut prueba, vista_previa());
        prueba.instantanea(&format!("vista_previa_{cols}x{filas}"));
    }
}

#[test]
fn instantaneas_de_transferencias_borrando() {
    for (cols, filas) in TAMANOS {
        let (mut prueba, sembrado) = con_archivos(80, 24, Tema::respaldo());
        let mut hecha = transferencia(EstadoTransferencia::Hecha, sembrado.host("hetzner-01"));
        hecha.id = 6;
        hecha.borrados = 0;
        hecha.borrados_total = 0;
        hecha.origen = "/home/hector/notas".to_string();
        hecha.destino = "/srv/notas".to_string();
        prueba.servidor(MensajeServidor::Transferencias {
            lista: vec![
                transferencia(EstadoTransferencia::Borrando, sembrado.host("hetzner-01")),
                hecha,
            ],
        });
        prueba.tecla(KeyCode::Char('t'));
        assert_eq!(prueba.app.vista, Vista::Transferencias);
        prueba.redimensionar(cols, filas);
        prueba.instantanea(&format!("transferencias_borrando_{cols}x{filas}"));
        if (cols, filas) != (40, 12) {
            let texto = prueba.texto();
            assert!(texto.contains("borrando 1/2"), "{texto}");
            assert!(texto.contains("1 borrados"), "{texto}");
        }
    }
}

/// En ASCII los signos son `+ ~ - .` y no queda ningún glifo Unicode.
#[test]
fn la_vista_previa_en_ascii() {
    let (mut prueba, _) = AppPrueba::con_semilla_y_tema(100, 30, arnes::tema_ascii());
    super::vistas_g::abrir_dialogo(&mut prueba, vista_previa());
    let texto = prueba.texto();
    assert!(texto.contains("+ 2 crear"), "{texto}");
    assert!(texto.contains("- 2 borrar"), "{texto}");
    assert!(texto.contains(". static/compartido"), "{texto}");
    assert!(texto.contains("! borrará 2 elementos"), "{texto}");
}
