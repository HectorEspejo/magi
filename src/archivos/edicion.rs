//! Ciclo de una edición remota (Fase 8, §3 y §7.3), sin red ni interfaz:
//! datos fijados al bajar el fichero (ruta, mtime, tamaño, permisos, uid y
//! SHA-256 del temporal), detección de cambios locales por hash, conflicto con
//! el remoto por `StatRemoto` (mtime o tamaño), aviso de propietario y nombre
//! de la copia local `<nombre>.magi-<AAAAMMDD-HHMMSS>`.
//!
//! Regla de oro (T55): el temporal solo se borra tras `hecha`, tras «sin
//! cambios» o tras «descartar» explícito (y tras una copia local verificada,
//! que deja el trabajo a salvo fuera del temporal). `borrado_permitido` lo
//! decide por fase; la App no borra por ningún otro camino.

use std::io::{Read as _, Write as _};
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use crate::archivos::marcas::{Entrada, Propietario, TipoEntrada};

/// Bytes del principio del fichero en los que se buscan nulos para decidir
/// si parece binario.
pub const MUESTRA_BINARIO: usize = 8 * 1024;

/// Saltos de enlace que se siguen antes de rendirse (un enlace a otro enlace).
pub const MAXIMO_SALTOS: u8 = 8;

/// SHA-256 de un fichero.
pub type Huella = [u8; 32];

/// Fase de una edición remota (diagrama de estados de §3). Las transiciones
/// las hace la App; aquí se nombran para poder escribir y probar la regla
/// del temporal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fase {
    /// `DescargarTemporal` en vuelo.
    Descargando,
    /// El temporal tiene bytes nulos: se espera el «editar igualmente».
    ConfirmandoBinario,
    /// El editor está abierto (o a punto de abrirse) sobre el temporal.
    Editando,
    /// Hay cambios y se espera al usuario: ¿subir?, conflicto o un aviso.
    Preguntando,
    /// `StatRemoto` en vuelo justo antes de subir.
    Verificando,
    /// `Transferir` enviado: se espera la fila de la cola con este id.
    Subiendo { peticion_id: u64 },
    /// La verificación o la subida falló: el temporal se conserva.
    Fallida { error: String },
}

/// Por qué termina una edición borrando su temporal. Son los únicos motivos
/// (T55); un error, una cancelación o el servidor caído no están aquí.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cierre {
    /// La transferencia de la subida llegó a `hecha`.
    Hecha,
    /// El temporal no se tocó: el hash no cambió o no se llegó a editar.
    SinCambios,
    /// El usuario confirmó descartar sus cambios.
    Descartada,
    /// La copia local se escribió y su hash coincide con el del temporal.
    CopiaGuardada,
}

/// ¿Se puede borrar el temporal al cerrar una edición en `fase` por `cierre`?
/// Nunca mientras se verifica o se sube, ni tras un error salvo con la copia
/// local ya verificada.
pub fn borrado_permitido(fase: &Fase, cierre: Cierre) -> bool {
    match fase {
        Fase::Descargando | Fase::ConfirmandoBinario | Fase::Editando => {
            cierre == Cierre::SinCambios
        }
        Fase::Preguntando => matches!(cierre, Cierre::Descartada | Cierre::CopiaGuardada),
        Fase::Verificando => false,
        Fase::Subiendo { .. } => cierre == Cierre::Hecha,
        Fase::Fallida { .. } => cierre == Cierre::CopiaGuardada,
    }
}

/// Lo que se fija al pulsar `E` sobre un fichero remoto: qué se edita, con
/// qué permisos y propietario estaba y adónde iría la copia local.
#[derive(Debug, Clone, PartialEq)]
pub struct Objetivo {
    pub host_id: i64,
    pub host_nombre: String,
    /// Ruta remota que se edita (la del destino si se pulsó sobre un enlace).
    pub ruta: String,
    /// Tamaño según el listado, para el aviso de `[archivos] editar_max_mb`.
    pub tamano: u64,
    pub permisos: Option<u32>,
    pub propietario: Option<Propietario>,
    /// Usuario de la conexión SFTP y su uid en el host.
    pub usuario_conexion: Option<String>,
    pub uid_conexion: Option<u32>,
    /// Directorio del panel local al pulsar `E`: ahí va la copia local.
    pub dir_local: PathBuf,
}

impl Objetivo {
    /// `host:ruta`, como se nombra en los diálogos.
    pub fn destino(&self) -> String {
        format!("{}:{}", self.host_nombre, self.ruta)
    }

    /// Nombre final de la ruta remota.
    pub fn nombre(&self) -> String {
        let nombre = self.ruta.trim_end_matches('/').rsplit('/').next();
        match nombre {
            Some(nombre) if !nombre.is_empty() => nombre.to_string(),
            _ => "fichero".to_string(),
        }
    }

    /// Permisos que se reaplican tras el `rename` de la subida: los doce bits
    /// del modo (sin el tipo de fichero).
    pub fn permisos_a_conservar(&self) -> Option<u32> {
        self.permisos.map(|modo| modo & 0o7777)
    }

    /// Aviso de propietario: si el uid del original y el de la conexión se
    /// conocen y son distintos, el `rename` dejará el fichero a nombre del
    /// usuario de conexión. Devuelve cómo se llama.
    pub fn nuevo_propietario(&self) -> Option<String> {
        let original = self.propietario.as_ref().and_then(|dueno| dueno.uid)?;
        let conexion = self.uid_conexion?;
        if original == conexion {
            return None;
        }
        Some(match &self.usuario_conexion {
            Some(usuario) => format!("{usuario} ({conexion})"),
            None => conexion.to_string(),
        })
    }

    /// Dueño actual del original, legible (`root (0)`).
    pub fn propietario_actual(&self) -> String {
        self.propietario
            .as_ref()
            .map(|dueno| dueno.usuario_legible())
            .unwrap_or_else(|| "—".to_string())
    }
}

/// Lo que se fija al llegar el temporal, antes de abrir el editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bajado {
    /// mtime y tamaño del temporal recién bajado: el servidor le pone el
    /// mtime del remoto, así que son los del remoto al abrirlo.
    pub mtime: i64,
    pub tamano: u64,
    pub huella: Huella,
    /// Bytes nulos en los primeros `MUESTRA_BINARIO`.
    pub binario: bool,
}

/// Lee los metadatos, el SHA-256 y la muestra de binario del temporal.
pub fn leer_bajado(temporal: &Path) -> Result<Bajado, String> {
    let metadata = std::fs::metadata(temporal).map_err(|error| describir(temporal, &error))?;
    let mut muestra = Vec::with_capacity(MUESTRA_BINARIO);
    std::fs::File::open(temporal)
        .and_then(|fichero| {
            fichero
                .take(MUESTRA_BINARIO as u64)
                .read_to_end(&mut muestra)
        })
        .map_err(|error| describir(temporal, &error))?;
    Ok(Bajado {
        mtime: crate::archivos::local::mtime_de(&metadata),
        tamano: metadata.len(),
        huella: huella(temporal)?,
        binario: parece_binario(&muestra),
    })
}

/// SHA-256 de un fichero, leído por bloques.
pub fn huella(ruta: &Path) -> Result<Huella, String> {
    let mut fichero = std::fs::File::open(ruta).map_err(|error| describir(ruta, &error))?;
    let mut resumen = Sha256::new();
    let mut bufer = vec![0u8; 64 * 1024];
    loop {
        let leidos = fichero
            .read(&mut bufer)
            .map_err(|error| describir(ruta, &error))?;
        if leidos == 0 {
            break;
        }
        resumen.update(&bufer[..leidos]);
    }
    Ok(resumen.finalize().into())
}

/// ¿Parece binario? Hay algún byte nulo en los primeros 8 KiB.
pub fn parece_binario(contenido: &[u8]) -> bool {
    contenido
        .iter()
        .take(MUESTRA_BINARIO)
        .any(|byte| *byte == 0)
}

/// El remoto justo antes de subir, comparado con el de al abrirlo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Remoto {
    Igual,
    /// Otro tamaño u otro mtime (o ya no es un fichero): conflicto.
    Cambiado {
        tamano: u64,
        mtime: i64,
    },
    /// La ruta ya no existe: también es un conflicto.
    NoExiste,
}

impl Remoto {
    pub fn conflicto(self) -> bool {
        self != Remoto::Igual
    }
}

/// Compara el `Stat` de ahora con lo guardado al bajar (§7.3): conflicto si
/// el mtime o el tamaño difieren, si ya no existe o si ya no es un fichero.
pub fn comparar_remoto(bajado: &Bajado, ahora: Option<&Entrada>) -> Remoto {
    let Some(ahora) = ahora else {
        return Remoto::NoExiste;
    };
    let mismo = ahora.tipo == TipoEntrada::Fichero
        && ahora.tamano == bajado.tamano
        && ahora.mtime == bajado.mtime;
    if mismo {
        Remoto::Igual
    } else {
        Remoto::Cambiado {
            tamano: ahora.tamano,
            mtime: ahora.mtime,
        }
    }
}

/// Destino de un enlace remoto resuelto textualmente respecto a su
/// directorio, con `.` y `..` normalizados léxicamente (sin preguntar al
/// host: lo que haya al final lo dice el `StatRemoto`).
pub fn resolver_enlace(dir: &str, destino: &str) -> String {
    let completa = if destino.starts_with('/') {
        destino.to_string()
    } else {
        crate::servidor::sftp::join(dir, destino)
    };
    let absoluta = completa.starts_with('/');
    let mut partes: Vec<&str> = Vec::new();
    for parte in completa.split('/') {
        match parte {
            "" | "." => {}
            ".." => match partes.last() {
                Some(ultima) if *ultima != ".." => {
                    partes.pop();
                }
                // Por encima de la raíz no hay nada: se queda en ella.
                _ if absoluta => {}
                _ => partes.push(".."),
            },
            otra => partes.push(otra),
        }
    }
    let unidas = partes.join("/");
    match (absoluta, unidas.is_empty()) {
        (true, _) => format!("/{unidas}"),
        (false, true) => ".".to_string(),
        (false, false) => unidas,
    }
}

/// Nombre de la copia local: `<nombre>.magi-<AAAAMMDD-HHMMSS>` en hora local.
pub fn nombre_copia(nombre: &str, momento: &chrono::DateTime<chrono::Local>) -> String {
    format!("{nombre}.magi-{}", momento.format("%Y%m%d-%H%M%S"))
}

/// Guarda el temporal como copia local en `dir`, con permisos 600 y sin
/// pisar nada (si el nombre ya existe se le añade `-2`, `-3`…). Devuelve la
/// ruta solo si el SHA-256 de la copia coincide con el del temporal; si no,
/// la copia se queda donde está y el temporal no debe borrarse.
pub fn guardar_copia(
    temporal: &Path,
    dir: &Path,
    nombre: &str,
    momento: &chrono::DateTime<chrono::Local>,
) -> Result<PathBuf, String> {
    let base = nombre_copia(nombre, momento);
    let mut intento = 1u32;
    let (ruta, mut salida) = loop {
        let candidato = match intento {
            1 => dir.join(&base),
            otro => dir.join(format!("{base}-{otro}")),
        };
        let abierto = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&candidato);
        match abierto {
            Ok(fichero) => break (candidato, fichero),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && intento < 100 => {
                intento += 1;
            }
            Err(error) => return Err(describir(&candidato, &error)),
        }
    };
    // El modo de creación pasa por la umask: se fija explícitamente.
    std::fs::set_permissions(&ruta, std::fs::Permissions::from_mode(0o600))
        .map_err(|error| describir(&ruta, &error))?;
    let mut entrada = std::fs::File::open(temporal).map_err(|error| describir(temporal, &error))?;
    std::io::copy(&mut entrada, &mut salida).map_err(|error| describir(&ruta, &error))?;
    salida
        .flush()
        .and_then(|_| salida.sync_all())
        .map_err(|error| describir(&ruta, &error))?;
    drop(salida);
    if huella(&ruta)? != huella(temporal)? {
        return Err(format!(
            "la copia {} no coincide con el temporal",
            ruta.display()
        ));
    }
    Ok(ruta)
}

/// `14 336`: los miles separados por espacios, como en los diálogos.
pub fn bytes_con_miles(bytes: u64) -> String {
    let cifras = bytes.to_string();
    let mut salida = String::with_capacity(cifras.len() + cifras.len() / 3);
    for (indice, cifra) in cifras.chars().enumerate() {
        if indice > 0 && (cifras.len() - indice).is_multiple_of(3) {
            salida.push(' ');
        }
        salida.push(cifra);
    }
    salida
}

/// Fecha corta del conflicto en modo estrecho: `03 oct 14:31:05`.
pub fn fecha_corta(mtime: i64) -> String {
    use chrono::TimeZone as _;
    match chrono::Local.timestamp_opt(mtime, 0).single() {
        Some(fecha) => {
            let mes: u32 = fecha.format("%m").to_string().parse().unwrap_or(1);
            format!(
                "{} {} {}",
                fecha.format("%d"),
                super::mes_abreviado(mes),
                fecha.format("%H:%M:%S")
            )
        }
        None => "—".to_string(),
    }
}

fn describir(ruta: &Path, error: &std::io::Error) -> String {
    format!("{}: {error}", ruta.display())
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::archivos::marcas::Marca;

    fn objetivo() -> Objetivo {
        Objetivo {
            host_id: 1,
            host_nombre: "hetzner-01".to_string(),
            ruta: "/etc/nginx/sites-available/cooperapp".to_string(),
            tamano: 2_041,
            permisos: Some(0o100644),
            propietario: Some(Propietario {
                uid: Some(0),
                gid: Some(0),
                usuario: Some("root".to_string()),
                grupo: Some("root".to_string()),
            }),
            usuario_conexion: Some("deploy".to_string()),
            uid_conexion: Some(1001),
            dir_local: PathBuf::from("/home/hector"),
        }
    }

    fn bajado(tamano: u64, mtime: i64) -> Bajado {
        Bajado {
            mtime,
            tamano,
            huella: [0; 32],
            binario: false,
        }
    }

    fn entrada(tipo: TipoEntrada, tamano: u64, mtime: i64) -> Entrada {
        Entrada {
            nombre: "cooperapp".to_string(),
            tipo,
            tamano,
            mtime,
            permisos: Some(0o644),
            propietario: None,
            enlace: None,
            marca: Marca::Ninguna,
        }
    }

    #[test]
    fn la_huella_es_el_sha256_del_contenido() {
        let dir = tempfile::tempdir().unwrap();
        let ruta = dir.path().join("vacio");
        std::fs::write(&ruta, b"").unwrap();
        let vacio = huella(&ruta).unwrap();
        assert_eq!(
            vacio[..4],
            [0xe3, 0xb0, 0xc4, 0x42],
            "SHA-256 de la cadena vacía"
        );
        std::fs::write(&ruta, b"abc").unwrap();
        let abc = huella(&ruta).unwrap();
        assert_eq!(abc[..4], [0xba, 0x78, 0x16, 0xbf], "SHA-256 de «abc»");
        assert!(huella(&dir.path().join("no-existe")).is_err());
    }

    #[test]
    fn parece_binario_solo_con_nulos_en_los_primeros_8_kib() {
        assert!(!parece_binario(b"server {\n  listen 80;\n}\n"));
        assert!(parece_binario(b"\x7fELF\0\0\0"));
        let mut tarde = vec![b'a'; MUESTRA_BINARIO];
        tarde.push(0);
        assert!(!parece_binario(&tarde), "un nulo pasado 8 KiB no cuenta");
        tarde[MUESTRA_BINARIO - 1] = 0;
        assert!(parece_binario(&tarde));
    }

    #[test]
    fn leer_bajado_fija_mtime_tamano_huella_y_binario() {
        let dir = tempfile::tempdir().unwrap();
        let ruta = dir.path().join("3-main.py");
        std::fs::write(&ruta, b"print('hola')\n").unwrap();
        filetime::set_file_mtime(&ruta, filetime::FileTime::from_unix_time(1_757_670_067, 0))
            .unwrap();
        let leido = leer_bajado(&ruta).unwrap();
        assert_eq!(leido.mtime, 1_757_670_067);
        assert_eq!(leido.tamano, 14);
        assert_eq!(leido.huella, huella(&ruta).unwrap());
        assert!(!leido.binario);

        std::fs::write(&ruta, b"\0binario").unwrap();
        assert!(leer_bajado(&ruta).unwrap().binario);
    }

    #[test]
    fn el_conflicto_salta_por_mtime_tamano_tipo_o_ausencia() {
        let guardado = bajado(2_041, 1_000);
        let igual = entrada(TipoEntrada::Fichero, 2_041, 1_000);
        assert_eq!(comparar_remoto(&guardado, Some(&igual)), Remoto::Igual);
        assert!(!Remoto::Igual.conflicto());

        let otro_tamano = entrada(TipoEntrada::Fichero, 2_112, 1_000);
        assert_eq!(
            comparar_remoto(&guardado, Some(&otro_tamano)),
            Remoto::Cambiado {
                tamano: 2_112,
                mtime: 1_000
            }
        );
        // Sin tolerancia: el temporal lleva el mtime exacto del remoto.
        let otra_fecha = entrada(TipoEntrada::Fichero, 2_041, 1_001);
        assert!(comparar_remoto(&guardado, Some(&otra_fecha)).conflicto());
        let ahora_dir = entrada(TipoEntrada::Directorio, 2_041, 1_000);
        assert!(comparar_remoto(&guardado, Some(&ahora_dir)).conflicto());
        assert_eq!(comparar_remoto(&guardado, None), Remoto::NoExiste);
    }

    #[test]
    fn el_aviso_de_propietario_exige_los_dos_uid_y_que_difieran() {
        let mut ajeno = objetivo();
        assert_eq!(ajeno.nuevo_propietario().as_deref(), Some("deploy (1001)"));
        assert_eq!(ajeno.propietario_actual(), "root (0)");

        ajeno.usuario_conexion = None;
        assert_eq!(ajeno.nuevo_propietario().as_deref(), Some("1001"));

        let mut propio = objetivo();
        propio.uid_conexion = Some(0);
        assert_eq!(propio.nuevo_propietario(), None);

        let mut sin_uid = objetivo();
        sin_uid.propietario = None;
        assert_eq!(sin_uid.nuevo_propietario(), None, "sin uid no se avisa");
        let mut sin_conexion = objetivo();
        sin_conexion.uid_conexion = None;
        assert_eq!(sin_conexion.nuevo_propietario(), None);
    }

    #[test]
    fn los_permisos_que_se_conservan_son_los_doce_bits_del_modo() {
        let mut con_especiales = objetivo();
        con_especiales.permisos = Some(0o104755);
        assert_eq!(con_especiales.permisos_a_conservar(), Some(0o4755));
        assert_eq!(objetivo().permisos_a_conservar(), Some(0o644));
        assert_eq!(objetivo().nombre(), "cooperapp");
        assert_eq!(
            objetivo().destino(),
            "hetzner-01:/etc/nginx/sites-available/cooperapp"
        );
    }

    #[test]
    fn el_nombre_de_la_copia_lleva_fecha_y_hora_locales() {
        use chrono::TimeZone as _;
        let momento = chrono::Local
            .with_ymd_and_hms(2026, 10, 3, 14, 36, 5)
            .single()
            .unwrap();
        assert_eq!(
            nombre_copia("cooperapp", &momento),
            "cooperapp.magi-20261003-143605"
        );
    }

    #[test]
    fn la_copia_local_es_600_verificada_y_nunca_pisa_nada() {
        use chrono::TimeZone as _;
        let dir = tempfile::tempdir().unwrap();
        let temporal = dir.path().join("7-cooperapp");
        std::fs::write(&temporal, b"server_name cooperapp;\n").unwrap();
        let destino = dir.path().join("panel");
        std::fs::create_dir(&destino).unwrap();
        let momento = chrono::Local
            .with_ymd_and_hms(2026, 10, 3, 14, 36, 5)
            .single()
            .unwrap();

        let copia = guardar_copia(&temporal, &destino, "cooperapp", &momento).unwrap();
        assert_eq!(copia, destino.join("cooperapp.magi-20261003-143605"));
        assert_eq!(std::fs::read(&copia).unwrap(), b"server_name cooperapp;\n");
        let modo = std::fs::metadata(&copia).unwrap().permissions().mode();
        assert_eq!(modo & 0o777, 0o600);

        // Otra copia en el mismo segundo no pisa la primera.
        std::fs::write(&temporal, b"otra version\n").unwrap();
        let segunda = guardar_copia(&temporal, &destino, "cooperapp", &momento).unwrap();
        assert_eq!(segunda, destino.join("cooperapp.magi-20261003-143605-2"));
        assert_eq!(std::fs::read(&copia).unwrap(), b"server_name cooperapp;\n");

        // Sin directorio, error y nada que borrar.
        assert!(guardar_copia(&temporal, &dir.path().join("no"), "x", &momento).is_err());
        assert!(temporal.exists());
    }

    #[test]
    fn el_enlace_se_resuelve_respecto_a_su_directorio() {
        assert_eq!(
            resolver_enlace("/var/www", "app/main.py"),
            "/var/www/app/main.py"
        );
        assert_eq!(
            resolver_enlace("/var/www/app", "../comun/x.conf"),
            "/var/www/comun/x.conf"
        );
        assert_eq!(
            resolver_enlace("/var/www", "/etc/./nginx//a.conf"),
            "/etc/nginx/a.conf"
        );
        assert_eq!(resolver_enlace("/", "../../etc/hosts"), "/etc/hosts");
        assert_eq!(resolver_enlace("dir", "../../x"), "../x");
        assert_eq!(resolver_enlace("/srv", "."), "/srv");
    }

    #[test]
    fn el_temporal_solo_se_borra_por_los_motivos_de_su_fase() {
        use Cierre::*;
        let fallida = Fase::Fallida {
            error: "permiso denegado".to_string(),
        };
        let subiendo = Fase::Subiendo { peticion_id: 7 };
        // Sin cambios: solo antes de que haya cambios.
        assert!(borrado_permitido(&Fase::Editando, SinCambios));
        assert!(borrado_permitido(&Fase::ConfirmandoBinario, SinCambios));
        assert!(!borrado_permitido(&Fase::Preguntando, SinCambios));
        // Subiendo: solo con la transferencia hecha.
        assert!(borrado_permitido(&subiendo, Hecha));
        for cierre in [SinCambios, Descartada, CopiaGuardada] {
            assert!(!borrado_permitido(&subiendo, cierre), "{cierre:?} subiendo");
        }
        // Verificando: nunca.
        for cierre in [Hecha, SinCambios, Descartada, CopiaGuardada] {
            assert!(!borrado_permitido(&Fase::Verificando, cierre));
        }
        // Tras un error: solo con la copia local verificada.
        assert!(borrado_permitido(&fallida, CopiaGuardada));
        for cierre in [Hecha, SinCambios, Descartada] {
            assert!(
                !borrado_permitido(&fallida, cierre),
                "{cierre:?} tras error"
            );
        }
        assert!(borrado_permitido(&Fase::Preguntando, Descartada));
        assert!(!borrado_permitido(&Fase::Preguntando, Hecha));
    }

    #[test]
    fn tamanos_y_fechas_de_los_dialogos() {
        assert_eq!(bytes_con_miles(0), "0");
        assert_eq!(bytes_con_miles(999), "999");
        assert_eq!(bytes_con_miles(2_041), "2 041");
        assert_eq!(bytes_con_miles(1_153_434), "1 153 434");
        use chrono::TimeZone as _;
        let momento = chrono::Local
            .with_ymd_and_hms(2026, 10, 3, 14, 31, 5)
            .single()
            .unwrap()
            .timestamp();
        assert_eq!(fecha_corta(momento), "03 oct 14:31:05");
    }
}
