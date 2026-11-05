//! Pruebas de la pasada de sincronización en modo lector (Fase 11, §7.3 y
//! §7.4): un fichero ajeno que borra 10 de 12 hosts se aplica sin preguntar y
//! sin copia, la carpeta queda byte a byte igual, los borrados con trabajo vivo
//! se aplazan y se aplican solos al terminar, la fusión es determinista frente
//! al escritorio, la carpeta es entrada no fiable (T81) y las huellas se
//! importan como en el escritorio dejando solo conflictos de huella y borrado.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use magi_nucleo::almacen::conflictos::{self, Conflicto};
use magi_nucleo::almacen::Almacen;
use magi_nucleo::almacen::{equipos as equipos_bd, versiones};
use magi_nucleo::configuracion::SeccionSincronizacion;
use magi_nucleo::escritorio::{OpcionesEscritorio, TamanoEscritorio, TipoHost};
use magi_nucleo::llavero::{LlaveroEnMemoria, LlaveroSecreto, CUENTA_SINCRONIZACION};
use magi_nucleo::modelo::{DatosHost, DatosTunel, Host, Origen, TipoTunel, Tunel};
use magi_nucleo::rutas::{Equipo, Rutas};
use magi_nucleo::sincronizacion::carpeta::{
    self, CarpetaDisco, CarpetaSincronizacion, EntradaCarpeta,
};
use magi_nucleo::sincronizacion::cifrado::ClaveSincronizacion;
use magi_nucleo::sincronizacion::formato;
use magi_nucleo::sincronizacion::lector::{self, EstadoLectura, ResultadoLectura};
use magi_nucleo::sincronizacion::{
    self, documento_propio, EstadoPasada, OpcionesPasada, PoliticaBorrados, ResultadoPasada,
};
use magi_nucleo::trabajo_vivo::TrabajoVivo;

/// Clave pública de prueba (pareja de usar y tirar; solo la parte pública).
const CLAVE_PUBLICA_A: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIIssqddrV94bmHCxAEc2P4g22y1jImlSnbAMbOuQlbeb";
/// Otra clave pública de prueba, distinta de la anterior.
const CLAVE_PUBLICA_B: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIIDSbhKpsEjjmB+GtukNZH+cmkil1RoT1h1++kgP94rD";

// ------------------------------------------------------------------ dobles

/// Un dispositivo sin trabajo vivo.
struct SinTrabajo;

impl TrabajoVivo for SinTrabajo {
    fn tiene_trabajo(&self, _tipo: &str, _uuid: &str) -> bool {
        false
    }
}

/// Trabajo vivo fijado a mano: pares (tipo de elemento, uuid).
struct TrabajoDePrueba {
    pares: Vec<(String, String)>,
}

impl TrabajoDePrueba {
    fn de(pares: &[(&str, &str)]) -> Self {
        Self {
            pares: pares
                .iter()
                .map(|(tipo, uuid)| ((*tipo).to_string(), (*uuid).to_string()))
                .collect(),
        }
    }
}

impl TrabajoVivo for TrabajoDePrueba {
    fn tiene_trabajo(&self, tipo: &str, uuid: &str) -> bool {
        self.pares
            .iter()
            .any(|(tipo_vivo, uuid_vivo)| tipo_vivo == tipo && uuid_vivo == uuid)
    }
}

/// Servicio de carpeta deliberadamente permisivo: lista todo lo que hay (sin
/// filtrar nombres ni tipos) y apunta qué ficheros se le piden. Simula una
/// implementación de SAF que no valida nada, para comprobar que la pasada
/// revalida por su cuenta (T81).
struct CarpetaCruda {
    ruta: PathBuf,
    leidos: std::sync::Mutex<Vec<String>>,
}

impl CarpetaCruda {
    fn nueva(ruta: &Path) -> Self {
        Self {
            ruta: ruta.to_path_buf(),
            leidos: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn leidos(&self) -> Vec<String> {
        self.leidos.lock().expect("cerrojo de lecturas").clone()
    }
}

impl CarpetaSincronizacion for CarpetaCruda {
    fn listar(&self) -> Result<Vec<EntradaCarpeta>, String> {
        let mut salida = Vec::new();
        for entrada in std::fs::read_dir(&self.ruta).map_err(|error| error.to_string())? {
            let entrada = entrada.map_err(|error| error.to_string())?;
            let metadatos =
                std::fs::symlink_metadata(entrada.path()).map_err(|error| error.to_string())?;
            salida.push(EntradaCarpeta {
                nombre: entrada.file_name().to_string_lossy().to_string(),
                tamano: metadatos.len(),
                modificado_ms: 0,
            });
        }
        Ok(salida)
    }

    fn leer(&self, nombre: &str, maximo: u64) -> Result<Vec<u8>, String> {
        self.leidos
            .lock()
            .map_err(|_| "cerrojo envenenado".to_string())?
            .push(nombre.to_string());
        let bytes = std::fs::read(self.ruta.join(nombre)).map_err(|error| error.to_string())?;
        if bytes.len() as u64 > maximo {
            return Err(format!("el fichero ocupa más de {maximo} bytes"));
        }
        Ok(bytes)
    }
}

// ----------------------------------------------------------------- entorno

/// Un MAGI de prueba con su magi.db, sus rutas, su clave en el llavero en
/// memoria y la carpeta compartida.
struct Entorno {
    _temporal: tempfile::TempDir,
    rutas: Rutas,
    almacen: Almacen,
    carpeta: PathBuf,
    texto_clave: String,
    llavero: LlaveroEnMemoria,
}

impl Entorno {
    fn nuevo(carpeta: &Path, nombre: &str, plataforma: &str, texto_clave: &str) -> Self {
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
        std::fs::write(rutas.fichero_known_hosts(), b"").expect("known_hosts vacío");
        let almacen = Almacen::abrir(
            &rutas.base_datos(),
            &Equipo::nuevo(nombre, plataforma),
            "1.1.0",
        )
        .expect("creando magi.db");
        let llavero = LlaveroEnMemoria::nuevo();
        llavero
            .guardar(CUENTA_SINCRONIZACION, texto_clave)
            .expect("guardando la clave de sincronización");
        Self {
            _temporal: temporal,
            rutas,
            almacen,
            carpeta: carpeta.to_path_buf(),
            texto_clave: texto_clave.to_string(),
            llavero,
        }
    }

    fn conexion(&self) -> &rusqlite::Connection {
        self.almacen.conexion()
    }

    fn clave(&self) -> ClaveSincronizacion {
        ClaveSincronizacion::desde_texto(&self.texto_clave).expect("clave de prueba ilegible")
    }

    /// Claves de sincronización que ve la pasada: las que hay en el llavero.
    fn claves(&self) -> Vec<ClaveSincronizacion> {
        match self
            .llavero
            .recuperar(CUENTA_SINCRONIZACION)
            .expect("leyendo el llavero")
        {
            Some(texto) => {
                vec![ClaveSincronizacion::desde_texto(&texto).expect("clave del llavero ilegible")]
            }
            None => Vec::new(),
        }
    }

    fn escribir_llavero(&self, texto_clave: &str) {
        self.llavero
            .guardar(CUENTA_SINCRONIZACION, texto_clave)
            .expect("guardando la clave en el llavero");
    }

    fn uuid_este(&self) -> String {
        equipos_bd::este(self.conexion())
            .expect("leyendo este equipo")
            .uuid
    }

    fn estado_equipo(&self, uuid: &str) -> String {
        equipos_bd::por_uuid(self.conexion(), uuid)
            .expect("buscando el equipo")
            .expect("el equipo no existe")
            .estado
    }

    fn hosts(&self) -> Vec<Host> {
        self.almacen.listar_hosts().expect("listando hosts")
    }

    fn host_por_nombre(&self, nombre: &str) -> Option<Host> {
        self.hosts().into_iter().find(|host| host.nombre == nombre)
    }

    fn crear_host(&self, nombre: &str, direccion: &str) -> String {
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

    fn borrar_host(&self, nombre: &str) {
        let id = self
            .host_por_nombre(nombre)
            .expect("el host que se va a borrar no existe")
            .id;
        self.almacen.borrar_host(id).expect("borrando el host");
    }

    fn editar_host(&self, nombre: &str, direccion: &str) {
        let host = self.host_por_nombre(nombre).expect("el host no existe");
        self.almacen
            .actualizar_host(
                host.id,
                &DatosHost {
                    nombre: host.nombre.clone(),
                    grupo_id: host.grupo_id,
                    direccion: direccion.to_string(),
                    puerto: host.puerto,
                    tipo: host.tipo,
                    escritorio_dominio: host.escritorio_dominio.clone(),
                    escritorio_opciones: host.escritorio_opciones.clone(),
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
            .expect("editando el host");
    }

    fn uuid_host(&self, nombre: &str) -> String {
        self.conexion()
            .query_row(
                "SELECT uuid FROM HOSTS WHERE nombre = ?1",
                [nombre],
                |fila| fila.get(0),
            )
            .expect("leyendo el uuid del host")
    }

    fn tuneles(&self) -> Vec<Tunel> {
        self.almacen.listar_tuneles().expect("listando túneles")
    }

    fn crear_tunel(&self, host_id: i64, nombre: &str) -> String {
        let id = self
            .almacen
            .crear_tunel(&DatosTunel {
                host_id,
                nombre: nombre.to_string(),
                tipo: TipoTunel::Local,
                escucha: "127.0.0.1:8080".to_string(),
                destino: Some("192.0.2.10:80".to_string()),
                automatico: false,
            })
            .expect("creando el túnel");
        self.conexion()
            .query_row("SELECT uuid FROM TUNELES WHERE id = ?1", [id], |fila| {
                fila.get(0)
            })
            .expect("leyendo el uuid del túnel")
    }

    fn uuid_tunel(&self, nombre: &str) -> String {
        self.conexion()
            .query_row(
                "SELECT uuid FROM TUNELES WHERE nombre = ?1",
                [nombre],
                |fila| fila.get(0),
            )
            .expect("leyendo el uuid del túnel")
    }

    fn version_host(&self, uuid: &str) -> String {
        self.conexion()
            .query_row(
                "SELECT version FROM HOSTS WHERE uuid = ?1",
                [uuid],
                |fila| fila.get(0),
            )
            .expect("leyendo la versión del host")
    }

    fn lapidas(&self) -> i64 {
        self.conexion()
            .query_row("SELECT COUNT(*) FROM LAPIDAS", [], |fila| fila.get(0))
            .expect("contando lápidas")
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

    fn conflictos(&self) -> Vec<Conflicto> {
        conflictos::listar_pendientes(self.conexion()).expect("listando conflictos")
    }

    fn leer_known_hosts(&self) -> String {
        std::fs::read_to_string(self.rutas.fichero_known_hosts()).unwrap_or_default()
    }

    fn escribir_known_hosts(&self, contenido: &str) {
        std::fs::write(self.rutas.fichero_known_hosts(), contenido)
            .expect("escribiendo known_hosts");
        std::fs::set_permissions(
            self.rutas.fichero_known_hosts(),
            std::fs::Permissions::from_mode(0o600),
        )
        .expect("fijando permisos de known_hosts");
    }

    /// Exporta el fichero propio como lo haría el escritorio.
    fn exportar(&self) {
        self.exportar_con(&self.clave());
    }

    fn exportar_con(&self, clave: &ClaveSincronizacion) {
        let documento = documento_propio(self.conexion(), &self.rutas, "1.1.0")
            .expect("construyendo el documento propio");
        escribir_documento(&self.carpeta, clave, &documento);
    }

    fn pasada(&self, trabajo_vivo: &dyn TrabajoVivo) -> ResultadoLectura {
        self.pasada_con(&CarpetaDisco::nueva(&self.carpeta), trabajo_vivo)
    }

    fn pasada_con(
        &self,
        servicio: &dyn CarpetaSincronizacion,
        trabajo_vivo: &dyn TrabajoVivo,
    ) -> ResultadoLectura {
        let claves = self.claves();
        lector::pasada_lector(
            self.conexion(),
            &self.rutas,
            servicio,
            &claves,
            trabajo_vivo,
        )
        .expect("la pasada en modo lector falla")
    }

    /// Una pasada en modo escritor, como la del escritorio (solo para el test
    /// de determinismo).
    fn pasada_escritora(&self) -> ResultadoPasada {
        let ajustes = SeccionSincronizacion {
            carpeta: Some(self.carpeta.to_string_lossy().to_string()),
            umbral_borrado_hosts: 1000,
            umbral_borrado_pct: 1000,
            ..SeccionSincronizacion::default()
        };
        let claves = self.claves();
        sincronizacion::pasada(
            self.conexion(),
            &self.rutas,
            &ajustes,
            "1.1.0",
            &claves,
            &OpcionesPasada::sin_trabajo_vivo(PoliticaBorrados::Preguntar),
        )
        .expect("la pasada en modo escritor falla")
    }
}

// --------------------------------------------------------------- utilidades

fn carpeta_compartida() -> (tempfile::TempDir, PathBuf) {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let carpeta = temporal.path().join("sync");
    std::fs::create_dir_all(&carpeta).expect("creando la carpeta compartida");
    (temporal, carpeta)
}

/// Escribe un documento firmado y cifrado como fichero de su equipo.
fn escribir_documento(carpeta: &Path, clave: &ClaveSincronizacion, documento: &formato::Documento) {
    let json = formato::serializar(documento).expect("serializando el documento");
    let firma = clave
        .firmar(json.as_bytes())
        .expect("firmando el documento");
    let plano = formato::componer(&json, &firma, documento.formato);
    let cifrado = clave.cifrar(&plano).expect("cifrando el documento");
    carpeta::escribir(carpeta, &documento.equipo.uuid, &cifrado)
        .expect("escribiendo el fichero de sincronización");
}

/// Retrato byte a byte de la carpeta: nombre → contenido (o destino del enlace
/// o marca de subcarpeta). Sirve para comprobar que la pasada en modo lector no
/// escribe nada.
fn retrato_carpeta(carpeta: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut retrato = BTreeMap::new();
    let Ok(entradas) = std::fs::read_dir(carpeta) else {
        return retrato;
    };
    for entrada in entradas {
        let entrada = entrada.expect("leyendo la carpeta");
        let nombre = entrada.file_name().to_string_lossy().to_string();
        let metadatos = std::fs::symlink_metadata(entrada.path()).expect("leyendo metadatos");
        let contenido = if metadatos.file_type().is_symlink() {
            std::fs::read_link(entrada.path())
                .expect("leyendo el enlace")
                .to_string_lossy()
                .as_bytes()
                .to_vec()
        } else if metadatos.is_file() {
            std::fs::read(entrada.path()).expect("leyendo el fichero")
        } else {
            b"<directorio>".to_vec()
        };
        retrato.insert(nombre, contenido);
    }
    retrato
}

/// Retrato canónico del inventario que viaja (grupos, hosts, túneles y
/// lápidas), sin ids locales ni fechas de exportación. Dos equipos con el mismo
/// conjunto de ficheros deben llegar a este mismo retrato.
fn retrato_inventario(conexion: &rusqlite::Connection) -> Vec<String> {
    let mut lineas = Vec::new();
    let mut sentencia = conexion
        .prepare("SELECT uuid, nombre, version, vector FROM GRUPOS ORDER BY uuid")
        .expect("preparando grupos");
    let filas = sentencia
        .query_map([], |fila| {
            Ok(format!(
                "grupo|{}|{}|{}|{}",
                fila.get::<_, String>(0)?,
                fila.get::<_, String>(1)?,
                fila.get::<_, String>(2)?,
                fila.get::<_, String>(3)?
            ))
        })
        .expect("leyendo grupos");
    lineas.extend(filas.map(|fila| fila.expect("fila de grupo")));
    let mut sentencia = conexion
        .prepare(
            "SELECT h.uuid, h.nombre, h.direccion, h.puerto, COALESCE(h.usuario, ''),
                    COALESCE(g.uuid, ''), h.version, h.vector
               FROM HOSTS h LEFT JOIN GRUPOS g ON g.id = h.grupo_id
              ORDER BY h.uuid",
        )
        .expect("preparando hosts");
    let filas = sentencia
        .query_map([], |fila| {
            Ok(format!(
                "host|{}|{}|{}|{}|{}|{}|{}|{}",
                fila.get::<_, String>(0)?,
                fila.get::<_, String>(1)?,
                fila.get::<_, String>(2)?,
                fila.get::<_, i64>(3)?,
                fila.get::<_, String>(4)?,
                fila.get::<_, String>(5)?,
                fila.get::<_, String>(6)?,
                fila.get::<_, String>(7)?
            ))
        })
        .expect("leyendo hosts");
    lineas.extend(filas.map(|fila| fila.expect("fila de host")));
    let mut sentencia = conexion
        .prepare(
            "SELECT t.uuid, t.nombre, t.tipo, t.escucha, COALESCE(t.destino, ''),
                    t.automatico, COALESCE(h.uuid, ''), t.version, t.vector
               FROM TUNELES t LEFT JOIN HOSTS h ON h.id = t.host_id
              ORDER BY t.uuid",
        )
        .expect("preparando túneles");
    let filas = sentencia
        .query_map([], |fila| {
            Ok(format!(
                "tunel|{}|{}|{}|{}|{}|{}|{}|{}|{}",
                fila.get::<_, String>(0)?,
                fila.get::<_, String>(1)?,
                fila.get::<_, String>(2)?,
                fila.get::<_, String>(3)?,
                fila.get::<_, String>(4)?,
                fila.get::<_, i64>(5)?,
                fila.get::<_, String>(6)?,
                fila.get::<_, String>(7)?,
                fila.get::<_, String>(8)?
            ))
        })
        .expect("leyendo túneles");
    lineas.extend(filas.map(|fila| fila.expect("fila de túnel")));
    let mut sentencia = conexion
        .prepare("SELECT uuid, tipo, version, vector FROM LAPIDAS ORDER BY uuid")
        .expect("preparando lápidas");
    let filas = sentencia
        .query_map([], |fila| {
            Ok(format!(
                "lapida|{}|{}|{}|{}",
                fila.get::<_, String>(0)?,
                fila.get::<_, String>(1)?,
                fila.get::<_, String>(2)?,
                fila.get::<_, String>(3)?
            ))
        })
        .expect("leyendo lápidas");
    lineas.extend(filas.map(|fila| fila.expect("fila de lápida")));
    lineas
}

// ------------------------------------------------------------------ pruebas

#[test]
fn un_fichero_ajeno_que_borra_diez_de_doce_hosts_se_aplica_sin_preguntar_ni_copia() {
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = ClaveSincronizacion::generar().texto().to_string();
    let escritorio = Entorno::nuevo(&carpeta, "omarchy", "linux", &clave);
    let movil = Entorno::nuevo(&carpeta, "pixel", "android", &clave);
    for indice in 0..12 {
        escritorio.crear_host(
            &format!("host-{indice:02}"),
            &format!("192.0.2.{}", indice + 1),
        );
    }
    escritorio.exportar();

    let primera = movil.pasada(&SinTrabajo);
    assert_eq!(primera.estado, EstadoLectura::AlDia);
    assert_eq!(movil.hosts().len(), 12, "se importa el inventario ajeno");
    assert_eq!(primera.resumen.expect("hay resumen").creados, 12);
    assert_eq!(movil.registros("inventario_importado"), 1);

    // El escritorio borra 10 de los 12 hosts y reexporta.
    for indice in 0..10 {
        escritorio.borrar_host(&format!("host-{indice:02}"));
    }
    escritorio.exportar();
    let antes = retrato_carpeta(&carpeta);

    let resultado = movil.pasada(&SinTrabajo);
    assert_eq!(resultado.estado, EstadoLectura::AlDia);
    assert_eq!(
        movil.hosts().len(),
        2,
        "los 10 borrados se aplican sin preguntar (el lector no tiene «decisión pendiente»)"
    );
    let resumen = resultado.resumen.expect("hay resumen");
    assert_eq!(resumen.borrados, 10);
    assert_eq!(resumen.equipos, 1);
    assert!(
        !movil.rutas.datos.join("copias").exists(),
        "la pasada en modo lector no hace copia de magi.db"
    );
    assert_eq!(
        retrato_carpeta(&carpeta),
        antes,
        "la carpeta queda byte a byte igual: el lector no escribe ni exporta"
    );
    assert_eq!(resultado.conflictos_pendientes, 0);
    assert_eq!(movil.registros("inventario_importado"), 2);
    assert!(
        movil.lapidas() >= 10,
        "las lápidas vienen del fichero ajeno"
    );
}

#[test]
fn un_borrado_con_trabajo_vivo_se_aplaza_y_se_aplica_solo_cuando_termina() {
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = ClaveSincronizacion::generar().texto().to_string();
    let escritorio = Entorno::nuevo(&carpeta, "omarchy", "linux", &clave);
    let movil = Entorno::nuevo(&carpeta, "pixel", "android", &clave);
    let uuid_host = escritorio.crear_host("web", "192.0.2.5");
    let id_host = escritorio
        .host_por_nombre("web")
        .expect("el host existe")
        .id;
    let uuid_tunel = escritorio.crear_tunel(id_host, "web-local");
    escritorio.exportar();
    movil.pasada(&SinTrabajo);
    assert_eq!(movil.hosts().len(), 1);
    assert_eq!(movil.tuneles().len(), 1);
    assert_eq!(movil.uuid_host("web"), uuid_host);
    assert_eq!(movil.uuid_tunel("web-local"), uuid_tunel);

    // El escritorio borra el host (con su túnel en cascada) y reexporta. En el
    // móvil hay una sesión abierta y un túnel activo de ese host.
    escritorio.borrar_host("web");
    escritorio.exportar();
    let trabajo = TrabajoDePrueba::de(&[("host", &uuid_host), ("tunel", &uuid_tunel)]);
    let aplazada = movil.pasada(&trabajo);
    assert_eq!(aplazada.estado, EstadoLectura::AlDia);
    assert_eq!(movil.hosts().len(), 1, "el host con trabajo vivo sigue");
    assert_eq!(movil.tuneles().len(), 1, "el túnel activo sigue");
    assert_eq!(aplazada.conflictos_pendientes, 2);
    let tipos: Vec<String> = movil
        .conflictos()
        .into_iter()
        .map(|conflicto| conflicto.tipo)
        .collect();
    assert_eq!(tipos, vec![conflictos::TIPO_BORRADO.to_string(); 2]);
    assert_eq!(
        movil.lapidas(),
        0,
        "un borrado aplazado no deja lápida local"
    );
    assert_eq!(
        aplazada.resumen.as_ref().expect("hay resumen").borrados,
        0,
        "aplazar no borra nada"
    );

    // Al cerrar la sesión y el túnel, la siguiente pasada relee todos los
    // ficheros (aunque su hash no cambió) y aplica los borrados sola y sin
    // preguntar.
    let aplicada = movil.pasada(&SinTrabajo);
    assert_eq!(aplicada.estado, EstadoLectura::AlDia);
    assert_eq!(movil.hosts().len(), 0, "el borrado aplazado se aplica solo");
    assert_eq!(movil.tuneles().len(), 0);
    assert_eq!(
        aplicada.conflictos_pendientes, 0,
        "el conflicto de borrado se resuelve al aplicarse"
    );
    assert_eq!(aplicada.resumen.expect("hay resumen").borrados, 2);
    assert_eq!(movil.lapidas(), 2);
}

#[test]
fn las_huellas_se_importan_como_en_el_escritorio_y_solo_dejan_conflicto_de_huella() {
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = ClaveSincronizacion::generar().texto().to_string();
    let escritorio = Entorno::nuevo(&carpeta, "omarchy", "linux", &clave);
    let movil = Entorno::nuevo(&carpeta, "pixel", "android", &clave);
    escritorio.crear_host("web", "192.0.2.5");
    escritorio.escribir_known_hosts(&format!("192.0.2.5 {CLAVE_PUBLICA_A}\n"));
    escritorio.exportar();

    // Sin huellas en el móvil: se añade, como en el escritorio.
    let primera = movil.pasada(&SinTrabajo);
    assert_eq!(primera.estado, EstadoLectura::AlDia);
    let known_hosts = movil.leer_known_hosts();
    assert!(known_hosts.contains("192.0.2.5 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIIss"));
    assert_eq!(movil.registros("huella_sincronizada"), 1);
    assert_eq!(movil.conflictos().len(), 0);

    // Repetir la pasada sin cambios no duplica la huella.
    movil.pasada(&SinTrabajo);
    assert_eq!(movil.registros("huella_sincronizada"), 1);

    // El móvil ya tenía otra huella distinta: conflicto de huella y no se
    // sustituye sola.
    movil.escribir_known_hosts(&format!("192.0.2.5 {CLAVE_PUBLICA_B}\n"));
    escritorio.exportar();
    let conflictiva = movil.pasada(&SinTrabajo);
    assert_eq!(conflictiva.estado, EstadoLectura::AlDia);
    assert_eq!(conflictiva.conflictos_pendientes, 1);
    let conflictos = movil.conflictos();
    assert_eq!(conflictos.len(), 1);
    assert_eq!(conflictos[0].tipo, conflictos::TIPO_HUELLA);
    assert_eq!(
        movil.leer_known_hosts(),
        format!("192.0.2.5 {CLAVE_PUBLICA_B}\n"),
        "la huella del móvil no se sustituye sin que el usuario lo acepte"
    );
}

#[test]
fn un_conflicto_de_elemento_de_la_fusion_no_se_guarda_en_el_movil() {
    // El móvil no edita el inventario, pero la fusión puede producir un
    // conflicto de elemento si una edición local concurrente pierde (por
    // ejemplo, un renombrado o una recuperación manual). Se simula esa edición
    // local y se comprueba que la pasada aplica la versión ganadora sin
    // guardar el conflicto, que en el móvil nunca se resolvería.
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = ClaveSincronizacion::generar().texto().to_string();
    let escritorio = Entorno::nuevo(&carpeta, "omarchy", "linux", &clave);
    let movil = Entorno::nuevo(&carpeta, "pixel", "android", &clave);
    let uuid = escritorio.crear_host("web", "192.0.2.5");
    escritorio.exportar();
    movil.pasada(&SinTrabajo);

    let version_1 = movil.version_host(&uuid);
    let ms_1 = versiones::ms_de_version(&version_1).expect("la versión tiene milisegundos");
    let uuid_escritorio = escritorio.uuid_este();
    let uuid_movil = movil.uuid_este();

    // El escritorio cambia la dirección con la edición ms_1 + 10.
    let ms_escritorio = ms_1 + 10;
    escritorio.editar_host("web", "192.0.2.99");
    escritorio
        .conexion()
        .execute(
            "UPDATE HOSTS SET version = ?1, vector = ?2 WHERE uuid = ?3",
            rusqlite::params![
                versiones::formatear_version(ms_escritorio, &uuid_escritorio),
                format!("{{\"{uuid_escritorio}\":{ms_escritorio}}}"),
                uuid
            ],
        )
        .expect("fijando la versión del escritorio");
    escritorio.exportar();

    // El móvil tenía una edición local concurrente con la dirección vieja y
    // ms_1 + 5: no domina a la del escritorio y pierde por versión más nueva.
    let ms_movil = ms_1 + 5;
    movil
        .conexion()
        .execute(
            "UPDATE HOSTS SET direccion = '192.0.2.77', version = ?1, vector = ?2 WHERE uuid = ?3",
            rusqlite::params![
                versiones::formatear_version(ms_movil, &uuid_movil),
                format!("{{\"{uuid_escritorio}\":{ms_1},\"{uuid_movil}\":{ms_movil}}}"),
                uuid
            ],
        )
        .expect("simulando la edición local del móvil");

    let resultado = movil.pasada(&SinTrabajo);
    assert_eq!(resultado.estado, EstadoLectura::AlDia);
    assert_eq!(
        movil
            .host_por_nombre("web")
            .expect("el host existe")
            .direccion,
        "192.0.2.99",
        "gana la versión del escritorio"
    );
    assert_eq!(
        movil.conflictos().len(),
        0,
        "el móvil no guarda conflictos de elemento"
    );
    assert_eq!(
        movil.registros("inventario_conflicto"),
        0,
        "tampoco los anota en REGISTRO"
    );
}

#[test]
fn el_lector_llega_al_mismo_inventario_que_un_escritorio() {
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = ClaveSincronizacion::generar().texto().to_string();
    let a = Entorno::nuevo(&carpeta, "omarchy", "linux", &clave);
    let b = Entorno::nuevo(&carpeta, "mac-mini", "macos", &clave);
    a.crear_host("web", "192.0.2.5");
    let id_host_a = a.host_por_nombre("web").expect("el host existe").id;
    a.crear_tunel(id_host_a, "web-local");
    a.exportar();
    b.crear_host("db", "192.0.2.6");
    b.exportar();

    // El móvil (lector) y un escritorio limpio leen exactamente los mismos
    // ficheros: la fusión es determinista y tienen que llegar al mismo
    // inventario.
    let movil = Entorno::nuevo(&carpeta, "pixel", "android", &clave);
    let lector = movil.pasada(&SinTrabajo);
    assert_eq!(lector.estado, EstadoLectura::AlDia);

    let escritorio = Entorno::nuevo(&carpeta, "otro", "linux", &clave);
    let escritor = escritorio.pasada_escritora();
    assert_eq!(escritor.estado, EstadoPasada::AlDia);

    assert_eq!(
        retrato_inventario(movil.conexion()),
        retrato_inventario(escritorio.conexion()),
        "con los mismos ficheros, el móvil llega al mismo inventario que un escritorio"
    );
}

#[test]
fn la_carpeta_es_entrada_no_fiable_y_el_lector_solo_lee_lo_que_magi_escribe() {
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = ClaveSincronizacion::generar().texto().to_string();
    let escritorio = Entorno::nuevo(&carpeta, "omarchy", "linux", &clave);
    let movil = Entorno::nuevo(&carpeta, "pixel", "android", &clave);
    escritorio.crear_host("web", "192.0.2.5");
    escritorio.exportar();
    let valido = carpeta::nombre_fichero(&escritorio.uuid_este());
    let uuid_mayusculas = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
    let uuid_ajeno = "11111111-1111-4111-8111-111111111111";

    // Basura que un tercero (o la herramienta de sincronización) puede dejar
    // en la carpeta: copia en conflicto, temporal, uuid en mayúsculas,
    // subcarpeta con nombre canónico y un fichero enorme (sparse, sin ocupar
    // disco). Ninguno debe importarse ni leerse.
    std::fs::copy(
        carpeta.join(&valido),
        carpeta.join(format!("{valido}.sync-conflict-20260101-000000")),
    )
    .expect("copia en conflicto");
    std::fs::copy(
        carpeta.join(&valido),
        carpeta.join(format!(".{valido}.tmp")),
    )
    .expect("temporal");
    std::fs::copy(
        carpeta.join(&valido),
        carpeta.join(format!("magi-{}.age", uuid_mayusculas.to_uppercase())),
    )
    .expect("uuid en mayúsculas");
    std::fs::create_dir_all(carpeta.join(carpeta::nombre_fichero(uuid_ajeno))).expect("subcarpeta");
    let enorme = carpeta.join(carpeta::nombre_fichero(
        "22222222-2222-4222-8222-222222222222",
    ));
    std::fs::File::create(&enorme)
        .expect("creando el fichero enorme")
        .set_len(carpeta::TAMANO_MAXIMO + 1)
        .expect("fijando el tamaño enorme");

    let servicio = CarpetaCruda::nueva(&carpeta);
    let antes = retrato_carpeta(&carpeta);
    let resultado = movil.pasada_con(&servicio, &SinTrabajo);
    assert_eq!(resultado.estado, EstadoLectura::AlDia);
    assert_eq!(movil.hosts().len(), 1, "solo se importa el fichero válido");
    assert_eq!(
        movil
            .host_por_nombre("web")
            .expect("host importado")
            .direccion,
        "192.0.2.5"
    );
    let leidos = servicio.leidos();
    assert!(
        !leidos.iter().any(|nombre| nombre.contains("sync-conflict")),
        "no se lee una copia en conflicto: {leidos:?}"
    );
    assert!(
        !leidos.iter().any(|nombre| nombre.starts_with(".magi-")),
        "no se lee un temporal: {leidos:?}"
    );
    assert!(
        !leidos
            .iter()
            .any(|nombre| nombre.contains(&uuid_mayusculas.to_uppercase())),
        "no se lee un uuid no canónico: {leidos:?}"
    );
    assert!(
        !leidos
            .iter()
            .any(|nombre| nombre == "magi-22222222-2222-4222-8222-222222222222.age"),
        "no se lee un fichero por encima del tamaño máximo: {leidos:?}"
    );
    assert!(
        resultado
            .avisos
            .iter()
            .any(|aviso| aviso.contains("máximo") || aviso.contains("T81")),
        "el fichero enorme se avisa: {:?}",
        resultado.avisos
    );
    assert_eq!(
        retrato_carpeta(&carpeta),
        antes,
        "la pasada no escribe nada en la carpeta, ni siquiera un temporal"
    );
    assert_eq!(resultado.conflictos_pendientes, 0);

    // La implementación real del escritorio ni siquiera lista los nombres que
    // no son de MAGI ni los enlaces simbólicos.
    let servicio_disco = CarpetaDisco::nueva(&carpeta);
    let listados: Vec<String> = servicio_disco
        .listar()
        .expect("listando la carpeta")
        .into_iter()
        .map(|entrada| entrada.nombre)
        .collect();
    assert_eq!(
        listados,
        vec![valido],
        "el servicio de disco solo lista ficheros de equipo canónicos"
    );
}

#[test]
fn sin_la_clave_nueva_queda_esperando_clave_y_se_recupera_al_escanearla() {
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = ClaveSincronizacion::generar().texto().to_string();
    let escritorio = Entorno::nuevo(&carpeta, "omarchy", "linux", &clave);
    let movil = Entorno::nuevo(&carpeta, "pixel", "android", &clave);
    escritorio.crear_host("web", "192.0.2.5");
    escritorio.exportar();
    movil.pasada(&SinTrabajo);
    assert_eq!(movil.hosts().len(), 1);

    // El escritorio rota la clave: el móvil pasa a esperando_clave y conserva
    // el inventario.
    let clave_nueva = ClaveSincronizacion::generar();
    escritorio.exportar_con(&clave_nueva);
    let esperando = movil.pasada(&SinTrabajo);
    assert_eq!(
        esperando.estado,
        EstadoLectura::EsperandoClave("omarchy".to_string())
    );
    assert_eq!(movil.hosts().len(), 1, "el inventario se queda como está");
    assert_eq!(movil.estado_equipo(&escritorio.uuid_este()), "ilegible");

    // Al escanear la clave nueva, vuelve sola a activa y aplica los cambios.
    movil.escribir_llavero(&clave_nueva.texto());
    escritorio.editar_host("web", "192.0.2.77");
    escritorio.exportar_con(&clave_nueva);
    let recuperada = movil.pasada(&SinTrabajo);
    assert_eq!(recuperada.estado, EstadoLectura::AlDia);
    assert_eq!(
        movil
            .host_por_nombre("web")
            .expect("el host existe")
            .direccion,
        "192.0.2.77"
    );
    assert_eq!(movil.estado_equipo(&escritorio.uuid_este()), "al_dia");
}

#[test]
fn el_lector_lee_el_formato_2_y_guarda_los_escritorios() {
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = ClaveSincronizacion::generar().texto().to_string();
    let escritorio = Entorno::nuevo(&carpeta, "omarchy", "linux", &clave);
    let movil = Entorno::nuevo(&carpeta, "pixel", "android", &clave);

    // Inventario del escritorio: un host SSH, un RDP y un VNC.
    escritorio.crear_host("web", "192.0.2.10");
    let opciones = OpcionesEscritorio {
        tamano: TamanoEscritorio::Fijo {
            ancho: 1600,
            alto: 900,
        },
        ..OpcionesEscritorio::default()
    };
    escritorio
        .almacen
        .crear_host(
            &DatosHost {
                nombre: "win-01".to_string(),
                direccion: "192.0.2.30".to_string(),
                puerto: 3389,
                tipo: TipoHost::Rdp,
                escritorio_dominio: Some("EJEMPLO".to_string()),
                escritorio_opciones: opciones.clone(),
                usuario: Some("operador".to_string()),
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .expect("creando el host RDP");
    escritorio
        .almacen
        .crear_host(
            &DatosHost {
                nombre: "vm-01".to_string(),
                direccion: "192.0.2.40".to_string(),
                puerto: 5900,
                tipo: TipoHost::Vnc,
                escritorio_opciones: opciones,
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .expect("creando el host VNC");
    escritorio.exportar();

    // El fichero del equipo con escritorios ha subido a formato 2.
    let ruta = carpeta.join(carpeta::nombre_fichero(&escritorio.uuid_este()));
    let bytes = std::fs::read(&ruta).expect("leyendo el fichero del escritorio");
    let plano = escritorio
        .clave()
        .descifrar(&bytes)
        .expect("descifrando el fichero del escritorio");
    let en_claro = formato::separar(&plano).expect("separando el fichero del escritorio");
    assert_eq!(en_claro.formato, formato::FORMATO_MAXIMO);

    let resultado = movil.pasada(&SinTrabajo);
    assert_eq!(resultado.estado, EstadoLectura::AlDia);
    assert_eq!(
        resultado.resumen.as_ref().expect("hay resumen").creados,
        3,
        "el móvil importa los tres hosts del formato 2"
    );
    let hosts = movil.hosts();
    assert_eq!(hosts.len(), 3);
    let por_nombre = |nombre: &str| {
        hosts
            .iter()
            .find(|host| host.nombre == nombre)
            .unwrap_or_else(|| panic!("falta el host «{nombre}»: {hosts:?}"))
    };
    assert_eq!(por_nombre("web").tipo, TipoHost::Ssh);
    let rdp = por_nombre("win-01");
    assert_eq!(rdp.tipo, TipoHost::Rdp);
    assert_eq!(rdp.escritorio_dominio.as_deref(), Some("EJEMPLO"));
    assert_eq!(
        rdp.escritorio_opciones.tamano,
        TamanoEscritorio::Fijo {
            ancho: 1600,
            alto: 900
        }
    );
    assert_eq!(por_nombre("vm-01").tipo, TipoHost::Vnc);
    // El móvil no resuelve conflictos de elemento, así que tampoco guarda
    // ninguno: para él, los escritorios son inventario de solo lectura.
    assert_eq!(resultado.conflictos_pendientes, 0);
    assert!(movil.conflictos().is_empty());
}

#[test]
fn sin_clave_no_toca_nada_y_lo_dice() {
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = ClaveSincronizacion::generar().texto().to_string();
    let escritorio = Entorno::nuevo(&carpeta, "omarchy", "linux", &clave);
    escritorio.crear_host("web", "192.0.2.5");
    escritorio.exportar();
    let movil = Entorno::nuevo(&carpeta, "pixel", "android", &clave);
    // Sin clave en el llavero: la pasada no puede descifrar nada.
    movil
        .llavero
        .olvidar(CUENTA_SINCRONIZACION)
        .expect("olvidando la clave");
    let resultado = movil.pasada(&SinTrabajo);
    assert_eq!(resultado.estado, EstadoLectura::SinClave);
    assert_eq!(movil.hosts().len(), 0);
    assert_eq!(resultado.conflictos_pendientes, 0);
}
