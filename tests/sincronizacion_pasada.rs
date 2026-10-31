//! Pruebas de la pasada de sincronización del inventario y del subcomando
//! «magi sincronizar» (Fase 10): sin configurar, sin clave, primera
//! exportación, fichero ajeno con un host nuevo, formato más nuevo, fichero
//! manipulado, umbral de borrados e importación de huellas. También se prueba
//! el código de salida de la CLI sin TUI.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use magi::almacen::Almacen;
use magi::config::{Config, Rutas};
use magi::llavero::{LlaveroEnMemoria, LlaveroSecreto, CUENTA_SINCRONIZACION};
use magi::modelo::{DatosHost, Origen};
use magi::sincronizacion::cifrado::ClaveSincronizacion;
use magi::sincronizacion::formato::{Documento, EquipoDocumento, HuellaDocumento, LapidaDocumento};
use magi::sincronizacion::{
    self, carpeta, formato, EstadoPasada, OpcionesPasada, PoliticaBorrados, ResultadoPasada,
};

/// Un MAGI de prueba con su magi.db, su config y su llavero en memoria.
struct Entorno {
    _temporal: tempfile::TempDir,
    rutas: Rutas,
    almacen: Almacen,
    config: Config,
    carpeta: PathBuf,
    clave_texto: String,
    con_clave: bool,
    _llavero: LlaveroEnMemoria,
}

impl Entorno {
    fn nuevo(configurada: bool, con_clave: bool) -> Self {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let raiz = temporal.path();
        let rutas = Rutas {
            datos: raiz.join("datos"),
            config: raiz.join("config"),
            estado: raiz.join("estado"),
            hogar: raiz.join("hogar"),
            runtime: raiz.join("runtime"),
        };
        std::fs::create_dir_all(&rutas.hogar).expect("creando el hogar");
        let almacen = Almacen::abrir(&rutas.base_datos()).expect("creando magi.db");
        let carpeta = rutas.hogar.join("magi-sync");
        let mut config = Config::default();
        if configurada {
            std::fs::create_dir_all(&carpeta).expect("creando la carpeta compartida");
            config.sincronizacion.carpeta = Some(carpeta.to_string_lossy().to_string());
        }
        let clave = ClaveSincronizacion::generar();
        let clave_texto = clave.texto().to_string();
        let llavero = LlaveroEnMemoria::nuevo();
        if con_clave {
            llavero
                .guardar(CUENTA_SINCRONIZACION, &clave_texto)
                .expect("guardando la clave de sincronización");
        }
        Self {
            _temporal: temporal,
            rutas,
            almacen,
            config,
            carpeta,
            clave_texto,
            con_clave,
            _llavero: llavero,
        }
    }

    fn conexion(&self) -> &rusqlite::Connection {
        self.almacen.conexion()
    }

    fn claves(&self) -> Vec<ClaveSincronizacion> {
        if !self.con_clave {
            return Vec::new();
        }
        vec![ClaveSincronizacion::desde_texto(&self.clave_texto)
            .expect("la clave de prueba no se puede releer")]
    }

    fn clave(&self) -> ClaveSincronizacion {
        ClaveSincronizacion::desde_texto(&self.clave_texto)
            .expect("la clave de prueba no se puede releer")
    }

    fn pasada(&self, politica: PoliticaBorrados) -> anyhow::Result<ResultadoPasada> {
        sincronizacion::pasada(
            self.conexion(),
            &self.rutas,
            &self.config,
            &self.claves(),
            &OpcionesPasada::sin_trabajo_vivo(politica),
        )
    }

    fn host(&self, nombre: &str, direccion: &str) -> String {
        let id = self
            .almacen
            .crear_host(
                &DatosHost {
                    nombre: nombre.to_string(),
                    direccion: direccion.to_string(),
                    ..DatosHost::default()
                },
                Origen::Manual,
            )
            .expect("creando el host");
        self.conexion()
            .query_row("SELECT uuid FROM HOSTS WHERE id = ?1", [id], |fila| {
                fila.get(0)
            })
            .expect("leyendo el uuid del host")
    }

    fn uuid_este(&self) -> String {
        magi::almacen::equipos::este(self.conexion())
            .expect("leyendo este equipo")
            .uuid
    }

    fn fichero_propio(&self) -> PathBuf {
        self.carpeta
            .join(carpeta::nombre_fichero(&self.uuid_este()))
    }

    fn equipos(&self) -> i64 {
        self.conexion()
            .query_row("SELECT COUNT(*) FROM EQUIPOS", [], |fila| fila.get(0))
            .expect("contando equipos")
    }

    fn estado_equipo(&self, uuid: &str) -> String {
        magi::almacen::equipos::por_uuid(self.conexion(), uuid)
            .expect("buscando el equipo")
            .expect("el equipo no existe")
            .estado
    }
}

/// Documento de un equipo ajeno recién inventado, sin elementos.
fn documento_vacio(equipo: &str, nombre: &str) -> Documento {
    Documento {
        formato: formato::FORMATO,
        app: "1.1.0".to_string(),
        equipo: EquipoDocumento {
            uuid: equipo.to_string(),
            nombre: nombre.to_string(),
            plataforma: "macos".to_string(),
        },
        generado_en: "2026-11-05T09:12:44.120+01:00".to_string(),
        reloj_ms: 1,
        elementos: Vec::new(),
        lapidas: Vec::new(),
        huellas: Vec::new(),
        equipos_quitados: Vec::new(),
    }
}

/// Documento propio de otro MAGI con un host nuevo, como lo exportaría él.
fn documento_ajeno_con_host() -> Documento {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let raiz = temporal.path();
    let rutas = Rutas {
        datos: raiz.join("datos"),
        config: raiz.join("config"),
        estado: raiz.join("estado"),
        hogar: raiz.join("hogar"),
        runtime: raiz.join("runtime"),
    };
    std::fs::create_dir_all(&rutas.hogar).expect("creando el hogar ajeno");
    let almacen = Almacen::abrir(&rutas.base_datos()).expect("creando la magi.db ajena");
    almacen
        .crear_host(
            &DatosHost {
                nombre: "web".to_string(),
                direccion: "192.0.2.5".to_string(),
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .expect("creando el host ajeno");
    sincronizacion::documento_propio(almacen.conexion(), &rutas)
        .expect("exportando el documento ajeno")
}

/// Escribe un documento firmado y cifrado como fichero de su equipo.
fn escribir_documento(carpeta: &Path, clave: &ClaveSincronizacion, documento: &Documento) {
    let json = formato::serializar(documento).expect("serializando el documento");
    let firma = clave
        .firmar(json.as_bytes())
        .expect("firmando el documento");
    let plano = formato::componer(&json, &firma);
    let cifrado = clave.cifrar(&plano).expect("cifrando el documento");
    carpeta::escribir(carpeta, &documento.equipo.uuid, &cifrado).expect("escribiendo el fichero");
}

/// Escribe un documento con una cabecera de formato concreta. Con
/// `firma_valida` firma el JSON como haría un MAGI más nuevo (para probar el
/// rechazo por formato); sin ella deja una firma que no valida (para probar el
/// rechazo por firma). El orden de lectura comprueba la firma antes del
/// formato, así que cada caso necesita lo suyo.
fn escribir_documento_con_formato(
    carpeta: &Path,
    clave: &ClaveSincronizacion,
    documento: &Documento,
    numero_formato: u32,
    firma_valida: bool,
) {
    let json = formato::serializar(documento).expect("serializando el documento");
    let firma = if firma_valida {
        clave
            .firmar(json.as_bytes())
            .expect("firmando el documento")
    } else {
        "0".repeat(64)
    };
    let plano = format!(
        "{} {numero_formato}\nfirma: {firma}\n{json}\n",
        formato::CABECERA
    );
    let cifrado = clave
        .cifrar(plano.as_bytes())
        .expect("cifrando el documento");
    carpeta::escribir(carpeta, &documento.equipo.uuid, &cifrado).expect("escribiendo el fichero");
}

/// Documento ajeno con una lápida por cada host de la lista.
fn documento_con_lapidas(hosts: &[(String, String)]) -> Documento {
    let equipo = uuid::Uuid::new_v4().to_string();
    let ms = chrono::Local::now().timestamp_millis() + 10_000;
    let version = format!("{ms:013}.{equipo}");
    let vector: BTreeMap<String, i64> = [(equipo.clone(), ms)].into_iter().collect();
    let mut documento = documento_vacio(&equipo, "mac-mini");
    documento.reloj_ms = ms;
    documento.lapidas = hosts
        .iter()
        .map(|(uuid, nombre)| LapidaDocumento {
            tipo: "host".to_string(),
            uuid: uuid.clone(),
            nombre: nombre.clone(),
            version: version.clone(),
            vector: vector.clone(),
            borrado_en: "2026-11-05T09:00:00+01:00".to_string(),
        })
        .collect();
    documento
}

#[test]
fn sin_carpeta_la_pasada_dice_sin_configurar() {
    let entorno = Entorno::nuevo(false, false);
    let resultado = entorno
        .pasada(PoliticaBorrados::Preguntar)
        .expect("la pasada no debería fallar");
    assert_eq!(resultado.estado, EstadoPasada::SinConfigurar);
}

#[test]
fn con_carpeta_pero_sin_clave_la_pasada_dice_sin_clave() {
    let entorno = Entorno::nuevo(true, false);
    let resultado = entorno
        .pasada(PoliticaBorrados::Preguntar)
        .expect("la pasada no debería fallar");
    assert_eq!(resultado.estado, EstadoPasada::SinClave);
}

#[test]
fn la_primera_pasada_exporta_el_fichero_propio_y_la_segunda_no_lo_reescribe() {
    let entorno = Entorno::nuevo(true, true);
    let resultado = entorno
        .pasada(PoliticaBorrados::Preguntar)
        .expect("la primera pasada no debería fallar");
    assert_eq!(resultado.estado, EstadoPasada::AlDia);
    assert!(resultado.exportado, "la primera pasada debe exportar");
    assert_eq!(resultado.resumen, None, "no había nada que importar");
    assert!(entorno.fichero_propio().exists(), "falta el fichero propio");
    // El fichero se descifra y se entiende con la clave de la sincronización.
    let claro = entorno
        .clave()
        .descifrar(&std::fs::read(entorno.fichero_propio()).expect("leyendo el fichero propio"))
        .expect("el fichero propio no se descifra");
    let en_claro = formato::separar(&claro).expect("el fichero propio no tiene cabecera");
    let documento = formato::parsear(&en_claro.json).expect("el fichero propio no se parsea");
    assert_eq!(documento.equipo.uuid, entorno.uuid_este());

    let segunda = entorno
        .pasada(PoliticaBorrados::Preguntar)
        .expect("la segunda pasada no debería fallar");
    assert_eq!(segunda.estado, EstadoPasada::AlDia);
    assert!(!segunda.exportado, "no ha cambiado nada: no se reexporta");

    let equipos = entorno.equipos();
    assert_eq!(equipos, 1, "solo la fila de este equipo");
}

#[test]
fn las_huellas_de_un_fichero_ajeno_se_importan_y_se_anotan() {
    let entorno = Entorno::nuevo(true, true);
    let uuid_host = entorno.host("web", "192.0.2.5");
    let mut documento = documento_vacio(&uuid::Uuid::new_v4().to_string(), "mac-mini");
    documento.huellas = vec![HuellaDocumento {
        host: uuid_host,
        tipo: "ssh-ed25519".to_string(),
        clave: "AAAAC3NzaC1lZDI1NTE5AAAAIIssqddrV94bmHCxAEc2P4g22y1jImlSnbAMbOuQlbeb".to_string(),
        aceptada_en: "2026-11-01T10:00:00+01:00".to_string(),
    }];
    escribir_documento(&entorno.carpeta, &entorno.clave(), &documento);

    let resultado = entorno
        .pasada(PoliticaBorrados::Preguntar)
        .expect("la pasada no debería fallar");
    assert_eq!(resultado.estado, EstadoPasada::AlDia);
    let resumen = resultado.resumen.expect("la pasada debió importar algo");
    assert_eq!(resumen.huellas, 1);
    let known_hosts = entorno.rutas.fichero_known_hosts();
    let contenido = std::fs::read_to_string(&known_hosts).expect("leyendo known_hosts");
    assert!(contenido.contains("192.0.2.5 ssh-ed25519"), "{contenido}");
    let anotaciones: i64 = entorno
        .conexion()
        .query_row(
            "SELECT COUNT(*) FROM REGISTRO WHERE tipo = 'huella_sincronizada'",
            [],
            |fila| fila.get(0),
        )
        .expect("contando huellas sincronizadas");
    assert_eq!(anotaciones, 1);
}

#[test]
fn un_fichero_ajeno_valido_con_un_host_nuevo_se_importa() {
    let entorno = Entorno::nuevo(true, true);
    let documento = documento_ajeno_con_host();
    let equipo_ajeno = documento.equipo.uuid.clone();
    escribir_documento(&entorno.carpeta, &entorno.clave(), &documento);

    let resultado = entorno
        .pasada(PoliticaBorrados::Preguntar)
        .expect("la pasada no debería fallar");
    assert_eq!(resultado.estado, EstadoPasada::AlDia);
    let resumen = resultado.resumen.expect("la pasada debió importar algo");
    assert_eq!(resumen.equipos, 1);
    assert_eq!(resumen.creados, 1, "el host ajeno es nuevo en este equipo");
    let hosts = entorno.almacen.listar_hosts().expect("listando hosts");
    assert_eq!(hosts.len(), 1);
    assert_eq!(hosts[0].nombre, "web");
    assert_eq!(hosts[0].direccion, "192.0.2.5");
    assert_eq!(
        entorno.estado_equipo(&equipo_ajeno),
        magi::almacen::equipos::ESTADO_AL_DIA
    );
}

#[test]
fn un_fichero_con_formato_mas_nuevo_se_ignora_con_aviso() {
    let entorno = Entorno::nuevo(true, true);
    let equipo_ajeno = uuid::Uuid::new_v4().to_string();
    let documento = documento_vacio(&equipo_ajeno, "mac-mini");
    escribir_documento(&entorno.carpeta, &entorno.clave(), &documento);
    entorno
        .pasada(PoliticaBorrados::Preguntar)
        .expect("la primera pasada no debería fallar");

    escribir_documento_con_formato(&entorno.carpeta, &entorno.clave(), &documento, 2, true);
    let resultado = entorno
        .pasada(PoliticaBorrados::Preguntar)
        .expect("la pasada no debería fallar");
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
        entorno.estado_equipo(&equipo_ajeno),
        magi::almacen::equipos::ESTADO_FORMATO_NUEVO
    );
}

#[test]
fn un_fichero_manipulado_queda_danado_y_no_se_aplica() {
    let entorno = Entorno::nuevo(true, true);
    let documento = documento_ajeno_con_host();
    escribir_documento_con_formato(
        &entorno.carpeta,
        &entorno.clave(),
        &documento,
        formato::FORMATO,
        false,
    );

    let resultado = entorno
        .pasada(PoliticaBorrados::Preguntar)
        .expect("la pasada no debería fallar");
    assert_eq!(resultado.estado, EstadoPasada::AlDia);
    assert!(
        resultado
            .avisos
            .iter()
            .any(|aviso| aviso.contains("dañado") && aviso.contains("firma no válida")),
        "{:?}",
        resultado.avisos
    );
    // No se aplica nada: el host del fichero manipulado no llega al almacén.
    assert!(entorno
        .almacen
        .listar_hosts()
        .expect("listando hosts")
        .is_empty());
    assert_eq!(entorno.equipos(), 1, "solo la fila de este equipo");
}

#[test]
fn el_umbral_de_borrados_deja_la_pasada_pendiente_y_no_aplica_nada() {
    let entorno = Entorno::nuevo(true, true);
    let mut hosts = Vec::new();
    for indice in 0..6 {
        let nombre = format!("web-{indice}");
        let uuid = entorno.host(&nombre, &format!("192.0.2.{}", indice + 1));
        hosts.push((uuid, nombre));
    }
    let documento = documento_con_lapidas(&hosts);
    escribir_documento(&entorno.carpeta, &entorno.clave(), &documento);

    let resultado = entorno
        .pasada(PoliticaBorrados::Preguntar)
        .expect("la pasada no debería fallar");
    match &resultado.estado {
        EstadoPasada::DecisionPendiente(pendientes) => {
            assert_eq!(pendientes.hosts.len(), 6, "{:?}", pendientes.hosts);
            assert_eq!(pendientes.total_hosts, 6);
            assert!(
                pendientes.copia.exists(),
                "falta la copia de magi.db: {}",
                pendientes.copia.display()
            );
        }
        otro => panic!("se esperaba DecisionPendiente, llegó {otro:?}"),
    }
    // No se ha borrado nada ni se ha exportado nada.
    assert_eq!(
        entorno
            .almacen
            .listar_hosts()
            .expect("listando hosts")
            .len(),
        6
    );
    assert!(!entorno.fichero_propio().exists());
}

// ------------------------------------------------------------------- CLI

/// Ejecuta el binario con un entorno aislado en la raíz indicada.
fn ejecutar_magi(raiz: &Path, argumentos: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_magi"))
        .args(argumentos)
        .env_clear()
        .env("HOME", raiz)
        .env("XDG_DATA_HOME", raiz.join("datos"))
        .env("XDG_CONFIG_HOME", raiz.join("config"))
        .env("XDG_STATE_HOME", raiz.join("estado"))
        .env("XDG_RUNTIME_DIR", raiz.join("runtime"))
        .output()
        .expect("ejecutando el binario magi")
}

#[test]
fn magi_sincronizar_estado_sin_configurar_sale_con_cero() {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let salida = ejecutar_magi(temporal.path(), &["sincronizar", "estado"]);
    assert!(
        salida.status.success(),
        "«magi sincronizar estado» salió con {:?}: {}",
        salida.status.code(),
        String::from_utf8_lossy(&salida.stderr)
    );
    let texto = String::from_utf8_lossy(&salida.stdout);
    assert!(texto.contains("carpeta: (sin configurar)"), "{texto}");
}

#[test]
fn magi_sincronizar_sin_configurar_sale_con_uno_y_no_aplica_nada() {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let salida = ejecutar_magi(temporal.path(), &["sincronizar"]);
    assert_eq!(
        salida.status.code(),
        Some(1),
        "«magi sincronizar» sin configurar debe salir con 1"
    );
    let error = String::from_utf8_lossy(&salida.stderr);
    assert!(error.contains("no está configurada"), "{error}");
}
