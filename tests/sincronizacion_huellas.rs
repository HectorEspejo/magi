//! Pruebas de las huellas de `known_hosts` que viajan con el inventario
//! (Fase 10, §4.4): se añade si este equipo no tiene ninguna de ese tipo, la
//! misma no hace nada, otra distinta deja conflicto sin tocar `known_hosts` y
//! aceptar sustituye con copia previa. Se cubren también las entradas
//! hasheadas, que escribe OpenSSH con `ssh-keygen -H`.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use magi::almacen::conflictos::Conflicto;
use magi::almacen::Almacen;
use magi::config::Rutas;
use magi::modelo::{DatosHost, Origen};
use magi::sincronizacion::formato::HuellaDocumento;
use magi::sincronizacion::huellas;

/// Clave pública de prueba (pareja de usar y tirar; solo la parte pública).
const CLAVE_A: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIIssqddrV94bmHCxAEc2P4g22y1jImlSnbAMbOuQlbeb";
/// Otra clave pública de prueba, distinta de la anterior.
const CLAVE_B: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIIDSbhKpsEjjmB+GtukNZH+cmkil1RoT1h1++kgP94rD";
/// Entrada hasheada de `known_hosts` para `db-02.ejemplo.org` con CLAVE_A.
/// Se generó con `ssh-keygen -H -f known_hosts`: el salt y el HMAC son fijos,
/// así que la prueba no depende de que `ssh-keygen` esté instalado.
const HASHED_A: &str = "|1|nH8BWy1BJL1nYz0oTcRB0VdGKvE=|GX0EeBK+g6D1rj6C0A/Ih5+Vf0g= ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIIssqddrV94bmHCxAEc2P4g22y1jImlSnbAMbOuQlbeb";

struct Entorno {
    _temporal: tempfile::TempDir,
    rutas: Rutas,
    almacen: Almacen,
    known_hosts: PathBuf,
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
        std::fs::create_dir_all(rutas.dir_ssh()).expect("creando ~/.ssh");
        let almacen = Almacen::abrir(&rutas.base_datos()).expect("creando magi.db");
        let known_hosts = rutas.fichero_known_hosts();
        Self {
            _temporal: temporal,
            rutas,
            almacen,
            known_hosts,
        }
    }

    fn conexion(&self) -> &rusqlite::Connection {
        self.almacen.conexion()
    }

    /// Crea un host en el inventario y devuelve su uuid.
    fn host(&self, nombre: &str, direccion: &str, puerto: u16) -> String {
        let id = self
            .almacen
            .crear_host(
                &DatosHost {
                    nombre: nombre.to_string(),
                    direccion: direccion.to_string(),
                    puerto,
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

    fn leer_known_hosts(&self) -> String {
        std::fs::read_to_string(&self.known_hosts).unwrap_or_default()
    }

    fn escribir_known_hosts(&self, contenido: &str) {
        std::fs::write(&self.known_hosts, contenido).expect("escribiendo known_hosts");
        std::fs::set_permissions(&self.known_hosts, std::fs::Permissions::from_mode(0o600))
            .expect("fijando permisos de known_hosts");
    }

    fn registros(&self, tipo: &str) -> i64 {
        self.conexion()
            .query_row(
                "SELECT COUNT(*) FROM REGISTRO WHERE tipo = ?1",
                [tipo],
                |fila| fila.get(0),
            )
            .expect("contando el registro")
    }

    /// Copias con fecha de known_hosts que dejó una escritura.
    fn copias(&self) -> Vec<PathBuf> {
        std::fs::read_dir(self.rutas.dir_ssh())
            .expect("leyendo ~/.ssh")
            .filter_map(|entrada| entrada.ok().map(|entrada| entrada.path()))
            .filter(|ruta| {
                ruta.file_name()
                    .and_then(|nombre| nombre.to_str())
                    .is_some_and(|nombre| nombre.starts_with("known_hosts.bak-"))
            })
            .collect()
    }
}

/// Documento de huella a partir de la línea pública ("tipo base64").
fn huella(host: &str, clave_publica: &str, aceptada_en: &str) -> HuellaDocumento {
    let mut partes = clave_publica.split_whitespace();
    HuellaDocumento {
        host: host.to_string(),
        tipo: partes.next().unwrap_or_default().to_string(),
        clave: partes.next().unwrap_or_default().to_string(),
        aceptada_en: aceptada_en.to_string(),
    }
}

/// Mapa de equipos con un único equipo ajeno.
fn equipos_de(uuid: &str) -> BTreeMap<String, (String, String)> {
    let mut equipos = BTreeMap::new();
    equipos.insert(
        uuid.to_string(),
        (
            "mac-mini".to_string(),
            "2026-11-05T09:12:44.120+01:00".to_string(),
        ),
    );
    equipos
}

/// Un conflicto de la base de datos, como lo escribiría la pasada.
fn conflicto_de(fusion: &magi::sincronizacion::fusion::ConflictoFusion) -> Conflicto {
    Conflicto {
        id: 1,
        tipo: fusion.tipo.clone(),
        elemento_tipo: fusion.elemento_tipo.clone(),
        uuid: fusion.uuid.clone(),
        nombre: fusion.nombre.clone(),
        version_perdedora_json: fusion.version_perdedora_json.clone(),
        version_ganadora: fusion.version_ganadora.clone(),
        equipo_ganador: fusion.equipo_ganador.clone(),
        detectado_en: "2026-11-05T09:12:44.120+01:00".to_string(),
        resuelto_en: None,
        resolucion: None,
    }
}

#[test]
fn sin_huella_se_anade_con_copia_y_registro() {
    let entorno = Entorno::nuevo();
    let uuid = entorno.host("web", "192.0.2.5", 22);
    let equipo = uuid::Uuid::new_v4().to_string();
    // Un known_hosts previo (aunque solo tenga un comentario) para comprobar
    // que se deja copia con fecha antes de tocarlo.
    entorno.escribir_known_hosts("# known_hosts de prueba\n");

    let resultado = huellas::importar(
        entorno.conexion(),
        &entorno.known_hosts,
        &[huella(&uuid, CLAVE_A, "2026-11-01T10:00:00+01:00")],
        &equipos_de(&equipo),
    )
    .expect("importando la huella");

    assert_eq!(resultado.anadidas, vec!["web".to_string()]);
    assert!(resultado.conflictos.is_empty());
    assert!(resultado.avisos.is_empty(), "{:?}", resultado.avisos);
    let contenido = entorno.leer_known_hosts();
    assert!(
        contenido.contains("192.0.2.5 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIIssqddr"),
        "{contenido}"
    );
    assert_eq!(entorno.registros("huella_sincronizada"), 1);
    assert_eq!(entorno.copias().len(), 1, "falta la copia de known_hosts");
    let permisos = std::fs::metadata(&entorno.known_hosts)
        .expect("leyendo permisos")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(permisos, 0o600);

    // Repetir la misma huella no hace nada: ni duplica líneas ni anota.
    let antes = entorno.leer_known_hosts();
    let repetida = huellas::importar(
        entorno.conexion(),
        &entorno.known_hosts,
        &[huella(&uuid, CLAVE_A, "2026-11-01T10:00:00+01:00")],
        &equipos_de(&equipo),
    )
    .expect("reimportando la misma huella");
    assert!(repetida.anadidas.is_empty());
    assert!(repetida.conflictos.is_empty());
    assert_eq!(entorno.leer_known_hosts(), antes);
    assert_eq!(entorno.registros("huella_sincronizada"), 1);
}

#[test]
fn una_huella_distinta_deja_conflicto_sin_tocar_known_hosts() {
    let entorno = Entorno::nuevo();
    let uuid = entorno.host("web", "192.0.2.5", 22);
    let equipo = uuid::Uuid::new_v4().to_string();
    entorno.escribir_known_hosts(&format!("192.0.2.5 {}\n", CLAVE_A));

    let resultado = huellas::importar(
        entorno.conexion(),
        &entorno.known_hosts,
        &[huella(&uuid, CLAVE_B, "2026-11-04T18:30:00+01:00")],
        &equipos_de(&equipo),
    )
    .expect("importando la huella distinta");

    assert!(resultado.anadidas.is_empty());
    assert_eq!(resultado.conflictos.len(), 1);
    let fusion = &resultado.conflictos[0];
    assert_eq!(fusion.tipo, "huella");
    assert_eq!(fusion.elemento_tipo, "host");
    assert_eq!(fusion.uuid, uuid);
    assert_eq!(fusion.nombre, "web");
    assert_eq!(fusion.equipo_ganador.as_deref(), Some(equipo.as_str()));
    // known_hosts sigue exactamente como estaba y no se anota una sustitución.
    assert_eq!(
        entorno.leer_known_hosts(),
        format!("192.0.2.5 {}\n", CLAVE_A)
    );
    assert_eq!(entorno.registros("huella_sincronizada"), 0);
    assert_eq!(entorno.registros("huella_sustituida"), 0);

    // El JSON de la versión perdedora lleva mis huellas y las ajenas.
    let json: serde_json::Value = serde_json::from_str(
        fusion
            .version_perdedora_json
            .as_deref()
            .expect("falta el JSON de la versión perdedora"),
    )
    .expect("el JSON del conflicto no es válido");
    let mias = json["mias"].as_array().expect("faltan mis huellas");
    assert_eq!(mias.len(), 1);
    assert_eq!(mias[0]["tipo"], "ssh-ed25519");
    assert!(
        mias[0]["huella"]
            .as_str()
            .unwrap_or_default()
            .starts_with("SHA256:"),
        "{json}"
    );
    let suyas = json["suyas"].as_array().expect("faltan las huellas ajenas");
    assert_eq!(suyas.len(), 1);
    assert_eq!(suyas[0]["equipo"], equipo);
    assert_eq!(suyas[0]["nombre"], "mac-mini");
    assert_eq!(suyas[0]["tipo"], "ssh-ed25519");
    assert_eq!(suyas[0]["aceptada_en"], "2026-11-04T18:30:00+01:00");
    // La versión ganadora es la clave ajena en base64.
    let clave_ajena = CLAVE_B.split_whitespace().nth(1).unwrap_or_default();
    assert_eq!(fusion.version_ganadora.as_deref(), Some(clave_ajena));

    // Los helpers de la vista CONFLICTOS extraen la huella ganadora.
    let conflicto = conflicto_de(fusion);
    let ganadora = huellas::datos_huella_ganadora(&conflicto).expect("sin huella ganadora");
    assert_eq!(ganadora.equipo, equipo);
    assert_eq!(ganadora.nombre, "mac-mini");
    assert_eq!(ganadora.tipo, "ssh-ed25519");
    assert_eq!(ganadora.clave, clave_ajena);
    assert_eq!(ganadora.aceptada_en, "2026-11-04T18:30:00+01:00");
    assert!(ganadora.huella.starts_with("SHA256:"));
}

#[test]
fn aceptar_sustituye_la_huella_con_copia_y_registro() {
    let entorno = Entorno::nuevo();
    let uuid = entorno.host("web", "192.0.2.5", 22);
    let equipo = uuid::Uuid::new_v4().to_string();
    entorno.escribir_known_hosts(&format!("192.0.2.5 {}\n", CLAVE_A));

    let resultado = huellas::importar(
        entorno.conexion(),
        &entorno.known_hosts,
        &[huella(&uuid, CLAVE_B, "2026-11-04T18:30:00+01:00")],
        &equipos_de(&equipo),
    )
    .expect("importando la huella distinta");
    let conflicto = conflicto_de(&resultado.conflictos[0]);

    huellas::aceptar_conflicto(entorno.conexion(), &entorno.known_hosts, &conflicto)
        .expect("aceptando la huella ajena");

    let contenido = entorno.leer_known_hosts();
    assert!(
        contenido.contains("192.0.2.5 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIIDSbhKp"),
        "{contenido}"
    );
    assert!(
        !contenido.contains("AAAAC3NzaC1lZDI1NTE5AAAAIIssqddr"),
        "{contenido}"
    );
    assert!(
        std::fs::read_to_string(entorno.known_hosts.with_file_name("known_hosts.old"))
            .unwrap_or_default()
            .contains("AAAAC3NzaC1lZDI1NTE5AAAAIIssqddr"),
        "falta known_hosts.old con la huella anterior"
    );
    assert!(!entorno.copias().is_empty(), "falta la copia con fecha");
    assert_eq!(entorno.registros("huella_sustituida"), 1);

    // Aceptar otra vez la misma no es un efecto nuevo: no vuelve a anotar.
    huellas::aceptar(
        entorno.conexion(),
        &entorno.known_hosts,
        &uuid,
        "ssh-ed25519",
        CLAVE_B.split_whitespace().nth(1).unwrap_or_default(),
    )
    .expect("reaceptando la huella");
    assert_eq!(entorno.registros("huella_sustituida"), 1);
}

#[test]
fn las_entradas_hasheadas_se_entienden_y_se_sustituyen() {
    let entorno = Entorno::nuevo();
    let uuid = entorno.host("db-02", "db-02.ejemplo.org", 22);
    let equipo = uuid::Uuid::new_v4().to_string();
    entorno.escribir_known_hosts(&format!("{HASHED_A}\n"));

    // La misma huella, hasheada: no hay nada que añadir.
    let misma = huellas::importar(
        entorno.conexion(),
        &entorno.known_hosts,
        &[huella(&uuid, CLAVE_A, "2026-11-01T10:00:00+01:00")],
        &equipos_de(&equipo),
    )
    .expect("importando una huella ya conocida hasheada");
    assert!(misma.anadidas.is_empty());
    assert!(misma.conflictos.is_empty());
    assert_eq!(entorno.leer_known_hosts(), format!("{HASHED_A}\n"));

    // Otra huella: conflicto sin tocar el fichero.
    let distinta = huellas::importar(
        entorno.conexion(),
        &entorno.known_hosts,
        &[huella(&uuid, CLAVE_B, "2026-11-04T18:30:00+01:00")],
        &equipos_de(&equipo),
    )
    .expect("importando una huella distinta sobre una entrada hasheada");
    assert_eq!(distinta.conflictos.len(), 1);
    let conflicto = conflicto_de(&distinta.conflictos[0]);
    entorno.escribir_known_hosts(&format!("{HASHED_A}\n"));

    // Aceptar la ajena quita la línea hasheada y deja la nueva en claro.
    huellas::aceptar_conflicto(entorno.conexion(), &entorno.known_hosts, &conflicto)
        .expect("aceptando sobre una entrada hasheada");
    let contenido = entorno.leer_known_hosts();
    assert!(
        !contenido.contains("|1|nH8BWy1BJL1nYz0oTcRB0VdGKvE="),
        "{contenido}"
    );
    assert!(
        contenido.contains("db-02.ejemplo.org ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIIDSbhKp"),
        "{contenido}"
    );
    assert_eq!(entorno.registros("huella_sustituida"), 1);
}

#[test]
fn un_host_desconocido_y_una_clave_invalida_solo_dejan_aviso() {
    let entorno = Entorno::nuevo();
    let uuid = entorno.host("web", "192.0.2.5", 22);
    let equipo = uuid::Uuid::new_v4().to_string();
    let desconocido = uuid::Uuid::new_v4().to_string();

    // Host que no está en el inventario: aviso y nothing se toca.
    let resultado = huellas::importar(
        entorno.conexion(),
        &entorno.known_hosts,
        &[huella(&desconocido, CLAVE_A, "2026-11-01T10:00:00+01:00")],
        &equipos_de(&equipo),
    )
    .expect("importando una huella de un host desconocido");
    assert!(resultado.anadidas.is_empty());
    assert!(resultado.conflictos.is_empty());
    assert_eq!(resultado.avisos.len(), 1);
    assert!(
        resultado.avisos[0].contains("no está en este inventario"),
        "{:?}",
        resultado.avisos
    );
    assert!(!entorno.known_hosts.exists());

    // Clave pública corrupta: aviso, sin escribir nada.
    let mut rota = huella(&uuid, CLAVE_A, "2026-11-01T10:00:00+01:00");
    rota.clave = "esto-no-es-base64".to_string();
    let resultado = huellas::importar(
        entorno.conexion(),
        &entorno.known_hosts,
        &[rota],
        &equipos_de(&equipo),
    )
    .expect("importando una huella corrupta");
    assert!(resultado.anadidas.is_empty());
    assert_eq!(resultado.avisos.len(), 1);
    assert!(
        resultado.avisos[0].contains("no válida"),
        "{:?}",
        resultado.avisos
    );
    assert!(!entorno.known_hosts.exists());

    // Aceptar una huella de un host que no existe es un error.
    assert!(huellas::aceptar(
        entorno.conexion(),
        &entorno.known_hosts,
        &desconocido,
        "ssh-ed25519",
        CLAVE_A.split_whitespace().nth(1).unwrap_or_default(),
    )
    .is_err());
}

#[test]
fn una_huella_que_no_es_de_tipo_huella_no_se_puede_aceptar() {
    let conflicto = Conflicto {
        id: 7,
        tipo: "elemento".to_string(),
        elemento_tipo: "host".to_string(),
        uuid: "0f1e2d3c-2222-4b3a-9c1d-abcdef012345".to_string(),
        nombre: "web".to_string(),
        version_perdedora_json: None,
        version_ganadora: None,
        equipo_ganador: None,
        detectado_en: "2026-11-05T09:12:44.120+01:00".to_string(),
        resuelto_en: None,
        resolucion: None,
    };
    assert!(huellas::datos_huella_ganadora(&conflicto).is_none());
    let entorno = Entorno::nuevo();
    assert!(
        huellas::aceptar_conflicto(entorno.conexion(), &entorno.known_hosts, &conflicto).is_err()
    );
}

/// Comprueba que la ruta del known_hosts de la prueba es la de ~/.ssh.
#[test]
fn el_known_hosts_de_prueba_esta_bajo_ssh() {
    let entorno = Entorno::nuevo();
    assert_eq!(
        entorno.known_hosts,
        entorno.rutas.dir_ssh().join("known_hosts")
    );
    assert!(Path::new(&entorno.known_hosts).starts_with(&entorno.rutas.hogar));
}
