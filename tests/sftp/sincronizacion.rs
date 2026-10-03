//! Fase 8 · Transferencias de la Fase 8: permisos tras el `rename`, `borrar_al_terminar` solo con éxito, deliberación y `ultimo_resultado`.

#[allow(unused_imports)]
use super::*;

use std::os::unix::fs::PermissionsExt as _;

use magi::almacen::deliberaciones;
use magi::deliberacion::{ComprobacionesHost, NuevaDeliberacion, ResultadoDeliberacion, Veredicto};
use magi::modelo::{DatosSincronizacion, ResultadoSincronizacion};
use magi::protocolo::{DeliberacionLanzada, EtiquetaTransferencia, SincronizacionLanzada};

/// Una `Transferir` con los campos de la Fase 8 (la política es siempre
/// «sobrescribir», como en una sincronización o una edición).
struct Encargo {
    direccion: Direccion,
    elementos: Vec<ElementoTransferencia>,
    borrar_al_terminar: Vec<String>,
    deliberacion: Option<DeliberacionLanzada>,
    sincronizacion: Option<SincronizacionLanzada>,
    etiqueta: Option<EtiquetaTransferencia>,
}

impl Encargo {
    /// La transferencia de una sincronización: lista plana y sus raíces.
    fn sincronizacion(
        direccion: Direccion,
        elementos: Vec<ElementoTransferencia>,
        borrar_al_terminar: Vec<String>,
        sincronizacion: SincronizacionLanzada,
    ) -> Self {
        Self {
            direccion,
            elementos,
            borrar_al_terminar,
            deliberacion: None,
            sincronizacion: Some(sincronizacion),
            etiqueta: Some(EtiquetaTransferencia::Sincronizacion),
        }
    }
}

/// Manda la `Transferir` con su `peticion_id` y espera la respuesta: el id de
/// su fila en la cola si fue `Hecho`, el motivo si fue `Error`.
async fn encargar(
    montaje: &mut Montaje,
    peticion_id: u64,
    encargo: Encargo,
) -> Result<u32, String> {
    let host_id = montaje.host_id;
    montaje
        .enviar(&MensajeCliente::Transferir {
            host_id,
            direccion: encargo.direccion,
            elementos: encargo.elementos,
            politica: Politica::Sobrescribir,
            borrar_origen: false,
            peticion_id: Some(peticion_id),
            borrar_al_terminar: encargo.borrar_al_terminar,
            deliberacion: encargo.deliberacion,
            sincronizacion: encargo.sincronizacion,
            etiqueta: encargo.etiqueta,
        })
        .await;
    let respuesta = montaje
        .esperar(|mensaje| match mensaje {
            MensajeServidor::Hecho {
                peticion_id: id, ..
            } => *id == peticion_id,
            MensajeServidor::Error {
                peticion_id: Some(id),
                ..
            } => *id == peticion_id,
            _ => false,
        })
        .await;
    if let MensajeServidor::Error { mensaje, .. } = respuesta {
        return Err(mensaje);
    }
    // La fila llega por la difusión de la cola; la ventana la reconoce por
    // su `peticion_id`.
    loop {
        if let Some(fila) = montaje
            .cola
            .iter()
            .find(|fila| fila.peticion_id == Some(peticion_id))
        {
            return Ok(fila.id);
        }
        montaje
            .esperar(|mensaje| matches!(mensaje, MensajeServidor::Transferencias { .. }))
            .await;
    }
}

fn texto(ruta: &Path) -> String {
    ruta.display().to_string()
}

fn elemento(origen: &Path, destino: &Path, permisos: Option<u32>) -> ElementoTransferencia {
    let bytes = std::fs::metadata(origen).map(|m| m.len()).unwrap_or(0);
    ElementoTransferencia {
        origen: texto(origen),
        destino: texto(destino),
        bytes,
        es_directorio: false,
        politica: Some(Politica::Sobrescribir),
        permisos,
    }
}

fn directorio(origen: &Path, destino: &Path, permisos: Option<u32>) -> ElementoTransferencia {
    ElementoTransferencia {
        es_directorio: true,
        bytes: 0,
        ..elemento(origen, destino, permisos)
    }
}

fn lanzada(
    id: Option<i64>,
    nombre: Option<&str>,
    origen: &Path,
    destino: &Path,
    creados: u32,
) -> SincronizacionLanzada {
    SincronizacionLanzada {
        id,
        nombre: nombre.map(str::to_string),
        creados,
        actualizados: 0,
        omitidos: 0,
        raiz_origen: texto(origen),
        raiz_destino: texto(destino),
    }
}

fn modo(ruta: &Path) -> u32 {
    std::fs::symlink_metadata(ruta)
        .unwrap_or_else(|error| panic!("{}: {error}", ruta.display()))
        .permissions()
        .mode()
        & 0o7777
}

/// Filas de `REGISTRO` de un tipo: detalle y resultado, de la más antigua a
/// la más nueva.
fn registro(montaje: &Montaje, tipo: &str) -> Vec<(String, String)> {
    let almacen = Almacen::abrir(&montaje.entorno.rutas.base_datos()).unwrap();
    let filas = {
        let mut sentencia = almacen
            .conexion()
            .prepare("SELECT detalle, resultado FROM REGISTRO WHERE tipo = ?1 ORDER BY id")
            .unwrap();
        sentencia
            .query_map([tipo], |fila| Ok((fila.get(0)?, fila.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<(String, String)>>>()
            .unwrap()
    };
    almacen.cerrar().unwrap();
    filas
}

/// El hilo escritor del servidor es asíncrono: se le da un respiro.
async fn hasta<F: FnMut() -> bool>(mut condicion: F) -> bool {
    for _ in 0..50 {
        if condicion() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    condicion()
}

fn guardar_sincronizacion(montaje: &Montaje, local: &Path, remoto: &Path) -> i64 {
    let almacen = Almacen::abrir(&montaje.entorno.rutas.base_datos()).unwrap();
    let id = almacen
        .crear_sincronizacion(&DatosSincronizacion {
            host_id: montaje.host_id,
            nombre: "web-prod".to_string(),
            ruta_local: texto(local),
            ruta_remota: texto(remoto),
            direccion: Direccion::Subida,
            borrar: true,
            exclusiones: Vec::new(),
        })
        .unwrap();
    almacen.cerrar().unwrap();
    id
}

fn ultimo_resultado(montaje: &Montaje, id: i64) -> Option<ResultadoSincronizacion> {
    let almacen = Almacen::abrir(&montaje.entorno.rutas.base_datos()).unwrap();
    let resultado = almacen.obtener_sincronizacion(id).unwrap().ultimo_resultado;
    almacen.cerrar().unwrap();
    resultado
}

/// AC (checklist, `borrar_al_terminar`): una sincronización con 15 ficheros y
/// 2 borrados en la que falla un fichero termina sin borrar nada, en error y
/// con el resultado «parcial».
#[tokio::test]
async fn quince_ficheros_y_dos_borrados_si_falla_uno_no_se_borra_nada_y_queda_parcial() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let origen = montaje.local().join("web");
    let destino = montaje.remoto().join("app");
    std::fs::create_dir_all(&origen).unwrap();
    std::fs::create_dir_all(&destino).unwrap();
    for numero in 1..=15 {
        std::fs::write(origen.join(format!("f{numero:02}.txt")), b"contenido").unwrap();
    }
    // El quinto no se puede leer: el servidor falla al copiarlo.
    let ilegible = origen.join("f05.txt");
    std::fs::set_permissions(&ilegible, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::File::open(&ilegible).is_ok() {
        eprintln!("[AVISO] se ejecuta como root: un 000 se lee igual; se salta la prueba");
        return;
    }
    std::fs::write(destino.join("viejo1.txt"), b"sobra").unwrap();
    std::fs::write(destino.join("viejo2.txt"), b"sobra").unwrap();
    let guardada = guardar_sincronizacion(&montaje, &origen, &destino);
    montaje.abrir_sftp().await;

    let elementos = (1..=15)
        .map(|numero| {
            let nombre = format!("f{numero:02}.txt");
            elemento(&origen.join(&nombre), &destino.join(&nombre), Some(0o644))
        })
        .collect();
    let id = encargar(
        &mut montaje,
        100,
        Encargo::sincronizacion(
            Direccion::Subida,
            elementos,
            vec![
                texto(&destino.join("viejo1.txt")),
                texto(&destino.join("viejo2.txt")),
            ],
            lanzada(Some(guardada), Some("web-prod"), &origen, &destino, 15),
        ),
    )
    .await
    .expect("se encola");
    let fila = montaje.esperar_terminada(id).await;
    assert_eq!(fila.estado, EstadoTransferencia::Error, "{fila:?}");
    assert!(
        fila.error.as_deref().unwrap_or("").contains("f05.txt"),
        "{fila:?}"
    );
    assert_eq!((fila.borrados, fila.borrados_total), (0, 2));
    assert!(destino.join("viejo1.txt").exists(), "no se borra nada");
    assert!(destino.join("viejo2.txt").exists(), "no se borra nada");
    assert!(destino.join("f04.txt").exists(), "lo copiado se queda");

    assert!(
        hasta(|| ultimo_resultado(&montaje, guardada) == Some(ResultadoSincronizacion::Parcial))
            .await,
        "ultimo_resultado: {:?}",
        ultimo_resultado(&montaje, guardada)
    );
    let filas = registro(&montaje, "sincronizacion");
    assert_eq!(filas.len(), 1, "{filas:?}");
    let (detalle, resultado) = &filas[0];
    assert!(
        detalle.starts_with("«web-prod» · subida · +15 ~0 −0 · "),
        "{detalle}"
    );
    assert!(detalle.contains(" · parcial · "), "{detalle}");
    assert_eq!(resultado, "error");
    assert!(
        registro(&montaje, "transferencia").is_empty(),
        "una sincronización se anota como sincronización"
    );
    std::fs::set_permissions(&ilegible, std::fs::Permissions::from_mode(0o644)).unwrap();
}

/// Con la copia sin errores se borra lo fijado: los ficheros (y los enlaces,
/// como enlaces) primero y los directorios después, del más profundo al menos
/// aunque lleguen en otro orden. Un directorio con algo que no está en la
/// lista se conserva.
#[tokio::test]
async fn una_sincronizacion_que_termina_bien_borra_lo_fijado_y_conserva_los_no_vacios() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let origen = montaje.local().join("web");
    let destino = montaje.remoto().join("app");
    std::fs::create_dir_all(&origen).unwrap();
    std::fs::write(origen.join("nuevo.txt"), b"nuevo").unwrap();
    std::fs::create_dir_all(destino.join("viejo/hondo")).unwrap();
    std::fs::write(destino.join("viejo/hondo/a.txt"), b"a").unwrap();
    std::fs::write(destino.join("viejo/b.txt"), b"b").unwrap();
    std::fs::write(destino.join("suelto.txt"), b"s").unwrap();
    std::fs::create_dir_all(destino.join("conserva")).unwrap();
    std::fs::write(destino.join("conserva/excluido.log"), b"x").unwrap();
    std::fs::create_dir_all(destino.join("fuera")).unwrap();
    std::fs::write(destino.join("fuera/dentro.txt"), b"f").unwrap();
    std::os::unix::fs::symlink(destino.join("fuera"), destino.join("enlace")).unwrap();
    montaje.abrir_sftp().await;

    let r = |relativa: &str| texto(&destino.join(relativa));
    let id = encargar(
        &mut montaje,
        200,
        Encargo::sincronizacion(
            Direccion::Subida,
            vec![elemento(
                &origen.join("nuevo.txt"),
                &destino.join("nuevo.txt"),
                Some(0o644),
            )],
            vec![
                r("viejo"),
                r("viejo/hondo"),
                r("conserva"),
                r("viejo/b.txt"),
                r("suelto.txt"),
                r("enlace"),
                r("viejo/hondo/a.txt"),
            ],
            lanzada(None, None, &origen, &destino, 1),
        ),
    )
    .await
    .expect("se encola");
    let fila = montaje.esperar_terminada(id).await;
    assert_eq!(fila.estado, EstadoTransferencia::Hecha, "{fila:?}");
    assert_eq!((fila.borrados, fila.borrados_total), (6, 7));
    assert_eq!(fila.origen, texto(&origen), "la fila enseña las raíces");
    assert_eq!(fila.destino, texto(&destino));
    assert_eq!(leer(&destino.join("nuevo.txt")), "nuevo");
    assert!(!destino.join("viejo").exists());
    assert!(!destino.join("suelto.txt").exists());
    assert!(std::fs::symlink_metadata(destino.join("enlace")).is_err());
    assert!(
        destino.join("fuera/dentro.txt").exists(),
        "un enlace se borra como enlace, nunca se sigue"
    );
    assert!(destino.join("conserva/excluido.log").exists());

    assert!(hasta(|| !registro(&montaje, "sincronizacion").is_empty()).await);
    let (detalle, resultado) = registro(&montaje, "sincronizacion").remove(0);
    assert!(
        detalle.starts_with("ad hoc · subida · +1 ~0 −6 · 1 directorio no vacío conservado · "),
        "{detalle}"
    );
    assert!(detalle.ends_with(" · ok"), "{detalle}");
    assert_eq!(resultado, "ok");
}

/// Nada se borra ni se escribe fuera de la raíz de destino: las rutas con
/// `..`, la raíz misma, las de fuera y las relativas se rechazan al encolar,
/// con `Error` y su `peticion_id`.
#[tokio::test]
async fn las_rutas_de_borrado_fuera_de_la_raiz_o_con_puntos_se_rechazan_al_encolar() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let origen = montaje.local().join("web");
    let destino = montaje.remoto().join("app");
    std::fs::create_dir_all(&origen).unwrap();
    std::fs::create_dir_all(&destino).unwrap();
    std::fs::write(origen.join("a.txt"), b"a").unwrap();
    let secreto = montaje.remoto().join("secreto.txt");
    std::fs::write(&secreto, b"no se toca").unwrap();
    montaje.abrir_sftp().await;

    let a = || {
        vec![elemento(
            &origen.join("a.txt"),
            &destino.join("a.txt"),
            None,
        )]
    };
    let casos: Vec<(Vec<ElementoTransferencia>, Vec<String>)> = vec![
        (a(), vec![format!("{}/../secreto.txt", texto(&destino))]),
        (a(), vec![texto(&secreto)]),
        (a(), vec![texto(&destino)]),
        (a(), vec!["secreto.txt".to_string()]),
        (
            vec![elemento(&origen.join("a.txt"), &secreto, None)],
            Vec::new(),
        ),
    ];
    for (numero, (elementos, borrar)) in casos.into_iter().enumerate() {
        let peticion_id = 300 + numero as u64;
        let respuesta = encargar(
            &mut montaje,
            peticion_id,
            Encargo::sincronizacion(
                Direccion::Subida,
                elementos,
                borrar.clone(),
                lanzada(None, None, &origen, &destino, 1),
            ),
        )
        .await;
        assert!(respuesta.is_err(), "{borrar:?}: {respuesta:?}");
    }
    // Sin sincronización no hay borrado al terminar.
    let respuesta = encargar(
        &mut montaje,
        310,
        Encargo {
            direccion: Direccion::Subida,
            elementos: a(),
            borrar_al_terminar: vec![texto(&destino.join("a.txt"))],
            deliberacion: None,
            sincronizacion: None,
            etiqueta: None,
        },
    )
    .await;
    assert!(respuesta.is_err(), "{respuesta:?}");
    assert_eq!(leer(&secreto), "no se toca");
    assert!(!destino.join("a.txt").exists(), "nada se encoló");
    assert!(montaje.cola.is_empty(), "{:?}", montaje.cola);
}

/// Los permisos de cada elemento se aplican al crear: ficheros tras el
/// `rename` y directorios solo si los crea esta transferencia (uno que ya
/// existía no se toca). Un directorio sin escritura para el dueño se llena
/// igual y acaba con su modo.
#[tokio::test]
async fn una_subida_aplica_los_permisos_de_cada_elemento_al_crear() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let origen = montaje.local().join("web");
    let destino = montaje.remoto().join("app");
    for directorio in ["privado", "solo_lectura", "existente"] {
        std::fs::create_dir_all(origen.join(directorio)).unwrap();
    }
    std::fs::write(origen.join("script.sh"), b"#!/bin/sh\n").unwrap();
    std::fs::write(origen.join("privado/clave.txt"), b"secreto").unwrap();
    std::fs::write(origen.join("solo_lectura/dentro.txt"), b"fijo").unwrap();
    std::fs::create_dir_all(destino.join("existente")).unwrap();
    std::fs::set_permissions(
        destino.join("existente"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    montaje.abrir_sftp().await;

    let par = |relativa: &str| (origen.join(relativa), destino.join(relativa));
    let (o, d) = par("privado");
    let mut elementos = vec![directorio(&o, &d, Some(0o700))];
    let (o, d) = par("privado/clave.txt");
    elementos.push(elemento(&o, &d, Some(0o600)));
    let (o, d) = par("script.sh");
    elementos.push(elemento(&o, &d, Some(0o750)));
    let (o, d) = par("solo_lectura");
    elementos.push(directorio(&o, &d, Some(0o555)));
    let (o, d) = par("solo_lectura/dentro.txt");
    elementos.push(elemento(&o, &d, Some(0o444)));
    let (o, d) = par("existente");
    elementos.push(directorio(&o, &d, Some(0o700)));

    let id = encargar(
        &mut montaje,
        400,
        Encargo::sincronizacion(
            Direccion::Subida,
            elementos,
            Vec::new(),
            lanzada(None, None, &origen, &destino, 6),
        ),
    )
    .await
    .expect("se encola");
    let fila = montaje.esperar_terminada(id).await;
    assert_eq!(fila.estado, EstadoTransferencia::Hecha, "{fila:?}");
    assert_eq!(modo(&destino.join("privado")), 0o700);
    assert_eq!(modo(&destino.join("privado/clave.txt")), 0o600);
    assert_eq!(modo(&destino.join("script.sh")), 0o750);
    assert_eq!(modo(&destino.join("solo_lectura")), 0o555);
    assert_eq!(modo(&destino.join("solo_lectura/dentro.txt")), 0o444);
    assert_eq!(
        modo(&destino.join("existente")),
        0o755,
        "un directorio que ya existía no se toca"
    );
    assert_eq!(leer(&destino.join("solo_lectura/dentro.txt")), "fijo");
    // El temporal se borra al acabar: el directorio tiene que dejarse.
    std::fs::set_permissions(
        destino.join("solo_lectura"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
}

/// En bajada, la lista de una sincronización llega plana: un directorio solo
/// se crea (con sus permisos) y nunca se recorre en el remoto.
#[tokio::test]
async fn una_bajada_de_sincronizacion_aplica_los_permisos_y_no_recorre_directorios() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let raiz_origen = montaje.remoto();
    let origen = raiz_origen.join("datos");
    let destino = montaje.local().join("copia");
    std::fs::create_dir_all(&origen).unwrap();
    std::fs::create_dir_all(&destino).unwrap();
    std::fs::write(origen.join("a.txt"), b"de alla").unwrap();
    std::fs::write(origen.join("no_listado.txt"), b"fuera del plan").unwrap();
    montaje.abrir_sftp().await;

    let id = encargar(
        &mut montaje,
        500,
        Encargo::sincronizacion(
            Direccion::Bajada,
            vec![
                directorio(&origen, &destino.join("datos"), Some(0o750)),
                elemento(
                    &origen.join("a.txt"),
                    &destino.join("datos/a.txt"),
                    Some(0o640),
                ),
            ],
            Vec::new(),
            lanzada(None, None, &raiz_origen, &destino, 2),
        ),
    )
    .await
    .expect("se encola");
    let fila = montaje.esperar_terminada(id).await;
    assert_eq!(fila.estado, EstadoTransferencia::Hecha, "{fila:?}");
    assert_eq!(leer(&destino.join("datos/a.txt")), "de alla");
    assert_eq!(modo(&destino.join("datos/a.txt")), 0o640);
    assert_eq!(modo(&destino.join("datos")), 0o750);
    assert!(
        !destino.join("datos/no_listado.txt").exists(),
        "lo que no está en el plan no se baja"
    );
}

/// Una sincronización en bajada borra en el disco local, con las mismas
/// reglas: ficheros primero, directorios vacíos después y los no vacíos se
/// conservan.
#[tokio::test]
async fn una_sincronizacion_en_bajada_borra_en_local() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let origen = montaje.remoto().join("fuente");
    let destino = montaje.local().join("destino");
    std::fs::create_dir_all(&origen).unwrap();
    std::fs::write(origen.join("nuevo.txt"), b"nuevo").unwrap();
    std::fs::create_dir_all(destino.join("viejo_dir")).unwrap();
    std::fs::write(destino.join("viejo_dir/x.txt"), b"x").unwrap();
    std::fs::write(destino.join("viejo.txt"), b"v").unwrap();
    std::fs::create_dir_all(destino.join("conserva")).unwrap();
    std::fs::write(destino.join("conserva/.env"), b"excluido").unwrap();
    montaje.abrir_sftp().await;

    let id = encargar(
        &mut montaje,
        600,
        Encargo::sincronizacion(
            Direccion::Bajada,
            vec![elemento(
                &origen.join("nuevo.txt"),
                &destino.join("nuevo.txt"),
                Some(0o644),
            )],
            vec![
                texto(&destino.join("viejo_dir")),
                texto(&destino.join("viejo.txt")),
                texto(&destino.join("viejo_dir/x.txt")),
                texto(&destino.join("conserva")),
            ],
            lanzada(None, None, &origen, &destino, 1),
        ),
    )
    .await
    .expect("se encola");
    let fila = montaje.esperar_terminada(id).await;
    assert_eq!(fila.estado, EstadoTransferencia::Hecha, "{fila:?}");
    assert_eq!((fila.borrados, fila.borrados_total), (3, 4));
    assert_eq!(leer(&destino.join("nuevo.txt")), "nuevo");
    assert!(!destino.join("viejo_dir").exists());
    assert!(!destino.join("viejo.txt").exists());
    assert_eq!(leer(&destino.join("conserva/.env")), "excluido");
}

/// Inserta una deliberación aprobada como la dejaría el cliente.
fn deliberacion(montaje: &Montaje) -> i64 {
    let almacen = Almacen::abrir(&montaje.entorno.rutas.base_datos()).unwrap();
    let id = deliberaciones::crear(
        almacen.conexion(),
        &NuevaDeliberacion {
            snippet_id: None,
            accion: "sync web-prod → prueba: 1 ficheros, 0 borrados".to_string(),
            hosts: vec![(montaje.host_id, "prueba".to_string())],
            comprobaciones: vec![ComprobacionesHost {
                host_id: montaje.host_id,
                host: "prueba".to_string(),
                salud: Veredicto::Aprueba {
                    detalle: "NOMINAL".to_string(),
                    ms: 10,
                },
                backup: Veredicto::NoActiva,
                tests: Veredicto::NoActiva,
            }],
            resultado: ResultadoDeliberacion::Aprobada,
            bloqueada: false,
            motivo: None,
            usuario: "hector".to_string(),
        },
    )
    .unwrap();
    almacen.cerrar().unwrap();
    id
}

fn resultado_deliberacion(montaje: &Montaje, id: i64) -> Option<String> {
    let almacen = Almacen::abrir(&montaje.entorno.rutas.base_datos()).unwrap();
    let resultado = almacen
        .conexion()
        .query_row(
            "SELECT ejecucion_resultado FROM DELIBERACIONES WHERE id = ?1",
            [id],
            |fila| fila.get(0),
        )
        .unwrap();
    almacen.cerrar().unwrap();
    resultado
}

/// Una sincronización deliberada cierra su deliberación al terminar
/// (`ejecucion_resultado` y `deliberacion_aprobada`), y esa deliberación no
/// autoriza nada más: ni mientras corre ni después.
#[tokio::test]
async fn una_sincronizacion_deliberada_cierra_la_deliberacion_y_no_se_puede_reutilizar() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let origen = montaje.local().join("web");
    let destino = montaje.remoto().join("app");
    std::fs::create_dir_all(&origen).unwrap();
    std::fs::create_dir_all(&destino).unwrap();
    std::fs::write(origen.join("a.txt"), b"a").unwrap();
    std::fs::write(origen.join("b.txt"), b"b").unwrap();
    let deliberacion_id = deliberacion(&montaje);
    montaje.abrir_sftp().await;

    let encargo = |nombre: &str| Encargo {
        deliberacion: Some(DeliberacionLanzada {
            id: deliberacion_id,
            forzada: false,
            motivo: None,
        }),
        ..Encargo::sincronizacion(
            Direccion::Subida,
            vec![elemento(
                &origen.join(nombre),
                &destino.join(nombre),
                Some(0o644),
            )],
            Vec::new(),
            lanzada(None, Some("web-prod"), &origen, &destino, 1),
        )
    };
    let id = encargar(&mut montaje, 700, encargo("a.txt"))
        .await
        .expect("la deliberación válida autoriza");
    // Recién encolada la primera, la misma deliberación ya no vale.
    let repetida = encargar(&mut montaje, 701, encargo("b.txt")).await;
    assert!(repetida.is_err(), "{repetida:?}");
    let fila = montaje.esperar_terminada(id).await;
    assert_eq!(fila.estado, EstadoTransferencia::Hecha, "{fila:?}");
    assert!(!destino.join("b.txt").exists());

    assert!(hasta(|| resultado_deliberacion(&montaje, deliberacion_id).is_some()).await);
    assert_eq!(
        resultado_deliberacion(&montaje, deliberacion_id).as_deref(),
        Some("ok")
    );
    let anotadas = registro(&montaje, "deliberacion_aprobada");
    let (detalle, _) = anotadas
        .iter()
        .find(|(detalle, _)| detalle.starts_with(&format!("deliberación #{deliberacion_id} · ")))
        .expect("sin anotación de la deliberación");
    assert!(detalle.contains("resultado ok"), "{detalle}");
    assert_eq!(registro(&montaje, "sincronizacion").len(), 1);

    // Y ya terminada, tampoco.
    let despues = encargar(&mut montaje, 702, encargo("b.txt")).await;
    assert!(despues.is_err(), "{despues:?}");
}

/// La subida de una edición sobrescribe el original, le devuelve sus permisos
/// tras el `rename` (T56) y se anota como `transferencia` con «edición».
#[tokio::test]
async fn una_edicion_conserva_los_permisos_del_original_y_se_anota_como_edicion() {
    let Some(mut montaje) = montar(true).await else {
        return;
    };
    let original = montaje.remoto().join("nginx.conf");
    std::fs::write(&original, b"version vieja").unwrap();
    std::fs::set_permissions(&original, std::fs::Permissions::from_mode(0o640)).unwrap();
    let temporal = montaje.local().join("1-nginx.conf");
    std::fs::write(&temporal, b"version editada").unwrap();
    montaje.abrir_sftp().await;

    let id = encargar(
        &mut montaje,
        800,
        Encargo {
            direccion: Direccion::Subida,
            elementos: vec![elemento(&temporal, &original, Some(0o640))],
            borrar_al_terminar: Vec::new(),
            deliberacion: None,
            sincronizacion: None,
            etiqueta: Some(EtiquetaTransferencia::Edicion),
        },
    )
    .await
    .expect("se encola");
    let fila = montaje.esperar_terminada(id).await;
    assert_eq!(fila.estado, EstadoTransferencia::Hecha, "{fila:?}");
    assert_eq!(fila.etiqueta, Some(EtiquetaTransferencia::Edicion));
    assert_eq!(leer(&original), "version editada");
    assert_eq!(modo(&original), 0o640, "los permisos del original");

    assert!(hasta(|| !registro(&montaje, "transferencia").is_empty()).await);
    let (detalle, resultado) = registro(&montaje, "transferencia").remove(0);
    assert!(
        detalle.starts_with("subida hecha · edición · "),
        "{detalle}"
    );
    assert_eq!(resultado, "ok");
    assert!(
        !detalle.contains("version"),
        "el registro nunca guarda contenido"
    );
}
