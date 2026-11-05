//! Pruebas del `formato` 2 de sincronización con escritorios (Fase 12, §7.12):
//! un inventario sin escritorios se exporta en formato 1 (lo lee MAGI 1.1), con
//! escritorios sube a formato 2, un MAGI 1.2 lee los dos, un formato mayor se
//! rechaza como `formato_nuevo` con aviso y un host con un tipo que no se puede
//! validar no se aplica y deja un conflicto de elemento.
//!
//! Sin red y con la carpeta compartida en un directorio temporal.

use std::path::PathBuf;

use serde_json::json;

use magi_nucleo::almacen::conflictos;
use magi_nucleo::almacen::{equipos as equipos_bd, Almacen};
use magi_nucleo::configuracion::SeccionSincronizacion;
use magi_nucleo::escritorio::{OpcionesEscritorio, TamanoEscritorio, TipoHost};
use magi_nucleo::modelo::{DatosHost, Origen};
use magi_nucleo::rutas::{Equipo, Rutas};
use magi_nucleo::sincronizacion::cifrado::ClaveSincronizacion;
use magi_nucleo::sincronizacion::formato::{
    self, Documento, ElementoDocumento, EquipoDocumento, FORMATO_1, FORMATO_MAXIMO,
};
use magi_nucleo::sincronizacion::{
    self, carpeta, EstadoPasada, OpcionesPasada, PoliticaBorrados, ResultadoPasada,
};

const EQUIPO_AJENO: &str = "eeeeeeee-1111-4111-8111-eeeeeeeeeeee";
const UUID_SSH: &str = "aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa";
const UUID_RDP: &str = "bbbbbbbb-2222-4222-8222-bbbbbbbbbbbb";
const UUID_VNC: &str = "cccccccc-3333-4333-8333-cccccccccccc";
const UUID_VOXL: &str = "dddddddd-4444-4444-8444-dddddddddddd";

/// Un MAGI de prueba con su magi.db, sus rutas y su carpeta compartida.
struct Entorno {
    _temporal: tempfile::TempDir,
    rutas: Rutas,
    almacen: Almacen,
    ajustes: SeccionSincronizacion,
    carpeta: PathBuf,
    texto_clave: String,
}

impl Entorno {
    fn nuevo() -> Self {
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
        let almacen = Almacen::abrir(
            &rutas.base_datos(),
            &Equipo::nuevo("portatil-prueba", "linux"),
            "1.2.0",
        )
        .expect("creando magi.db");
        let carpeta = rutas.hogar.join("magi-sync");
        std::fs::create_dir_all(&carpeta).expect("creando la carpeta compartida");
        let ajustes = SeccionSincronizacion {
            carpeta: Some(carpeta.to_string_lossy().to_string()),
            umbral_borrado_hosts: 1000,
            umbral_borrado_pct: 1000,
            ..SeccionSincronizacion::default()
        };
        let clave = ClaveSincronizacion::generar();
        let texto_clave = clave.texto().to_string();
        Self {
            _temporal: temporal,
            rutas,
            almacen,
            ajustes,
            carpeta,
            texto_clave,
        }
    }

    fn clave(&self) -> ClaveSincronizacion {
        ClaveSincronizacion::desde_texto(&self.texto_clave).expect("clave de prueba ilegible")
    }

    fn conexion(&self) -> &rusqlite::Connection {
        self.almacen.conexion()
    }

    fn pasada(&self) -> ResultadoPasada {
        sincronizacion::pasada(
            self.conexion(),
            &self.rutas,
            &self.ajustes,
            "1.2.0",
            &[self.clave()],
            &OpcionesPasada::sin_trabajo_vivo(PoliticaBorrados::Preguntar),
        )
        .expect("la pasada no debería fallar")
    }

    fn crear_host(
        &self,
        nombre: &str,
        tipo: TipoHost,
        direccion: &str,
        puerto: u16,
        escritorio_dominio: Option<&str>,
        escritorio_opciones: OpcionesEscritorio,
    ) -> i64 {
        self.almacen
            .crear_host(
                &DatosHost {
                    nombre: nombre.to_string(),
                    direccion: direccion.to_string(),
                    puerto,
                    tipo,
                    escritorio_dominio: escritorio_dominio.map(str::to_string),
                    escritorio_opciones,
                    usuario: if tipo == TipoHost::Rdp {
                        Some("operador".to_string())
                    } else {
                        None
                    },
                    ..DatosHost::default()
                },
                Origen::Manual,
            )
            .expect("creando el host")
    }

    fn uuid_este(&self) -> String {
        equipos_bd::este(self.conexion()).expect("este equipo").uuid
    }

    fn fichero_propio(&self) -> PathBuf {
        self.carpeta
            .join(carpeta::nombre_fichero(&self.uuid_este()))
    }

    fn estado_equipo(&self, uuid: &str) -> String {
        equipos_bd::por_uuid(self.conexion(), uuid)
            .expect("buscando el equipo")
            .expect("el equipo no existe")
            .estado
    }
}

/// Contenido en claro del fichero propio (descifrado y separado).
fn contenido_propio(entorno: &Entorno) -> formato::EnClaro {
    let bytes = std::fs::read(entorno.fichero_propio()).expect("leyendo el fichero propio");
    let plano = entorno
        .clave()
        .descifrar(&bytes)
        .expect("descifrando el fichero propio");
    formato::separar(&plano).expect("separando el fichero propio")
}

/// Datos de un host del fichero ajeno, sin pasar por `DatosHost` para poder
/// mandar exactamente lo que manda un MAGI 1.1 o un fichero con campos raros.
fn datos_host(
    nombre: &str,
    direccion: &str,
    puerto: u16,
    tipo: Option<&str>,
    extra: serde_json::Value,
) -> serde_json::Value {
    let mut datos = json!({
        "nombre": nombre,
        "direccion": direccion,
        "puerto": puerto,
        "usuario": null,
        "identidad_ref": null,
        "salto": null,
        "grupo": null,
        "multiplexar": false,
        "keepalive_seg": 30,
        "opciones_extra": "",
        "servicios": "",
        "origen": "manual",
        "etiquetas": [],
        "verificaciones": null,
        "snippet_al_conectar": null,
    });
    let objeto = datos.as_object_mut().expect("datos de host es un objeto");
    if let Some(tipo) = tipo {
        objeto.insert(
            "tipo".to_string(),
            serde_json::Value::String(tipo.to_string()),
        );
    }
    if let Some(extra) = extra.as_object() {
        for (clave, valor) in extra {
            objeto.insert(clave.clone(), valor.clone());
        }
    }
    datos
}

/// Documento ajeno con versiones posteriores a las de este equipo.
fn documento_ajeno(formato: u32, mut elementos: Vec<ElementoDocumento>) -> Documento {
    let ms = chrono::Local::now().timestamp_millis() + 10_000;
    for elemento in &mut elementos {
        elemento.version = format!("{ms:013}.{EQUIPO_AJENO}");
        elemento.vector = [(EQUIPO_AJENO.to_string(), ms)].into_iter().collect();
    }
    Documento {
        formato,
        app: if formato == FORMATO_1 {
            "1.1.0".to_string()
        } else {
            "1.2.0".to_string()
        },
        equipo: EquipoDocumento {
            uuid: EQUIPO_AJENO.to_string(),
            nombre: "escritorio-ajeno".to_string(),
            plataforma: "macos".to_string(),
        },
        generado_en: "2026-11-05T09:12:44.120+01:00".to_string(),
        reloj_ms: ms,
        elementos,
        lapidas: Vec::new(),
        huellas: Vec::new(),
        equipos_quitados: Vec::new(),
    }
}

fn elemento_host(uuid: &str, datos: serde_json::Value) -> ElementoDocumento {
    ElementoDocumento {
        tipo: "host".to_string(),
        uuid: uuid.to_string(),
        version: String::new(),
        vector: Default::default(),
        datos,
    }
}

/// Escribe un documento ajeno firmado y cifrado con la clave de la prueba.
fn escribir_documento(entorno: &Entorno, documento: &Documento) {
    escribir_documento_con_cabecera(entorno, documento, documento.formato);
}

/// Escribe el documento con una cabecera de formato distinta de la del JSON
/// (para probar el rechazo de un formato más nuevo).
fn escribir_documento_con_cabecera(entorno: &Entorno, documento: &Documento, cabecera: u32) {
    let json = formato::serializar(documento).expect("serializando el documento");
    let clave = entorno.clave();
    let firma = clave
        .firmar(json.as_bytes())
        .expect("firmando el documento");
    let plano = formato::componer(&json, &firma, cabecera);
    let cifrado = clave.cifrar(&plano).expect("cifrando el documento");
    carpeta::escribir(&entorno.carpeta, &documento.equipo.uuid, &cifrado)
        .expect("escribiendo el fichero del otro equipo");
}

#[test]
fn sin_escritorios_se_exporta_formato_1_y_lo_lee_magi_1_1() {
    let entorno = Entorno::nuevo();
    entorno.crear_host(
        "web-01",
        TipoHost::Ssh,
        "192.0.2.10",
        22,
        None,
        OpcionesEscritorio::default(),
    );
    let resultado = entorno.pasada();
    assert!(resultado.exportado, "la primera pasada exporta el fichero");

    let claro = contenido_propio(&entorno);
    let documento = formato::parsear(&claro.json).expect("parseando el fichero propio");
    // Cabecera y JSON dicen lo mismo, y es el formato 1 (lo que lee MAGI 1.1).
    assert_eq!(claro.formato, FORMATO_1);
    assert_eq!(documento.formato, FORMATO_1);
    // MAGI 1.1 rechaza cualquier formato mayor que el 1 (aquí no se puede
    // ejecutar); por eso, sin escritorios, se sigue escribiendo el 1 (D220).

    // El fichero del formato 1 no lleva ningún escritorio.
    for elemento in &documento.elementos {
        if elemento.tipo == "host" {
            let tipo = elemento
                .datos
                .get("tipo")
                .and_then(serde_json::Value::as_str);
            assert!(
                tipo.is_none() || tipo == Some("ssh"),
                "un inventario sin escritorios es todo SSH: {tipo:?}"
            );
        }
    }
}

#[test]
fn con_escritorios_se_exporta_formato_2() {
    let entorno = Entorno::nuevo();
    entorno.crear_host(
        "web-01",
        TipoHost::Ssh,
        "192.0.2.10",
        22,
        None,
        OpcionesEscritorio::default(),
    );
    let opciones = OpcionesEscritorio {
        tamano: TamanoEscritorio::Fijo {
            ancho: 1920,
            alto: 1080,
        },
        teclado: Some("es-ES".to_string()),
        ..OpcionesEscritorio::default()
    };
    let id_rdp = entorno.crear_host(
        "win-01",
        TipoHost::Rdp,
        "192.0.2.30",
        3389,
        Some("EJEMPLO"),
        opciones,
    );
    // La huella del certificado RDP es propia del equipo y no debe viajar.
    entorno
        .conexion()
        .execute(
            "UPDATE HOSTS SET huella_escritorio = 'SHA256:secreta' WHERE id = ?1",
            rusqlite::params![id_rdp],
        )
        .expect("fijando la huella del escritorio");

    entorno.pasada();
    let claro = contenido_propio(&entorno);
    let documento = formato::parsear(&claro.json).expect("parseando el fichero propio");
    assert_eq!(claro.formato, FORMATO_MAXIMO);
    assert_eq!(documento.formato, FORMATO_MAXIMO);
    assert!(
        !claro.json.contains("huella_escritorio") && !claro.json.contains("SHA256:secreta"),
        "la huella_escritorio no viaja: {}",
        claro.json
    );
    let uuid_rdp: String = entorno
        .conexion()
        .query_row(
            "SELECT uuid FROM HOSTS WHERE id = ?1",
            rusqlite::params![id_rdp],
            |fila| fila.get(0),
        )
        .expect("uuid del host rdp");
    let rdp = documento
        .elementos
        .iter()
        .find(|elemento| elemento.uuid == uuid_rdp)
        .expect("el host rdp está en el fichero");
    assert_eq!(rdp.datos["tipo"], "rdp");
    assert_eq!(rdp.datos["escritorio_dominio"], "EJEMPLO");
    assert_eq!(rdp.datos["escritorio_opciones"]["tamano"], "1920x1080");
    assert_eq!(rdp.datos["escritorio_opciones"]["teclado"], "es-ES");
}

#[test]
fn el_formato_1_se_importa_con_todos_los_hosts_ssh() {
    let entorno = Entorno::nuevo();
    // Fichero como el que escribe MAGI 1.1: sin `tipo` ni campos de escritorio.
    let documento = documento_ajeno(
        FORMATO_1,
        vec![
            elemento_host(
                UUID_SSH,
                datos_host("web-01", "192.0.2.10", 22, None, json!({})),
            ),
            elemento_host(
                UUID_RDP,
                datos_host("win-01", "192.0.2.30", 3389, None, json!({})),
            ),
        ],
    );
    escribir_documento(&entorno, &documento);

    let resultado = entorno.pasada();
    assert_eq!(resultado.estado, EstadoPasada::AlDia);
    let hosts = entorno.almacen.listar_hosts().expect("listando hosts");
    assert_eq!(hosts.len(), 2);
    for host in &hosts {
        // En el formato 1 no hay escritorios: todos los hosts son SSH.
        assert_eq!(
            host.tipo,
            TipoHost::Ssh,
            "«{}» debería ser SSH",
            host.nombre
        );
        assert_eq!(host.escritorio_dominio, None);
        assert_eq!(host.escritorio_opciones, OpcionesEscritorio::default());
    }
    assert_eq!(
        entorno.estado_equipo(EQUIPO_AJENO),
        equipos_bd::ESTADO_AL_DIA
    );
}

#[test]
fn el_formato_2_se_importa_con_los_escritorios_y_descarta_la_huella() {
    let entorno = Entorno::nuevo();
    let ssh = datos_host("bastion", "192.0.2.1", 22, Some("ssh"), json!({}));
    let rdp = datos_host(
        "win-01",
        "192.0.2.30",
        3389,
        Some("rdp"),
        json!({
            "usuario": "operador",
            "escritorio_dominio": "EJEMPLO",
            "escritorio_opciones": {"tamano": "1920x1080", "portapapeles": true,
                                    "teclado": "es-ES"},
            // Aunque venga, no viaja ni se aplica.
            "huella_escritorio": "SHA256:del-otro-equipo",
        }),
    );
    let vnc = datos_host(
        "vm-01",
        "192.0.2.40",
        5900,
        Some("vnc"),
        json!({
            "escritorio_opciones": {"tamano": "ventana", "portapapeles": false},
        }),
    );
    let mut voxl = datos_host(
        "linux-01",
        "127.0.0.1",
        6901,
        Some("voxl"),
        json!({
            "escritorio_opciones": {"tamano": "ventana", "portapapeles": true,
                                    "pantalla": ":1", "programa": "voxl"},
        }),
    );
    voxl["salto"] = serde_json::Value::String(UUID_SSH.to_string());
    let documento = documento_ajeno(
        FORMATO_MAXIMO,
        vec![
            elemento_host(UUID_SSH, ssh),
            elemento_host(UUID_RDP, rdp),
            elemento_host(UUID_VNC, vnc),
            elemento_host(UUID_VOXL, voxl),
        ],
    );
    escribir_documento(&entorno, &documento);

    let resultado = entorno.pasada();
    assert_eq!(resultado.estado, EstadoPasada::AlDia);
    let hosts = entorno.almacen.listar_hosts().expect("listando hosts");
    assert_eq!(hosts.len(), 4, "los cuatro hosts del formato 2: {hosts:?}");
    let por_nombre = |nombre: &str| {
        hosts
            .iter()
            .find(|host| host.nombre == nombre)
            .unwrap_or_else(|| panic!("falta el host «{nombre}»: {hosts:?}"))
    };

    let ssh = por_nombre("bastion");
    assert_eq!(ssh.tipo, TipoHost::Ssh);

    let rdp = por_nombre("win-01");
    assert_eq!(rdp.tipo, TipoHost::Rdp);
    assert_eq!(rdp.escritorio_dominio.as_deref(), Some("EJEMPLO"));
    assert_eq!(
        rdp.escritorio_opciones.tamano,
        TamanoEscritorio::Fijo {
            ancho: 1920,
            alto: 1080
        }
    );
    assert_eq!(rdp.escritorio_opciones.teclado.as_deref(), Some("es-ES"));
    // La huella del certificado que venía en el fichero se descarta: la guarda
    // el equipo que acepta el certificado, no se importa.
    assert_eq!(rdp.huella_escritorio, None);

    let vnc = por_nombre("vm-01");
    assert_eq!(vnc.tipo, TipoHost::Vnc);
    assert!(!vnc.escritorio_opciones.portapapeles);

    let voxl = por_nombre("linux-01");
    assert_eq!(voxl.tipo, TipoHost::Voxl);
    assert_eq!(voxl.escritorio_opciones.pantalla.as_deref(), Some(":1"));
    assert_eq!(voxl.escritorio_opciones.programa.as_deref(), Some("voxl"));
    assert_eq!(
        voxl.salto_nombre.as_deref(),
        Some("bastion"),
        "«a través de» se resuelve por uuid"
    );
}

#[test]
fn un_formato_mayor_que_el_maximo_queda_formato_nuevo_y_avisa() {
    let entorno = Entorno::nuevo();
    // Equipo conocido que embarca un formato futuro (3).
    let documento = documento_ajeno(
        FORMATO_1,
        vec![elemento_host(
            UUID_SSH,
            datos_host("web-01", "192.0.2.10", 22, None, json!({})),
        )],
    );
    escribir_documento(&entorno, &documento);
    entorno.pasada();

    escribir_documento_con_cabecera(&entorno, &documento, FORMATO_MAXIMO + 1);
    let resultado = entorno.pasada();
    assert_eq!(resultado.estado, EstadoPasada::AlDia);
    assert!(
        resultado
            .avisos
            .iter()
            .any(|aviso| aviso.contains("actualiza")),
        "aviso legible de actualizar MAGI: {:?}",
        resultado.avisos
    );
    assert_eq!(
        entorno.estado_equipo(EQUIPO_AJENO),
        equipos_bd::ESTADO_FORMATO_NUEVO
    );
    // Lo que había importado antes sigue ahí y no se malinterpreta nada.
    assert_eq!(entorno.almacen.listar_hosts().expect("hosts").len(), 1);
}

#[test]
fn un_host_con_tipo_o_opciones_invalidos_no_se_aplica_y_deja_conflicto() {
    let entorno = Entorno::nuevo();
    let valido = datos_host("web-01", "192.0.2.10", 22, Some("ssh"), json!({}));
    let tipo_raro = datos_host("telnet-01", "192.0.2.50", 23, Some("telnet"), json!({}));
    let opciones_raras = datos_host(
        "vnc-roto",
        "192.0.2.60",
        5900,
        Some("vnc"),
        json!({"escritorio_opciones": {"tamano": "99x10"}}),
    );
    let documento = documento_ajeno(
        FORMATO_MAXIMO,
        vec![
            elemento_host(UUID_SSH, valido),
            elemento_host(UUID_RDP, tipo_raro),
            elemento_host(UUID_VNC, opciones_raras),
        ],
    );
    escribir_documento(&entorno, &documento);

    let resultado = entorno.pasada();
    assert_eq!(resultado.estado, EstadoPasada::AlDia);
    let hosts = entorno.almacen.listar_hosts().expect("listando hosts");
    // Solo se aplica el host válido; los otros dos no tocan el inventario.
    assert_eq!(hosts.len(), 1, "solo el host válido: {hosts:?}");
    assert_eq!(hosts[0].nombre, "web-01");

    // Un host que no se puede validar se guarda como conflicto de elemento.
    let pendientes = conflictos::listar_pendientes(entorno.conexion()).expect("conflictos");
    assert_eq!(
        pendientes.len(),
        2,
        "un conflicto por host inválido: {pendientes:?}"
    );
    for conflicto in &pendientes {
        assert_eq!(conflicto.tipo, conflictos::TIPO_ELEMENTO);
        assert_eq!(conflicto.elemento_tipo, "host");
        assert!(
            ["telnet-01", "vnc-roto"].contains(&conflicto.nombre.as_str()),
            "conflicto de un host inesperado: {}",
            conflicto.nombre
        );
    }
}

/// Comprobación de que el nombre del fichero propio se calcula con el uuid de
/// este equipo (lo usan las pruebas de arriba).
#[test]
fn el_fichero_propio_lleva_el_uuid_del_equipo() {
    let entorno = Entorno::nuevo();
    let esperado = carpeta::nombre_fichero(&entorno.uuid_este());
    assert_eq!(
        entorno.fichero_propio().file_name().unwrap().to_str(),
        Some(esperado.as_str())
    );
}
