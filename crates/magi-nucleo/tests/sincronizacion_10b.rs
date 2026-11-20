//! Corrección 10b de la sincronización (Fase 9b), en el núcleo:
//!
//! - R85: solo «ninguna identidad coincide» pide la clave; un fichero cortado,
//!   vacío o que no es age se avisa con su nombre, no para la pasada y se
//!   reintenta en la siguiente (pasada en modo escritor y en modo lector).
//! - R97: la clave anterior solo abre ficheros de equipos conocidos y no
//!   quitados; el de un equipo desconocido se rechaza con aviso, sin crear fila.
//! - R84: el cambio de un elemento que viaja y su versión nueva van en la misma
//!   transacción, en todas las tablas que viajan: un fallo provocado entre los
//!   dos pasos no deja ningún cambio sin su versión.
//! - R83 (parte del núcleo): la pasada dice qué borrados aplazó y el estado los
//!   lista con su motivo.

use std::path::{Path, PathBuf};

use rusqlite::{types::ValueRef, Connection};

use magi_nucleo::almacen::versiones::{self, Tabla};
use magi_nucleo::almacen::{
    comprobaciones_servicio, conflictos, equipos as equipos_bd, grupos, hosts, identidades,
    sesiones_persistentes, sincronizaciones, snippets, tuneles, verificaciones, Almacen,
};
use magi_nucleo::configuracion::SeccionSincronizacion;
use magi_nucleo::deliberacion::DatosVerificaciones;
use magi_nucleo::modelo::{
    DatosHost, DatosSincronizacion, DatosTunel, Direccion, IdentidadRef, Origen, OrigenIdentidad,
    TipoTunel,
};
use magi_nucleo::rutas::{Equipo, Rutas};
use magi_nucleo::servicios::TipoObjetivo;
use magi_nucleo::sesiones_persistentes::{DatosFicha, OrigenFicha};
use magi_nucleo::sincronizacion::carpeta::{self, CarpetaDisco};
use magi_nucleo::sincronizacion::cifrado::ClaveSincronizacion;
use magi_nucleo::sincronizacion::lector::{self, EstadoLectura, ResultadoLectura};
use magi_nucleo::sincronizacion::{
    self, documento_propio, formato, EstadoPasada, OpcionesPasada, PoliticaBorrados,
    ResultadoPasada, MOTIVO_RECHAZADO,
};
use magi_nucleo::snippets::{DatosSnippet, Destino};
use magi_nucleo::trabajo_vivo::TrabajoVivo;

// ------------------------------------------------------------------ maqueta

/// Un MAGI de prueba: su magi.db (en fichero, para poder reabrirla), sus rutas y
/// la carpeta compartida.
struct Maqueta {
    _temporal: tempfile::TempDir,
    rutas: Rutas,
    almacen: Almacen,
    carpeta: PathBuf,
}

impl Maqueta {
    fn nueva(carpeta: &Path, nombre: &str) -> Self {
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
            &Equipo::nuevo(nombre, "linux"),
            "1.6.1",
        )
        .expect("creando magi.db");
        Self {
            _temporal: temporal,
            rutas,
            almacen,
            carpeta: carpeta.to_path_buf(),
        }
    }

    fn conexion(&self) -> &Connection {
        self.almacen.conexion()
    }

    fn uuid(&self) -> String {
        equipos_bd::este(self.conexion())
            .expect("leyendo este equipo")
            .uuid
    }

    fn nombre_fichero(&self) -> String {
        carpeta::nombre_fichero(&self.uuid())
    }

    fn ruta_fichero(&self) -> PathBuf {
        self.carpeta.join(self.nombre_fichero())
    }

    fn crear_host(&self, nombre: &str) -> String {
        let id = hosts::crear(
            self.conexion(),
            &DatosHost {
                nombre: nombre.to_string(),
                direccion: "192.0.2.10".to_string(),
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .expect("creando el host");
        self.conexion()
            .query_row("SELECT uuid FROM HOSTS WHERE id = ?1", [id], |fila| {
                fila.get(0)
            })
            .expect("uuid del host")
    }

    fn borrar_host(&self, nombre: &str) {
        let id: i64 = self
            .conexion()
            .query_row("SELECT id FROM HOSTS WHERE nombre = ?1", [nombre], |fila| {
                fila.get(0)
            })
            .expect("el host que se borra no existe");
        hosts::borrar(self.conexion(), id).expect("borrando el host");
    }

    fn tiene_host(&self, nombre: &str) -> bool {
        self.conexion()
            .query_row(
                "SELECT COUNT(*) FROM HOSTS WHERE nombre = ?1",
                [nombre],
                |fila| fila.get::<_, i64>(0),
            )
            .expect("contando hosts")
            > 0
    }

    /// Escribe el fichero de este equipo en la carpeta, cifrado con `clave`.
    fn exportar(&self, clave: &ClaveSincronizacion) {
        let documento = documento_propio(self.conexion(), &self.rutas, "1.6.1")
            .expect("construyendo el documento propio");
        let json = formato::serializar(&documento).expect("serializando");
        let firma = clave.firmar(json.as_bytes()).expect("firmando");
        let plano = formato::componer(&json, &firma, documento.formato);
        let cifrado = clave.cifrar(&plano).expect("cifrando");
        carpeta::escribir(&self.carpeta, &documento.equipo.uuid, &cifrado)
            .expect("escribiendo el fichero");
    }

    /// Este equipo ya conoce a otro (tiene su fila en EQUIPOS).
    fn conocer(&self, otro: &Maqueta, nombre: &str) {
        equipos_bd::crear(self.conexion(), &otro.uuid(), nombre, "linux")
            .expect("dando de alta al equipo");
    }

    fn estado_de(&self, uuid: &str) -> Option<String> {
        equipos_bd::por_uuid(self.conexion(), uuid)
            .expect("buscando el equipo")
            .map(|equipo| equipo.estado)
    }

    fn hash_de(&self, uuid: &str) -> Option<String> {
        equipos_bd::por_uuid(self.conexion(), uuid)
            .expect("buscando el equipo")
            .and_then(|equipo| equipo.hash_ultimo_fichero)
    }

    fn pasada(&self, claves: &[ClaveSincronizacion]) -> ResultadoPasada {
        self.pasada_con(claves, &|_, _| false)
    }

    fn pasada_con(
        &self,
        claves: &[ClaveSincronizacion],
        trabajo_vivo: &dyn Fn(&str, &str) -> bool,
    ) -> ResultadoPasada {
        self.pasada_con_politica(claves, PoliticaBorrados::Preguntar, trabajo_vivo)
    }

    fn pasada_con_politica(
        &self,
        claves: &[ClaveSincronizacion],
        politica: PoliticaBorrados,
        trabajo_vivo: &dyn Fn(&str, &str) -> bool,
    ) -> ResultadoPasada {
        self.intentar_pasada(claves, politica, trabajo_vivo)
            .expect("la pasada en modo escritor falla")
    }

    fn intentar_pasada(
        &self,
        claves: &[ClaveSincronizacion],
        politica: PoliticaBorrados,
        trabajo_vivo: &dyn Fn(&str, &str) -> bool,
    ) -> anyhow::Result<ResultadoPasada> {
        let ajustes = SeccionSincronizacion {
            carpeta: Some(self.carpeta.to_string_lossy().to_string()),
            umbral_borrado_hosts: 1000,
            umbral_borrado_pct: 1000,
            ..SeccionSincronizacion::default()
        };
        let opciones = OpcionesPasada {
            politica_borrados: politica,
            trabajo_vivo,
            al_aplicar: None,
            copia_previa: None,
        };
        sincronizacion::pasada(
            self.conexion(),
            &self.rutas,
            &ajustes,
            "1.6.1",
            claves,
            &opciones,
        )
    }

    fn pasada_lector(&self, claves: &[ClaveSincronizacion]) -> ResultadoLectura {
        lector::pasada_lector(
            self.conexion(),
            &self.rutas,
            &CarpetaDisco::nueva(&self.carpeta),
            claves,
            &SinTrabajo,
        )
        .expect("la pasada en modo lector falla")
    }
}

struct SinTrabajo;

impl TrabajoVivo for SinTrabajo {
    fn tiene_trabajo(&self, _tipo: &str, _uuid: &str) -> bool {
        false
    }
}

fn carpeta_compartida() -> (tempfile::TempDir, PathBuf) {
    let temporal = tempfile::tempdir().expect("directorio temporal");
    let carpeta = temporal.path().join("sync");
    std::fs::create_dir_all(&carpeta).expect("creando la carpeta compartida");
    (temporal, carpeta)
}

fn clave_nueva() -> ClaveSincronizacion {
    ClaveSincronizacion::generar()
}

/// Otra copia de la misma clave (la clave no es `Clone`).
fn copia(clave: &ClaveSincronizacion) -> ClaveSincronizacion {
    ClaveSincronizacion::desde_texto(&clave.texto()).expect("clave de prueba")
}

/// Un fichero de la carpeta en el estado en que lo deja una copia a medias.
type Corrupcion = (&'static str, fn(&[u8]) -> Vec<u8>);

const CORRUPCIONES: [Corrupcion; 4] = [
    ("cortado por la mitad", |bytes| {
        bytes[..bytes.len() / 2].to_vec()
    }),
    ("cortado dentro de la cabecera", |bytes| {
        bytes[..30].to_vec()
    }),
    ("vacío", |_| Vec::new()),
    ("bytes que no son age", |_| {
        b"esto no es un fichero age".to_vec()
    }),
];

fn aviso_de_dañado(uuid: &str) -> String {
    format!(
        "{}: a medio copiar o dañado; se reintenta en la siguiente pasada",
        carpeta::nombre_fichero(uuid)
    )
}

// ------------------------------------------------------------------- R85

/// Un fichero que no se lee por una causa que no es la clave: se importan los
/// demás, sale el aviso con el nombre del fichero, no se pide ninguna clave y se
/// reintenta en la siguiente pasada. `conocido`: el equipo ya tiene su fila.
fn comprobar_danado(corrupcion: &Corrupcion, conocido: bool, en_lector: bool) {
    let (nombre_corrupcion, corromper) = corrupcion;
    let contexto = format!(
        "{nombre_corrupcion} · {} · {}",
        if conocido { "conocido" } else { "desconocido" },
        if en_lector { "lector" } else { "escritor" }
    );
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = clave_nueva();
    let sano = Maqueta::nueva(&carpeta, "sano");
    sano.crear_host("web-sano");
    sano.exportar(&clave);
    let roto = Maqueta::nueva(&carpeta, "roto");
    roto.crear_host("web-roto");
    roto.exportar(&clave);
    let bueno = std::fs::read(roto.ruta_fichero()).expect("leyendo el fichero");
    std::fs::write(roto.ruta_fichero(), corromper(&bueno)).expect("cortando el fichero");
    let este = Maqueta::nueva(&carpeta, "este");
    if conocido {
        este.conocer(&roto, "roto");
    }
    let claves = [copia(&clave)];

    let (al_dia, avisos) = if en_lector {
        let resultado = este.pasada_lector(&claves);
        (
            resultado.estado == EstadoLectura::AlDia,
            resultado.avisos.clone(),
        )
    } else {
        let resultado = este.pasada(&claves);
        (resultado.estado == EstadoPasada::AlDia, resultado.avisos)
    };
    assert!(al_dia, "{contexto}: no se pide ninguna clave ni se para");
    assert!(
        este.tiene_host("web-sano"),
        "{contexto}: se importan los demás ficheros"
    );
    assert!(!este.tiene_host("web-roto"), "{contexto}");
    assert!(
        avisos.contains(&aviso_de_dañado(&roto.uuid())),
        "{contexto}: falta el aviso con el nombre del fichero: {avisos:?}"
    );
    if conocido {
        assert_eq!(
            este.estado_de(&roto.uuid()).as_deref(),
            Some(equipos_bd::ESTADO_DANADO),
            "{contexto}"
        );
    } else {
        assert_eq!(este.estado_de(&roto.uuid()), None, "{contexto}: sin fila");
    }
    assert_eq!(
        este.hash_de(&roto.uuid()),
        None,
        "{contexto}: un fichero que no se lee no guarda hash"
    );

    // La copia termina: la siguiente pasada lo importa sin que nadie toque nada.
    roto.exportar(&clave);
    let avisos = if en_lector {
        let resultado = este.pasada_lector(&claves);
        assert_eq!(resultado.estado, EstadoLectura::AlDia, "{contexto}");
        resultado.avisos
    } else {
        let resultado = este.pasada(&claves);
        assert_eq!(resultado.estado, EstadoPasada::AlDia, "{contexto}");
        resultado.avisos
    };
    assert!(avisos.is_empty(), "{contexto}: {avisos:?}");
    assert!(este.tiene_host("web-roto"), "{contexto}: se reintenta");
    assert_eq!(
        este.estado_de(&roto.uuid()).as_deref(),
        Some(equipos_bd::ESTADO_AL_DIA),
        "{contexto}"
    );
}

#[test]
fn un_fichero_cortado_vacio_o_que_no_es_age_se_avisa_sin_pedir_clave_y_se_reintenta() {
    for corrupcion in &CORRUPCIONES {
        for conocido in [false, true] {
            for en_lector in [false, true] {
                comprobar_danado(corrupcion, conocido, en_lector);
            }
        }
    }
}

#[test]
fn un_fichero_que_vuelve_a_ser_el_ya_importado_deja_de_estar_danado() {
    // Se copió a medias y luego se terminó de copiar con el mismo contenido que
    // ya se había importado: no cambia su hash, pero el equipo no se queda
    // marcado como dañado para siempre.
    for en_lector in [false, true] {
        let (_temporal, carpeta) = carpeta_compartida();
        let clave = clave_nueva();
        let otro = Maqueta::nueva(&carpeta, "otro");
        otro.crear_host("web-otro");
        otro.exportar(&clave);
        let este = Maqueta::nueva(&carpeta, "este");
        let claves = [copia(&clave)];
        let pasar = || {
            if en_lector {
                este.pasada_lector(&claves).avisos
            } else {
                este.pasada(&claves).avisos
            }
        };
        assert!(pasar().is_empty());
        assert_eq!(
            este.estado_de(&otro.uuid()).as_deref(),
            Some(equipos_bd::ESTADO_AL_DIA)
        );

        let bueno = std::fs::read(otro.ruta_fichero()).expect("leyendo el fichero");
        std::fs::write(otro.ruta_fichero(), &bueno[..bueno.len() / 2]).expect("cortando");
        assert_eq!(pasar(), vec![aviso_de_dañado(&otro.uuid())]);
        assert_eq!(
            este.estado_de(&otro.uuid()).as_deref(),
            Some(equipos_bd::ESTADO_DANADO)
        );

        std::fs::write(otro.ruta_fichero(), &bueno).expect("restaurando");
        assert!(pasar().is_empty(), "lector: {en_lector}");
        assert_eq!(
            este.estado_de(&otro.uuid()).as_deref(),
            Some(equipos_bd::ESTADO_AL_DIA),
            "lector: {en_lector}"
        );
    }
}

#[test]
fn un_fichero_de_otra_clave_sigue_pidiendo_la_clave_con_o_sin_clave_anterior() {
    for en_lector in [false, true] {
        let (_temporal, carpeta) = carpeta_compartida();
        let clave = clave_nueva();
        let anterior = clave_nueva();
        let ajena = clave_nueva();
        let otro = Maqueta::nueva(&carpeta, "otro");
        otro.crear_host("web-otro");
        otro.exportar(&ajena);
        let este = Maqueta::nueva(&carpeta, "este");
        este.conocer(&otro, "otro");

        // Sin clave anterior y con una anterior que tampoco lo abre.
        for claves in [vec![copia(&clave)], vec![copia(&clave), copia(&anterior)]] {
            let esperando = if en_lector {
                matches!(
                    este.pasada_lector(&claves).estado,
                    EstadoLectura::EsperandoClave(_)
                )
            } else {
                matches!(este.pasada(&claves).estado, EstadoPasada::EsperandoClave(_))
            };
            assert!(
                esperando,
                "otra clave sigue pidiendo la clave ({} claves, lector: {en_lector})",
                claves.len()
            );
            assert_eq!(
                este.estado_de(&otro.uuid()).as_deref(),
                Some(equipos_bd::ESTADO_ILEGIBLE)
            );
            assert!(!este.tiene_host("web-otro"));
        }
    }
}

#[test]
fn un_fichero_de_otra_clave_de_un_equipo_desconocido_solo_avisa() {
    for en_lector in [false, true] {
        let (_temporal, carpeta) = carpeta_compartida();
        let clave = clave_nueva();
        let ajena = clave_nueva();
        let otro = Maqueta::nueva(&carpeta, "otro");
        otro.crear_host("web-otro");
        otro.exportar(&ajena);
        let este = Maqueta::nueva(&carpeta, "este");
        let claves = [copia(&clave)];
        let avisos = if en_lector {
            let resultado = este.pasada_lector(&claves);
            assert_eq!(resultado.estado, EstadoLectura::AlDia);
            resultado.avisos
        } else {
            let resultado = este.pasada(&claves);
            assert_eq!(resultado.estado, EstadoPasada::AlDia);
            assert!(resultado.rechazados.is_empty(), "no es un rechazo (R97)");
            resultado.avisos
        };
        assert!(
            avisos
                .iter()
                .any(|aviso| aviso.contains(&otro.nombre_fichero())
                    && aviso.contains("no se abre con esta clave")),
            "{avisos:?}"
        );
        assert_eq!(este.estado_de(&otro.uuid()), None);
    }
}

// ------------------------------------------------------------------- R97

#[test]
fn la_clave_anterior_abre_el_fichero_de_un_equipo_conocido() {
    for en_lector in [false, true] {
        let (_temporal, carpeta) = carpeta_compartida();
        let nueva = clave_nueva();
        let vieja = clave_nueva();
        let otro = Maqueta::nueva(&carpeta, "otro");
        otro.crear_host("web-otro");
        otro.exportar(&vieja);
        let este = Maqueta::nueva(&carpeta, "este");
        este.conocer(&otro, "otro");
        let claves = [copia(&nueva), copia(&vieja)];
        if en_lector {
            assert_eq!(este.pasada_lector(&claves).estado, EstadoLectura::AlDia);
        } else {
            let resultado = este.pasada(&claves);
            assert_eq!(resultado.estado, EstadoPasada::AlDia);
            assert!(resultado.rechazados.is_empty());
        }
        assert!(
            este.tiene_host("web-otro"),
            "el equipo conocido se abre con la clave anterior (lector: {en_lector})"
        );
        assert_eq!(
            este.estado_de(&otro.uuid()).as_deref(),
            Some(equipos_bd::ESTADO_AL_DIA)
        );
    }
}

#[test]
fn la_clave_anterior_rechaza_el_fichero_de_un_equipo_desconocido_sin_crear_fila() {
    for en_lector in [false, true] {
        let (_temporal, carpeta) = carpeta_compartida();
        let nueva = clave_nueva();
        let vieja = clave_nueva();
        // Un equipo desconocido con la clave anterior y otro, también
        // desconocido, con la nueva: este se importa, aquel se rechaza.
        let intruso = Maqueta::nueva(&carpeta, "intruso");
        intruso.crear_host("web-intruso");
        intruso.exportar(&vieja);
        let legitimo = Maqueta::nueva(&carpeta, "legitimo");
        legitimo.crear_host("web-legitimo");
        legitimo.exportar(&nueva);
        let este = Maqueta::nueva(&carpeta, "este");
        let claves = [copia(&nueva), copia(&vieja)];
        let aviso = format!(
            "{}: rechazado: {MOTIVO_RECHAZADO}",
            intruso.nombre_fichero()
        );
        let avisos = if en_lector {
            let resultado = este.pasada_lector(&claves);
            assert_eq!(resultado.estado, EstadoLectura::AlDia, "no para la pasada");
            resultado.avisos
        } else {
            let resultado = este.pasada(&claves);
            assert_eq!(resultado.estado, EstadoPasada::AlDia, "no para la pasada");
            assert_eq!(resultado.rechazados, vec![intruso.nombre_fichero()]);
            resultado.avisos
        };
        assert!(avisos.contains(&aviso), "{avisos:?}");
        assert!(!este.tiene_host("web-intruso"), "no importa nada");
        assert!(este.tiene_host("web-legitimo"), "los demás sí");
        assert_eq!(
            este.estado_de(&intruso.uuid()),
            None,
            "no crea fila en EQUIPOS (lector: {en_lector})"
        );
        // El texto dice por qué.
        assert!(aviso.contains("de un equipo que este no conocía"));
    }
}

#[test]
fn la_clave_anterior_no_abre_el_fichero_de_un_equipo_quitado() {
    for en_lector in [false, true] {
        let (_temporal, carpeta) = carpeta_compartida();
        let nueva = clave_nueva();
        let vieja = clave_nueva();
        let quitado = Maqueta::nueva(&carpeta, "quitado");
        quitado.crear_host("web-quitado");
        quitado.exportar(&vieja);
        let este = Maqueta::nueva(&carpeta, "este");
        este.conocer(&quitado, "quitado");
        let id = equipos_bd::por_uuid(este.conexion(), &quitado.uuid())
            .expect("buscando")
            .expect("existe")
            .id;
        equipos_bd::marcar_quitado(este.conexion(), id, "2026-11-05T09:00:00+01:00")
            .expect("quitando el equipo");
        let claves = [copia(&nueva), copia(&vieja)];
        let avisos = if en_lector {
            let resultado = este.pasada_lector(&claves);
            assert_eq!(resultado.estado, EstadoLectura::AlDia);
            resultado.avisos
        } else {
            let resultado = este.pasada(&claves);
            assert_eq!(resultado.estado, EstadoPasada::AlDia);
            resultado.avisos
        };
        assert!(
            !este.tiene_host("web-quitado"),
            "se salta (lector: {en_lector})"
        );
        assert!(avisos.is_empty(), "{avisos:?}");
        assert_eq!(
            este.estado_de(&quitado.uuid()).as_deref(),
            Some(equipos_bd::ESTADO_QUITADO)
        );
    }
}

#[test]
fn con_la_clave_anterior_caducada_el_fichero_de_la_rotacion_pide_la_clave() {
    // Pasados los 30 días, quien lee ya no recibe la clave anterior: es la
    // lista de una sola clave. El equipo conocido pide la clave nueva y el
    // desconocido solo se avisa, sin llamarlo «rechazado».
    for en_lector in [false, true] {
        let (_temporal, carpeta) = carpeta_compartida();
        let nueva = clave_nueva();
        let vieja = clave_nueva();
        let conocido = Maqueta::nueva(&carpeta, "conocido");
        conocido.exportar(&vieja);
        let este = Maqueta::nueva(&carpeta, "este");
        este.conocer(&conocido, "conocido");
        let claves = [copia(&nueva)];
        let esperando = if en_lector {
            matches!(
                este.pasada_lector(&claves).estado,
                EstadoLectura::EsperandoClave(_)
            )
        } else {
            matches!(este.pasada(&claves).estado, EstadoPasada::EsperandoClave(_))
        };
        assert!(esperando, "caducada: pide la clave (lector: {en_lector})");

        // Un equipo desconocido cifrado con la clave anterior caducada: solo
        // se avisa, sin llamarlo «rechazado» (no hay clave anterior vigente).
        let (_temporal_2, carpeta_2) = carpeta_compartida();
        let desconocido = Maqueta::nueva(&carpeta_2, "desconocido");
        desconocido.exportar(&vieja);
        let este_2 = Maqueta::nueva(&carpeta_2, "este-2");
        let avisos = if en_lector {
            este_2.pasada_lector(&claves).avisos
        } else {
            let resultado = este_2.pasada(&claves);
            assert!(resultado.rechazados.is_empty());
            resultado.avisos
        };
        assert!(
            avisos.iter().all(|aviso| !aviso.contains("rechazado")),
            "{avisos:?}"
        );
        assert!(
            avisos
                .iter()
                .any(|aviso| aviso.contains("no se abre con esta clave")),
            "{avisos:?}"
        );
    }
}

// ------------------------------------------------------------------- R83

#[test]
fn la_pasada_dice_que_borrados_aplazo_y_que_equipos_importo() {
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = clave_nueva();
    let a = Maqueta::nueva(&carpeta, "a");
    let b = Maqueta::nueva(&carpeta, "b");
    let uuid_web = a.crear_host("web-01");
    a.crear_host("db-01");
    a.exportar(&clave);
    let resultado = b.pasada(&[copia(&clave)]);
    assert_eq!(resultado.estado, EstadoPasada::AlDia);
    assert_eq!(resultado.equipos_importados, vec!["a".to_string()]);
    assert!(resultado.aplazados.is_empty());
    assert!(b.tiene_host("web-01"));

    // A borra web-01 y b tiene trabajo vivo en él: se aplaza.
    a.borrar_host("web-01");
    a.exportar(&clave);
    let resultado = b.pasada_con(&[copia(&clave)], &|tipo, uuid| {
        tipo == "host" && uuid == uuid_web
    });
    assert_eq!(resultado.estado, EstadoPasada::AlDia);
    assert_eq!(
        resultado.aplazados,
        vec![sincronizacion::BorradoAplazado {
            tipo: "host".to_string(),
            uuid: uuid_web.clone(),
            nombre: "web-01".to_string(),
        }]
    );
    assert!(
        b.tiene_host("web-01"),
        "el host con trabajo vivo no se borra"
    );
    assert!(b.tiene_host("db-01"));

    // Una pasada sin cambios no repite el aplazado y cuenta al equipo al día.
    let resultado = b.pasada(&[copia(&clave)]);
    assert!(resultado.aplazados.is_empty());
    assert_eq!(resultado.equipos_al_dia, vec!["a".to_string()]);
}

#[test]
fn el_estado_lista_los_aplazados_con_su_motivo_y_no_los_cuenta_como_conflictos() {
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = clave_nueva();
    let a = Maqueta::nueva(&carpeta, "a");
    let b = Maqueta::nueva(&carpeta, "b");
    a.crear_host("web-01");
    a.exportar(&clave);
    b.pasada(&[copia(&clave)]);
    a.borrar_host("web-01");
    a.exportar(&clave);
    b.pasada_con(&[copia(&clave)], &|tipo, _| tipo == "host");

    let ajustes = SeccionSincronizacion {
        carpeta: Some(carpeta.to_string_lossy().to_string()),
        ..SeccionSincronizacion::default()
    };
    let texto =
        sincronizacion::estado_texto(b.conexion(), &ajustes, &b.rutas.hogar, &|conflicto| {
            assert_eq!(conflicto.tipo, conflictos::TIPO_BORRADO);
            format!("motivo de {}", conflicto.nombre)
        })
        .expect("texto de estado");
    assert!(texto.contains("conflictos pendientes: 0"), "{texto}");
    assert!(texto.contains("borrados aplazados: 1"), "{texto}");
    assert!(texto.contains("host «web-01»: motivo de web-01"), "{texto}");
}

// ------------------------------------------------------------------- R84

/// Los ids de lo que crea cada prueba de transacción.
struct Ids {
    grupo: i64,
    host: i64,
    host_2: i64,
    identidad: i64,
    identidad_revocada: i64,
    tunel: i64,
    snippet: i64,
    sincronizacion: i64,
    ficha: i64,
}

/// Un inventario con un elemento de cada tabla que viaja.
fn inventario(conexion: &Connection, hogar: &Path) -> Ids {
    let grupo = grupos::crear(conexion, "produccion").expect("grupo");
    let host = hosts::crear(
        conexion,
        &DatosHost {
            nombre: "web-01".to_string(),
            direccion: "192.0.2.10".to_string(),
            etiquetas: vec!["web".to_string()],
            ..DatosHost::default()
        },
        Origen::Manual,
    )
    .expect("host");
    let host_2 = hosts::crear(
        conexion,
        &DatosHost {
            nombre: "db-01".to_string(),
            direccion: "192.0.2.11".to_string(),
            ..DatosHost::default()
        },
        Origen::Manual,
    )
    .expect("segundo host");
    let identidad = identidades::crear(
        conexion,
        "ed25519",
        "SHA256:huella-de-prueba-1",
        OrigenIdentidad::Fichero,
        Some("~/.ssh/id_prueba"),
        Some("clave de prueba"),
    )
    .expect("identidad");
    let identidad_revocada = identidades::crear(
        conexion,
        "ed25519",
        "SHA256:huella-de-prueba-2",
        OrigenIdentidad::Agente,
        None,
        Some("otra clave"),
    )
    .expect("identidad revocada");
    identidades::revocar(conexion, identidad_revocada, hogar).expect("revocando");
    let tunel = tuneles::crear(
        conexion,
        &DatosTunel {
            host_id: host,
            nombre: "web".to_string(),
            tipo: TipoTunel::Local,
            escucha: "127.0.0.1:8080".to_string(),
            destino: Some("192.0.2.20:80".to_string()),
            automatico: false,
        },
    )
    .expect("túnel");
    let snippet = snippets::crear(
        conexion,
        &DatosSnippet {
            nombre: "estado".to_string(),
            comando: "uptime".to_string(),
            destinos: vec![Destino::Etiqueta("web".to_string())],
            ..DatosSnippet::default()
        },
    )
    .expect("snippet");
    let sincronizacion = sincronizaciones::crear(
        conexion,
        &DatosSincronizacion {
            host_id: host,
            nombre: "datos".to_string(),
            ruta_local: "/home/usuario/datos".to_string(),
            ruta_remota: "/srv/datos".to_string(),
            direccion: Direccion::Subida,
            borrar: false,
            exclusiones: Vec::new(),
        },
    )
    .expect("sincronización de directorio");
    comprobaciones_servicio::guardar(
        conexion,
        host,
        TipoObjetivo::Unidad,
        "nginx.service",
        "systemctl is-active nginx",
    )
    .expect("comprobación");
    let ficha = sesiones_persistentes::crear(
        conexion,
        host,
        &DatosFicha {
            nombre: "agente".to_string(),
            carpeta: None,
            comando: None,
            comando_reanudar: None,
            nota: None,
            origen: OrigenFicha::Manual,
            creada_por: "equipo".to_string(),
        },
    )
    .expect("ficha");
    Ids {
        grupo,
        host,
        host_2,
        identidad,
        identidad_revocada,
        tunel,
        snippet,
        sincronizacion,
        ficha,
    }
}

/// Lo que hay en una tabla, columna a columna (incluye lo que no viaja, como
/// `actualizado_en`): cualquier cambio, aunque no sea de lo que viaja, se nota.
fn volcar(conexion: &Connection, tabla: &str) -> Vec<String> {
    let mut sentencia = conexion
        .prepare(&format!("SELECT * FROM {tabla} ORDER BY 1, 2"))
        .expect("preparando el volcado");
    let columnas = sentencia.column_count();
    let mut filas = sentencia.query([]).expect("volcando");
    let mut salida = Vec::new();
    while let Some(fila) = filas.next().expect("leyendo una fila") {
        let mut texto = Vec::new();
        for indice in 0..columnas {
            texto.push(match fila.get_ref(indice).expect("columna") {
                ValueRef::Null => "NULL".to_string(),
                ValueRef::Integer(valor) => valor.to_string(),
                ValueRef::Real(valor) => valor.to_string(),
                ValueRef::Text(bytes) => String::from_utf8_lossy(bytes).to_string(),
                ValueRef::Blob(bytes) => format!("<{} bytes>", bytes.len()),
            });
        }
        salida.push(texto.join("|"));
    }
    salida
}

const TABLAS_VOLCADAS: [&str; 15] = [
    "GRUPOS",
    "HOSTS",
    "IDENTIDADES",
    "TUNELES",
    "SNIPPETS",
    "SINCRONIZACIONES_DIR",
    "COMPROBACIONES_SERVICIO",
    "SESIONES_PERSISTENTES",
    "EQUIPOS",
    "LAPIDAS",
    "ETIQUETAS",
    "HOST_ETIQUETAS",
    "VERIFICACIONES_HOST",
    "SNIPPET_DESTINOS",
    "CONFLICTOS",
];

fn foto(conexion: &Connection) -> Vec<(String, Vec<String>)> {
    TABLAS_VOLCADAS
        .iter()
        .map(|tabla| ((*tabla).to_string(), volcar(conexion, tabla)))
        .collect()
}

/// Una operación del almacén sobre un elemento que viaja.
struct Operacion {
    nombre: &'static str,
    /// La tabla cuya versión tiene que subir.
    tabla: Tabla,
    ejecutar: fn(&Connection, &Ids) -> anyhow::Result<()>,
}

fn datos_de_host(host: &str, direccion: &str, etiquetas: &[&str]) -> DatosHost {
    DatosHost {
        nombre: host.to_string(),
        direccion: direccion.to_string(),
        etiquetas: etiquetas.iter().map(|texto| (*texto).to_string()).collect(),
        ..DatosHost::default()
    }
}

fn operaciones() -> Vec<Operacion> {
    vec![
        Operacion {
            nombre: "grupos::renombrar",
            tabla: Tabla::Grupos,
            ejecutar: |c, ids| grupos::renombrar(c, ids.grupo, "preproduccion"),
        },
        Operacion {
            nombre: "hosts::actualizar (con etiquetas)",
            tabla: Tabla::Hosts,
            ejecutar: |c, ids| {
                hosts::actualizar(
                    c,
                    ids.host,
                    &datos_de_host("web-01", "192.0.2.99", &["web", "nueva"]),
                )
            },
        },
        Operacion {
            nombre: "hosts::mover_a_grupo",
            tabla: Tabla::Hosts,
            ejecutar: |c, ids| hosts::mover_a_grupo(c, ids.host, Some(ids.grupo)),
        },
        Operacion {
            nombre: "hosts::marcar_origen",
            tabla: Tabla::Hosts,
            ejecutar: |c, ids| hosts::marcar_origen(c, ids.host, Origen::SshConfig),
        },
        Operacion {
            nombre: "hosts::fijar_salto",
            tabla: Tabla::Hosts,
            ejecutar: |c, ids| hosts::fijar_salto(c, ids.host, Some(ids.host_2)),
        },
        Operacion {
            nombre: "hosts::anadir_opciones_extra",
            tabla: Tabla::Hosts,
            ejecutar: |c, ids| hosts::anadir_opciones_extra(c, ids.host, "ForwardAgent no"),
        },
        Operacion {
            nombre: "hosts::marcar_identidad_ref",
            tabla: Tabla::Hosts,
            ejecutar: |c, ids| {
                hosts::marcar_identidad_ref(
                    c,
                    ids.host,
                    &IdentidadRef::Fichero("~/.ssh/id_prueba".to_string()),
                )
            },
        },
        Operacion {
            nombre: "hosts::fijar_snippet_al_conectar",
            tabla: Tabla::Hosts,
            ejecutar: |c, ids| hosts::fijar_snippet_al_conectar(c, ids.host, Some(ids.snippet)),
        },
        Operacion {
            nombre: "hosts::fijar_sesiones_persistentes",
            tabla: Tabla::Hosts,
            ejecutar: |c, ids| hosts::fijar_sesiones_persistentes(c, ids.host, true),
        },
        Operacion {
            nombre: "verificaciones::guardar (versión del host)",
            tabla: Tabla::Hosts,
            ejecutar: |c, ids| {
                verificaciones::guardar(
                    c,
                    ids.host,
                    &DatosVerificaciones {
                        salud: true,
                        backup: false,
                        backup_ruta: None,
                        backup_patron: None,
                        tests: false,
                        tests_comando: None,
                    },
                )
            },
        },
        Operacion {
            nombre: "identidades::renombrar",
            tabla: Tabla::Identidades,
            ejecutar: |c, ids| identidades::renombrar(c, ids.identidad, "portatil"),
        },
        Operacion {
            nombre: "identidades::reactivar",
            tabla: Tabla::Identidades,
            ejecutar: |c, ids| identidades::reactivar(c, ids.identidad_revocada),
        },
        Operacion {
            nombre: "tuneles::actualizar",
            tabla: Tabla::Tuneles,
            ejecutar: |c, ids| {
                tuneles::actualizar(
                    c,
                    ids.tunel,
                    &DatosTunel {
                        host_id: ids.host,
                        nombre: "web".to_string(),
                        tipo: TipoTunel::Local,
                        escucha: "127.0.0.1:8081".to_string(),
                        destino: Some("192.0.2.20:80".to_string()),
                        automatico: false,
                    },
                )
            },
        },
        Operacion {
            nombre: "tuneles::alternar_automatico",
            tabla: Tabla::Tuneles,
            ejecutar: |c, ids| tuneles::alternar_automatico(c, ids.tunel, true),
        },
        Operacion {
            nombre: "snippets::actualizar",
            tabla: Tabla::Snippets,
            ejecutar: |c, ids| {
                snippets::actualizar(
                    c,
                    ids.snippet,
                    &DatosSnippet {
                        nombre: "estado".to_string(),
                        comando: "uptime -p".to_string(),
                        destinos: vec![Destino::Etiqueta("web".to_string())],
                        ..DatosSnippet::default()
                    },
                )
            },
        },
        Operacion {
            nombre: "sincronizaciones::actualizar",
            tabla: Tabla::SincronizacionesDir,
            ejecutar: |c, ids| {
                sincronizaciones::actualizar(
                    c,
                    ids.sincronizacion,
                    &DatosSincronizacion {
                        host_id: ids.host,
                        nombre: "datos".to_string(),
                        ruta_local: "/home/usuario/datos".to_string(),
                        ruta_remota: "/srv/otros".to_string(),
                        direccion: Direccion::Subida,
                        borrar: true,
                        exclusiones: Vec::new(),
                    },
                )
            },
        },
        Operacion {
            nombre: "comprobaciones_servicio::guardar",
            tabla: Tabla::ComprobacionesServicio,
            ejecutar: |c, ids| {
                comprobaciones_servicio::guardar(
                    c,
                    ids.host,
                    TipoObjetivo::Unidad,
                    "nginx.service",
                    "systemctl is-active --quiet nginx",
                )
                .map(|_| ())
            },
        },
        Operacion {
            nombre: "sesiones_persistentes::actualizar",
            tabla: Tabla::SesionesPersistentes,
            ejecutar: |c, ids| {
                sesiones_persistentes::actualizar(
                    c,
                    ids.ficha,
                    &DatosFicha {
                        nombre: "agente".to_string(),
                        carpeta: Some("/home/usuario/proyecto".to_string()),
                        comando: None,
                        comando_reanudar: None,
                        nota: None,
                        origen: OrigenFicha::Manual,
                        creada_por: "equipo".to_string(),
                    },
                )
            },
        },
        // Las altas también: un elemento nuevo nace con su versión o no nace.
        Operacion {
            nombre: "hosts::crear (con etiquetas)",
            tabla: Tabla::Hosts,
            ejecutar: |c, _| {
                hosts::crear(
                    c,
                    &datos_de_host("app-01", "192.0.2.30", &["app", "nueva"]),
                    Origen::Manual,
                )
                .map(|_| ())
            },
        },
        Operacion {
            nombre: "grupos::crear",
            tabla: Tabla::Grupos,
            ejecutar: |c, _| grupos::crear(c, "otro-grupo").map(|_| ()),
        },
    ]
}

fn instalar_fallo_en_el_reloj(conexion: &Connection) {
    conexion
        .execute_batch(
            "CREATE TEMP TRIGGER fallo_en_el_reloj BEFORE UPDATE OF reloj_ms ON EQUIPOS
             BEGIN
                 SELECT RAISE(ABORT, 'fallo provocado');
             END;",
        )
        .expect("trigger de fallo provocado");
}

#[test]
fn cada_tabla_que_viaja_tiene_su_prueba_de_transaccion() {
    let cubiertas: Vec<Tabla> = operaciones()
        .iter()
        .map(|operacion| operacion.tabla)
        .collect();
    for tabla in Tabla::todas() {
        assert!(
            cubiertas.contains(&tabla),
            "la tabla {} que viaja no tiene ninguna operación en la prueba de la transacción (R84)",
            tabla.nombre()
        );
    }
}

#[test]
fn un_fallo_entre_el_cambio_y_su_version_no_deja_ningun_cambio_sin_version() {
    for operacion in operaciones() {
        let temporal = tempfile::tempdir().expect("directorio temporal");
        let ruta = temporal.path().join("magi.db");
        let hogar = temporal.path().join("hogar");
        std::fs::create_dir_all(&hogar).expect("hogar");
        let equipo = Equipo::nuevo("equipo-a", "linux");
        let almacen = Almacen::abrir(&ruta, &equipo, "1.6.1").expect("abriendo magi.db");
        let ids = inventario(almacen.conexion(), &hogar);
        let antes = foto(almacen.conexion());
        let versiones_antes = versiones_de(almacen.conexion(), operacion.tabla);

        // El fallo llega justo al escribir el reloj híbrido: después del cambio
        // y antes de la versión.
        instalar_fallo_en_el_reloj(almacen.conexion());
        let error = (operacion.ejecutar)(almacen.conexion(), &ids).expect_err(&format!(
            "{}: el fallo provocado no llegó",
            operacion.nombre
        ));
        assert!(
            format!("{error:#}").contains("fallo provocado"),
            "{}: {error:#}",
            operacion.nombre
        );
        assert_eq!(
            foto(almacen.conexion()),
            antes,
            "{}: quedó un cambio sin su versión en la misma conexión",
            operacion.nombre
        );

        // Y al reabrir la base (otra conexión, sin el trigger) tampoco.
        drop(almacen);
        let almacen = Almacen::abrir(&ruta, &equipo, "1.6.1").expect("reabriendo magi.db");
        assert_eq!(
            foto(almacen.conexion()),
            antes,
            "{}: quedó un cambio sin su versión al reabrir",
            operacion.nombre
        );

        // Sin el fallo, la misma operación sí cambia y sube la versión.
        (operacion.ejecutar)(almacen.conexion(), &ids)
            .unwrap_or_else(|error| panic!("{}: {error:#}", operacion.nombre));
        let despues = foto(almacen.conexion());
        assert_ne!(despues, antes, "{}: no cambió nada", operacion.nombre);
        assert_ne!(
            versiones_de(almacen.conexion(), operacion.tabla),
            versiones_antes,
            "{}: cambió sin subir la versión de {}",
            operacion.nombre,
            operacion.tabla.nombre()
        );
    }
}

/// `(uuid, version)` de cada fila de la tabla.
fn versiones_de(conexion: &Connection, tabla: Tabla) -> Vec<(String, String)> {
    let mut sentencia = conexion
        .prepare(&format!(
            "SELECT uuid, version FROM {} ORDER BY id",
            tabla.nombre()
        ))
        .expect("preparando las versiones");
    sentencia
        .query_map([], |fila| Ok((fila.get(0)?, fila.get(1)?)))
        .expect("leyendo las versiones")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("recogiendo las versiones")
}

#[test]
fn mantener_los_mios_resucita_todos_los_hosts_o_ninguno() {
    // La rama «mantener los míos» de la pasada sube el reloj y la versión de
    // cada host que otro borró: va en una transacción. Un fallo al subir el
    // segundo host no deja el primero con versión nueva.
    let (_temporal, carpeta) = carpeta_compartida();
    let clave = clave_nueva();
    let a = Maqueta::nueva(&carpeta, "a");
    let b = Maqueta::nueva(&carpeta, "b");
    a.crear_host("web-1");
    a.crear_host("web-2");
    a.exportar(&clave);
    b.pasada(&[copia(&clave)]);
    b.borrar_host("web-1");
    b.borrar_host("web-2");
    b.exportar(&clave);
    // A ya conoce a B: la pasada no escribe nada antes de la rama «mantener».
    a.conocer(&b, "b");

    let antes = foto(a.conexion());
    a.conexion()
        .execute_batch(
            "CREATE TEMP TRIGGER fallo_en_el_segundo BEFORE UPDATE OF version ON HOSTS
             WHEN NEW.nombre = 'web-2'
             BEGIN
                 SELECT RAISE(ABORT, 'fallo provocado');
             END;",
        )
        .expect("trigger");
    let resultado = a.intentar_pasada(&[copia(&clave)], PoliticaBorrados::Mantener, &|_, _| false);
    let error = resultado.expect_err("el fallo provocado no llegó");
    assert!(
        format!("{error:#}").contains("fallo provocado"),
        "{error:#}"
    );
    assert_eq!(foto(a.conexion()), antes, "no queda un host a medias");
    a.conexion()
        .execute_batch("DROP TRIGGER fallo_en_el_segundo;")
        .expect("quitando el trigger");

    // Sin el fallo resucitan los dos.
    let resultado =
        a.pasada_con_politica(&[copia(&clave)], PoliticaBorrados::Mantener, &|_, _| false);
    assert_eq!(resultado.estado, EstadoPasada::AlDia);
    assert!(a.tiene_host("web-1") && a.tiene_host("web-2"));
}

#[test]
fn un_cambio_sin_fallo_sube_la_version_y_el_reloj_a_la_vez() {
    // La otra cara de la transacción: lo normal sigue funcionando y deja el
    // reloj y la versión coherentes.
    let almacen = Almacen::abrir_en_memoria().expect("almacén en memoria");
    let host = hosts::crear(
        almacen.conexion(),
        &datos_de_host("web-01", "192.0.2.10", &[]),
        Origen::Manual,
    )
    .expect("host");
    let antes: String = almacen
        .conexion()
        .query_row("SELECT version FROM HOSTS WHERE id = ?1", [host], |fila| {
            fila.get(0)
        })
        .expect("versión");
    let reloj_antes = versiones::reloj_de_este(almacen.conexion()).expect("reloj");
    hosts::actualizar(
        almacen.conexion(),
        host,
        &datos_de_host("web-01", "192.0.2.11", &["nueva"]),
    )
    .expect("actualizando");
    let despues: String = almacen
        .conexion()
        .query_row("SELECT version FROM HOSTS WHERE id = ?1", [host], |fila| {
            fila.get(0)
        })
        .expect("versión");
    let reloj = versiones::reloj_de_este(almacen.conexion()).expect("reloj");
    assert_ne!(antes, despues);
    assert!(reloj > reloj_antes);
    assert_eq!(
        versiones::ms_de_version(&despues),
        Some(reloj),
        "la versión lleva el último milisegundo emitido"
    );
    // Cambiar un campo propio del equipo no sube la versión.
    hosts::marcar_conexion(almacen.conexion(), host).expect("marcando");
    let sin_cambio: String = almacen
        .conexion()
        .query_row("SELECT version FROM HOSTS WHERE id = ?1", [host], |fila| {
            fila.get(0)
        })
        .expect("versión");
    assert_eq!(sin_cambio, despues);
}
