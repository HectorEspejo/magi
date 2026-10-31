//! Prueba de extremo a extremo de la pasada de sincronización (Fase 10): dos
//! equipos con bases de datos distintas comparten una carpeta temporal, con la
//! misma clave, y se comprueba la importación, los conflictos, las lápidas, el
//! umbral de borrados y los ficheros dañados o de formato nuevo.

use std::path::{Path, PathBuf};

use magi::almacen::Almacen;
use magi::config::{Config, Rutas};
use magi::modelo::{DatosHost, Host, Origen};
use magi::sincronizacion::cifrado::ClaveSincronizacion;
use magi::sincronizacion::formato;
use magi::sincronizacion::{self, EstadoPasada, OpcionesPasada, PoliticaBorrados, ResultadoPasada};

struct Equipo {
    _temporal: tempfile::TempDir,
    rutas: Rutas,
    almacen: Almacen,
    config: Config,
    texto_clave: String,
}

fn equipo(carpeta: &Path, texto_clave: &str) -> Equipo {
    let temporal = tempfile::tempdir().unwrap();
    let raiz = temporal.path();
    let rutas = Rutas {
        datos: raiz.join("datos"),
        config: raiz.join("config"),
        estado: raiz.join("estado"),
        hogar: raiz.join("hogar"),
        runtime: raiz.join("runtime"),
    };
    std::fs::create_dir_all(rutas.dir_ssh()).unwrap();
    let mut config = Config::default();
    config.sincronizacion.carpeta = Some(carpeta.to_string_lossy().to_string());
    config.sincronizacion.copias = 5;
    // Umbrales altos para que las pruebas que no van de borrados masivos no
    // abran el diálogo de confirmación.
    config.sincronizacion.umbral_borrado_hosts = 1000;
    config.sincronizacion.umbral_borrado_pct = 1000;
    let almacen = Almacen::abrir(&rutas.base_datos()).unwrap();
    Equipo {
        _temporal: temporal,
        rutas,
        almacen,
        config,
        texto_clave: texto_clave.to_string(),
    }
}

impl Equipo {
    fn clave(&self) -> ClaveSincronizacion {
        ClaveSincronizacion::desde_texto(&self.texto_clave).unwrap()
    }

    fn pasada(&self, politica: PoliticaBorrados) -> ResultadoPasada {
        sincronizacion::pasada(
            self.almacen.conexion(),
            &self.rutas,
            &self.config,
            &[self.clave()],
            &OpcionesPasada::sin_trabajo_vivo(politica),
        )
        .unwrap()
    }

    fn hosts(&self) -> Vec<Host> {
        self.almacen.listar_hosts().unwrap()
    }

    fn host_por_nombre(&self, nombre: &str) -> Option<Host> {
        self.hosts().into_iter().find(|host| host.nombre == nombre)
    }

    fn crear_host(&self, nombre: &str, direccion: &str) -> i64 {
        self.almacen
            .crear_host(
                &DatosHost {
                    nombre: nombre.to_string(),
                    direccion: direccion.to_string(),
                    ..DatosHost::default()
                },
                Origen::Manual,
            )
            .unwrap()
    }

    /// Copia los datos editables del host y cambia la dirección.
    fn editar_host(&self, nombre: &str, direccion: &str) {
        let host = self.host_por_nombre(nombre).unwrap();
        self.almacen
            .actualizar_host(
                host.id,
                &DatosHost {
                    nombre: host.nombre.clone(),
                    grupo_id: host.grupo_id,
                    direccion: direccion.to_string(),
                    puerto: host.puerto,
                    usuario: host.usuario.clone(),
                    identidad_ref: host.identidad_ref.clone(),
                    salto_host_id: host.salto_host_id,
                    multiplexar: host.multiplexar,
                    keepalive_seg: host.keepalive_seg,
                    opciones_extra: host.opciones_extra.clone(),
                    servicios: host.servicios.clone(),
                    etiquetas: host.etiquetas.clone(),
                },
            )
            .unwrap();
    }
}

fn carpeta_compartida() -> (tempfile::TempDir, PathBuf) {
    let temporal = tempfile::tempdir().unwrap();
    let carpeta = temporal.path().join("sync");
    std::fs::create_dir_all(&carpeta).unwrap();
    (temporal, carpeta)
}

fn ficheros_propios(carpeta: &Path) -> Vec<String> {
    sincronizacion::carpeta::listar(carpeta)
        .unwrap()
        .into_iter()
        .map(|fichero| fichero.uuid)
        .collect()
}

#[test]
fn primera_pasada_exporta_y_segunda_no_reexporta_sin_cambios() {
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = ClaveSincronizacion::generar().texto().to_string();
    let a = equipo(&carpeta, &clave);
    a.crear_host("web", "192.0.2.5");

    let resultado = a.pasada(PoliticaBorrados::Preguntar);
    assert_eq!(resultado.estado, EstadoPasada::AlDia);
    assert!(resultado.exportado, "la primera pasada escribe el fichero");
    assert_eq!(ficheros_propios(&carpeta).len(), 1);

    let resultado = a.pasada(PoliticaBorrados::Preguntar);
    assert_eq!(resultado.estado, EstadoPasada::AlDia);
    assert!(!resultado.exportado, "sin cambios no se reexporta");
}

#[test]
fn otro_equipo_importa_el_inventario_y_los_cambios_posteriores() {
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = ClaveSincronizacion::generar().texto().to_string();
    let a = equipo(&carpeta, &clave);
    let b = equipo(&carpeta, &clave);
    a.crear_host("web", "192.0.2.5");
    a.pasada(PoliticaBorrados::Preguntar);

    let resultado = b.pasada(PoliticaBorrados::Preguntar);
    assert_eq!(resultado.estado, EstadoPasada::AlDia);
    let host = b.host_por_nombre("web").expect("B importa el host de A");
    assert_eq!(host.direccion, "192.0.2.5");
    assert!(resultado.resumen.is_some());
    assert_eq!(
        ficheros_propios(&carpeta).len(),
        2,
        "cada equipo su fichero"
    );

    // Edición secuencial: A cambia, B importa y no hay conflicto.
    a.editar_host("web", "192.0.2.6");
    a.pasada(PoliticaBorrados::Preguntar);
    let resultado = b.pasada(PoliticaBorrados::Preguntar);
    assert_eq!(
        b.host_por_nombre("web").unwrap().direccion,
        "192.0.2.6",
        "la edición posterior de A llega a B"
    );
    assert!(resultado.conflictos_pendientes == 0, "sin conflicto");
}

#[test]
fn dos_ediciones_a_la_vez_ganan_la_mas_reciente_y_la_perdedora_queda_como_conflicto() {
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = ClaveSincronizacion::generar().texto().to_string();
    let a = equipo(&carpeta, &clave);
    let b = equipo(&carpeta, &clave);
    a.crear_host("web", "192.0.2.5");
    a.pasada(PoliticaBorrados::Preguntar);
    b.pasada(PoliticaBorrados::Preguntar);
    assert_eq!(b.host_por_nombre("web").unwrap().direccion, "192.0.2.5");

    // A y B editan a la vez antes de leer lo del otro. B exporta primero su
    // edición y A importa el fichero de B; como A perdió, queda el conflicto.
    a.editar_host("web", "192.0.2.10");
    std::thread::sleep(std::time::Duration::from_millis(5));
    b.editar_host("web", "192.0.2.20");
    b.pasada(PoliticaBorrados::Preguntar);
    let resultado = a.pasada(PoliticaBorrados::Preguntar);
    assert_eq!(resultado.estado, EstadoPasada::AlDia);
    assert_eq!(
        a.host_por_nombre("web").unwrap().direccion,
        "192.0.2.20",
        "gana la versión más reciente (B)"
    );
    let conflictos = magi::almacen::conflictos::listar_pendientes(a.almacen.conexion()).unwrap();
    assert_eq!(conflictos.len(), 1, "A perdió su versión: conflicto");
    assert_eq!(conflictos[0].tipo, "elemento");
    let perdedora = conflictos[0]
        .version_perdedora_json
        .as_deref()
        .expect("contenido de la versión perdedora");
    assert!(perdedora.contains("192.0.2.10"), "{perdedora}");

    // Y B acaba viendo lo de A (su versión ganadora ya lo domina).
    let resultado = b.pasada(PoliticaBorrados::Preguntar);
    assert_eq!(resultado.estado, EstadoPasada::AlDia);
    assert_eq!(b.host_por_nombre("web").unwrap().direccion, "192.0.2.20");
}

#[test]
fn un_borrado_delega_en_lapida_y_se_propaga() {
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = ClaveSincronizacion::generar().texto().to_string();
    let a = equipo(&carpeta, &clave);
    let b = equipo(&carpeta, &clave);
    a.crear_host("web", "192.0.2.5");
    a.pasada(PoliticaBorrados::Preguntar);
    b.pasada(PoliticaBorrados::Preguntar);
    assert!(b.host_por_nombre("web").is_some());

    let id = a.host_por_nombre("web").unwrap().id;
    a.almacen.borrar_host(id).unwrap();
    let resultado = a.pasada(PoliticaBorrados::Preguntar);
    assert_eq!(resultado.estado, EstadoPasada::AlDia);
    let resultado = b.pasada(PoliticaBorrados::Preguntar);
    assert_eq!(resultado.estado, EstadoPasada::AlDia);
    assert!(
        b.host_por_nombre("web").is_none(),
        "el borrado llega a B como lápida"
    );
    let lapida = magi::almacen::lapidas::listar(b.almacen.conexion())
        .unwrap()
        .into_iter()
        .find(|lapida| lapida.tipo == "host")
        .expect("B guarda la lápida del host");
    assert_eq!(lapida.nombre, "web");
}

#[test]
fn los_borrados_masivos_piden_confirmacion_y_no_aplican_nada() {
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = ClaveSincronizacion::generar().texto().to_string();
    let a = equipo(&carpeta, &clave);
    let b = equipo(&carpeta, &clave);
    let mut a = a;
    a.config.sincronizacion.umbral_borrado_hosts = 5;
    a.config.sincronizacion.umbral_borrado_pct = 20;
    for indice in 0..6 {
        b.crear_host(&format!("web-{indice}"), "192.0.2.5");
    }
    b.pasada(PoliticaBorrados::Preguntar);
    a.pasada(PoliticaBorrados::Preguntar);
    assert_eq!(a.hosts().len(), 6, "A importa los seis hosts");

    for host in b.hosts() {
        b.almacen.borrar_host(host.id).unwrap();
    }
    b.pasada(PoliticaBorrados::Preguntar);
    let resultado = a.pasada(PoliticaBorrados::Preguntar);
    let EstadoPasada::DecisionPendiente(borrados) = resultado.estado else {
        panic!("debe pedir confirmación: {:?}", resultado.estado);
    };
    assert_eq!(borrados.hosts.len(), 6);
    assert!(borrados.copia.exists(), "la copia existe antes de decidir");
    assert_eq!(a.hosts().len(), 6, "no se ha borrado nada");

    let resultado = a.pasada(PoliticaBorrados::Aplicar);
    assert_eq!(resultado.estado, EstadoPasada::AlDia);
    assert!(a.hosts().is_empty(), "al aplicar, se borran");
}

#[test]
fn un_fichero_de_formato_nuevo_o_danado_no_se_aplica() {
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = ClaveSincronizacion::generar().texto().to_string();
    let a = equipo(&carpeta, &clave);
    let clave_obj = a.clave();

    // Equipo conocido con formato 2: se marca formato_nuevo y no se importa.
    let uuid_nuevo = uuid::Uuid::new_v4().to_string();
    magi::almacen::equipos::crear(a.almacen.conexion(), &uuid_nuevo, "futuro", "linux").unwrap();
    let plano = b"MAGI-SINCRONIZACION 2\nfirma: 0000000000000000000000000000000000000000000000000000000000000000\n{}\n";
    let cifrado = clave_obj.cifrar(plano).unwrap();
    sincronizacion::carpeta::escribir(&carpeta, &uuid_nuevo, &cifrado).unwrap();
    let resultado = a.pasada(PoliticaBorrados::Preguntar);
    assert_eq!(resultado.estado, EstadoPasada::AlDia);
    assert!(
        resultado
            .avisos
            .iter()
            .any(|aviso| aviso.contains("más nueva")),
        "{:?}",
        resultado.avisos
    );
    assert_eq!(
        magi::almacen::equipos::por_uuid(a.almacen.conexion(), &uuid_nuevo)
            .unwrap()
            .unwrap()
            .estado,
        magi::almacen::equipos::ESTADO_FORMATO_NUEVO
    );

    // Fichero con firma que no cuadra: se marca dañado y no se aplica.
    let uuid_danado = uuid::Uuid::new_v4().to_string();
    magi::almacen::equipos::crear(a.almacen.conexion(), &uuid_danado, "roto", "linux").unwrap();
    let documento = format!(
        "{{\"formato\":1,\"app\":\"1.1.0\",\"equipo\":{{\"uuid\":\"{uuid_danado}\",\"nombre\":\"roto\",\"plataforma\":\"linux\"}},\"generado_en\":\"2026-11-05T00:00:00+01:00\",\"reloj_ms\":1,\"elementos\":[],\"lapidas\":[],\"huellas\":[],\"equipos_quitados\":[]}}"
    );
    let firma_mala = "0".repeat(64);
    let plano = formato::componer(&documento, &firma_mala);
    let cifrado = clave_obj.cifrar(&plano).unwrap();
    sincronizacion::carpeta::escribir(&carpeta, &uuid_danado, &cifrado).unwrap();
    let resultado = a.pasada(PoliticaBorrados::Preguntar);
    assert_eq!(resultado.estado, EstadoPasada::AlDia);
    assert!(
        resultado
            .avisos
            .iter()
            .any(|aviso| aviso.contains("dañado")),
        "{:?}",
        resultado.avisos
    );
    assert_eq!(
        magi::almacen::equipos::por_uuid(a.almacen.conexion(), &uuid_danado)
            .unwrap()
            .unwrap()
            .estado,
        magi::almacen::equipos::ESTADO_DANADO
    );
}

#[test]
fn un_equipo_ajeno_que_deja_de_abrirse_detiene_la_sincronizacion() {
    let (_temporal, carpeta) = carpeta_compartida();
    let clave_buena = ClaveSincronizacion::generar().texto().to_string();
    let a = equipo(&carpeta, &clave_buena);
    a.pasada(PoliticaBorrados::Preguntar);
    assert_eq!(ficheros_propios(&carpeta).len(), 1);

    // Un equipo conocido (fila en EQUIPOS) cuyo fichero está cifrado con otra
    // clave: la pasada se detiene pidiendo la clave nueva y no exporta.
    let uuid_ajeno = uuid::Uuid::new_v4().to_string();
    magi::almacen::equipos::crear(a.almacen.conexion(), &uuid_ajeno, "ajeno", "linux").unwrap();
    let clave_ajena = ClaveSincronizacion::generar();
    let cifrado = clave_ajena.cifrar(b"contenido que no abre").unwrap();
    sincronizacion::carpeta::escribir(&carpeta, &uuid_ajeno, &cifrado).unwrap();
    let resultado = a.pasada(PoliticaBorrados::Preguntar);
    assert!(
        matches!(resultado.estado, EstadoPasada::EsperandoClave(_)),
        "{:?}",
        resultado.estado
    );
    assert!(!resultado.exportado, "con clave pendiente no se exporta");
    assert_eq!(
        magi::almacen::equipos::por_uuid(a.almacen.conexion(), &uuid_ajeno)
            .unwrap()
            .unwrap()
            .estado,
        magi::almacen::equipos::ESTADO_ILEGIBLE
    );
}
#[test]
fn quitar_un_equipo_rota_la_clave_y_el_quitado_puede_reincorporarse() {
    use magi::almacen::equipos as equipos_bd;
    use magi::llavero::{LlaveroEnMemoria, LlaveroSecreto, CUENTA_SINCRONIZACION};
    use magi::sincronizacion::equipos as equipos_sync;

    let (_temporal, carpeta) = carpeta_compartida();
    let clave_original = ClaveSincronizacion::generar().texto().to_string();
    let mut a = equipo(&carpeta, &clave_original);
    let mut b = equipo(&carpeta, &clave_original);
    a.crear_host("web", "192.0.2.5");
    a.pasada(PoliticaBorrados::Preguntar);
    b.pasada(PoliticaBorrados::Preguntar);
    assert_eq!(b.host_por_nombre("web").unwrap().direccion, "192.0.2.5");

    // A conoce a B y lo quita: se marca quitado, se borra su fichero y se rota
    // la clave.
    let uuid_b = equipos_bd::este(b.almacen.conexion()).unwrap().uuid;
    equipos_bd::crear(a.almacen.conexion(), &uuid_b, "equipo-b", "linux").unwrap();
    let llavero_a = LlaveroEnMemoria::nuevo();
    llavero_a
        .guardar(CUENTA_SINCRONIZACION, &clave_original)
        .unwrap();
    let nueva = equipos_sync::quitar(
        a.almacen.conexion(),
        &a.rutas,
        &mut a.config,
        &uuid_b,
        &llavero_a,
    )
    .unwrap();
    a.texto_clave = nueva.texto().to_string();
    let resultado = a.pasada(PoliticaBorrados::Preguntar);
    assert_eq!(resultado.estado, EstadoPasada::AlDia);
    assert!(resultado.exportado);
    assert!(!carpeta
        .join(sincronizacion::carpeta::nombre_fichero(&uuid_b))
        .exists());

    // B, con la clave vieja, ya no abre el fichero de A.
    let resultado = b.pasada(PoliticaBorrados::Preguntar);
    assert!(
        matches!(resultado.estado, EstadoPasada::EsperandoClave(_)),
        "{:?}",
        resultado.estado
    );

    // B introduce la clave nueva (unir) y su pasada lo detecta quitado.
    let llavero_b = LlaveroEnMemoria::nuevo();
    let texto_nuevo = a.texto_clave.clone();
    equipos_sync::unir(
        b.almacen.conexion(),
        &b.rutas,
        &carpeta,
        &texto_nuevo,
        &mut b.config,
        &llavero_b,
    )
    .unwrap();
    b.texto_clave = texto_nuevo;
    let resultado = b.pasada(PoliticaBorrados::Preguntar);
    assert_eq!(resultado.estado, EstadoPasada::Quitado);

    // Se reincorpora como equipo nuevo: otro uuid, exporta su fichero y A lo
    // acepta como un equipo más.
    let uuid_nuevo = equipos_sync::reunir(b.almacen.conexion(), &b.rutas, &mut b.config).unwrap();
    assert_ne!(uuid_nuevo, uuid_b);
    let resultado = b.pasada(PoliticaBorrados::Preguntar);
    assert_eq!(resultado.estado, EstadoPasada::AlDia);
    assert!(resultado.exportado);
    let resultado = a.pasada(PoliticaBorrados::Preguntar);
    assert_eq!(resultado.estado, EstadoPasada::AlDia);
    assert!(
        equipos_bd::por_uuid(a.almacen.conexion(), &uuid_nuevo)
            .unwrap()
            .is_some(),
        "A ve al equipo reincorporado"
    );
    assert_eq!(a.host_por_nombre("web").unwrap().direccion, "192.0.2.5");
}
