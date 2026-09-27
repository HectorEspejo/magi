//! Datos de las instantáneas: los siete hosts de las maquetas del informe, sus
//! grupos, sondeos, túneles, snippets, entradas del registro e identidades.
//!
//! Todo es determinista: las fechas absolutas son fijas (con `TZ=UTC`) y las
//! relativas («hace 30 s») se calculan contra la hora real con valores lejos
//! de los redondeos. El registro y las identidades se escriben con SQL a
//! mano solo para fijar sus fechas: en la App nadie escribe así en `REGISTRO`.

use std::collections::BTreeMap;

use magi::almacen::{grupos, hosts, snippets, sondeos, tuneles, Almacen};
use magi::config::Rutas;
use magi::modelo::{
    DatosHost, DatosTunel, IdentidadRef, Origen, ResultadoSondeo, Sondeo, TipoTunel,
};
use magi::snippets::{DatosSnippet, Destino};

/// Ids de lo sembrado, para las pruebas que los necesiten.
#[derive(Debug, Clone, Default)]
pub struct Sembrado {
    /// En el orden de `HOSTS` de la semilla.
    pub hosts: Vec<(String, i64)>,
    pub tuneles: Vec<i64>,
    pub snippets: Vec<i64>,
}

impl Sembrado {
    pub fn host(&self, nombre: &str) -> i64 {
        self.hosts
            .iter()
            .find(|(otro, _)| otro == nombre)
            .map(|(_, id)| *id)
            .unwrap_or_else(|| panic!("host «{nombre}» no sembrado"))
    }
}

/// Nombre, grupo, dirección, usuario, puerto.
pub const HOSTS: [(&str, Option<&str>, &str, &str, u16); 7] = [
    ("hetzner-01", Some("produccion"), "10.0.1.11", "root", 22),
    (
        "hetzner-02",
        Some("produccion"),
        "10.0.1.12",
        "deploy",
        2222,
    ),
    (
        "vps-openclaw",
        Some("produccion"),
        "203.0.113.40",
        "admin",
        22,
    ),
    ("mac-mini-m1", Some("casa"), "192.168.1.20", "hector", 22),
    ("dgx-spark", Some("casa"), "192.168.1.30", "hector", 22),
    ("rincon-dev", None, "dev.rincon.example", "hector", 2200),
    ("backup-nas", None, "192.168.1.40", "backup", 22),
];

/// Una fecha RFC 3339 `segundos` atrás.
fn hace(segundos: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::seconds(segundos))
        .format("%Y-%m-%dT%H:%M:%S+00:00")
        .to_string()
}

fn sondeo_ok(host_id: i64, carga: f64, mem_libre_pct: i64, disco_pct: i64) -> Sondeo {
    Sondeo {
        host_id,
        fecha: hace(30),
        resultado: ResultadoSondeo::Ok,
        nucleos: Some(8),
        carga_1m: Some(carga),
        carga_5m: Some(carga * 0.8),
        carga_15m: Some(carga * 0.6),
        mem_total_kb: Some(16_000_000),
        mem_disponible_kb: Some(160_000 * mem_libre_pct),
        disco_total_kb: Some(500_000_000),
        disco_usado_kb: Some(5_000_000 * disco_pct),
        red_rx_bytes: Some(1_000_000),
        red_tx_bytes: Some(500_000),
        uptime_seg: Some(12 * 86_400 + 3_600),
        servicios: BTreeMap::new(),
        duracion_ms: 420,
        ..Sondeo::default()
    }
}

/// Siembra todo en la base de datos de `rutas`.
pub fn sembrar(rutas: &Rutas) -> Sembrado {
    let almacen = Almacen::abrir(&rutas.base_datos()).expect("almacén de la semilla");
    let conexion = almacen.conexion();
    let mut sembrado = Sembrado::default();

    let produccion = grupos::crear(conexion, "produccion").unwrap();
    let casa = grupos::crear(conexion, "casa").unwrap();
    for (nombre, grupo, direccion, usuario, puerto) in HOSTS {
        let id = hosts::crear(
            conexion,
            &DatosHost {
                nombre: nombre.to_string(),
                grupo_id: grupo.map(|grupo| if grupo == "casa" { casa } else { produccion }),
                direccion: direccion.to_string(),
                puerto,
                usuario: Some(usuario.to_string()),
                identidad_ref: IdentidadRef::Auto,
                etiquetas: if grupo == Some("produccion") {
                    vec!["web".to_string()]
                } else {
                    Vec::new()
                },
                ..DatosHost::default()
            },
            Origen::Manual,
        )
        .unwrap();
        sembrado.hosts.push((nombre.to_string(), id));
    }

    // Sondeos: nominal, carga, caída y uno sin sondear (fría).
    let ids = sembrado.clone();
    let id = |nombre: &str| ids.host(nombre);
    for sondeo in [
        sondeo_ok(id("hetzner-01"), 0.42, 55, 38),
        sondeo_ok(id("hetzner-02"), 1.10, 61, 44),
        sondeo_ok(id("vps-openclaw"), 0.20, 70, 21),
        sondeo_ok(id("mac-mini-m1"), 9.60, 12, 71),
        sondeo_ok(id("backup-nas"), 0.05, 80, 93),
        Sondeo {
            host_id: id("rincon-dev"),
            fecha: hace(30),
            resultado: ResultadoSondeo::Error,
            error: Some("conexión rechazada".to_string()),
            duracion_ms: 10_000,
            ..Sondeo::default()
        },
    ] {
        sondeos::guardar(conexion, &sondeo).unwrap();
    }

    for (host, nombre, tipo, escucha, destino) in [
        (
            "hetzner-01",
            "postgres",
            TipoTunel::Local,
            "127.0.0.1:5432",
            Some("127.0.0.1:5432"),
        ),
        (
            "hetzner-02",
            "grafana",
            TipoTunel::Local,
            "127.0.0.1:3000",
            Some("10.0.1.50:3000"),
        ),
        (
            "vps-openclaw",
            "socks",
            TipoTunel::Dinamico,
            "127.0.0.1:1080",
            None,
        ),
        (
            "mac-mini-m1",
            "webhook",
            TipoTunel::Remoto,
            "0.0.0.0:8080",
            Some("127.0.0.1:8080"),
        ),
    ] {
        let tunel = tuneles::crear(
            conexion,
            &DatosTunel {
                host_id: id(host),
                nombre: nombre.to_string(),
                tipo,
                escucha: escucha.to_string(),
                destino: destino.map(str::to_string),
                automatico: nombre == "postgres",
            },
        )
        .unwrap();
        sembrado.tuneles.push(tunel);
    }

    for (nombre, comando, critico, destinos) in [
        (
            "reiniciar nginx",
            "sudo systemctl restart nginx",
            true,
            vec![Destino::Etiqueta("web".to_string())],
        ),
        (
            "espacio en disco",
            "df -h /",
            false,
            vec![
                Destino::Host {
                    id: id("hetzner-01"),
                    nombre: "hetzner-01".to_string(),
                },
                Destino::Host {
                    id: id("backup-nas"),
                    nombre: "backup-nas".to_string(),
                },
            ],
        ),
        (
            "actualizar paquetes",
            "sudo apt update && sudo apt -y upgrade",
            true,
            vec![Destino::Etiqueta("web".to_string())],
        ),
    ] {
        let snippet = snippets::crear(
            conexion,
            &DatosSnippet {
                nombre: nombre.to_string(),
                comando: comando.to_string(),
                descripcion: format!("{nombre} en los destinos"),
                critico,
                destinos,
                ..DatosSnippet::default()
            },
        )
        .unwrap();
        sembrado.snippets.push(snippet);
    }

    // Registro con fechas fijas (SQL a mano solo para fijarlas).
    for (fecha, tipo, host, detalle, resultado) in [
        (
            "2026-09-15T09:12:03+00:00",
            "conexion_abierta",
            Some("hetzner-01"),
            "sesión abierta con clave ed25519",
            "ok",
        ),
        (
            "2026-09-15T09:40:51+00:00",
            "transferencia",
            Some("hetzner-02"),
            "subida de main.py (14 kB)",
            "ok",
        ),
        (
            "2026-09-15T10:02:17+00:00",
            "sondeo_fallido",
            Some("rincon-dev"),
            "conexión rechazada",
            "error",
        ),
        (
            "2026-09-15T11:25:44+00:00",
            "tunel_abierto",
            Some("hetzner-01"),
            "postgres 127.0.0.1:5432",
            "ok",
        ),
        (
            "2026-09-15T12:03:09+00:00",
            "snippet_ejecutado",
            Some("hetzner-02"),
            "reiniciar nginx: código 0",
            "ok",
        ),
        (
            "2026-09-15T15:41:02+00:00",
            "deliberacion_forzada",
            None,
            "reiniciar nginx → 3 hosts: backup de hace 31 h",
            "ok",
        ),
    ] {
        conexion
            .execute(
                "INSERT INTO REGISTRO (fecha, tipo, host_id, detalle, resultado)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![fecha, tipo, host.map(id), detalle, resultado],
            )
            .unwrap();
    }

    // Identidades con fechas fijas.
    for (alias, tipo, huella, origen, ruta) in [
        (
            "trabajo",
            "ssh-ed25519",
            "SHA256:Ab3dE5fG7hJ9kL1mN3pQ5rS7tU9vW1xY3zA5bC7dE9",
            "fichero",
            Some("~/.ssh/id_ed25519"),
        ),
        (
            "yubikey",
            "sk-ssh-ed25519@openssh.com",
            "SHA256:Zy9xW7vU5tS3rQ1pO9nM7lK5jI3hG1fE9dC7bA5zY3",
            "agente",
            None,
        ),
    ] {
        conexion
            .execute(
                "INSERT INTO IDENTIDADES (alias, tipo, huella, origen, ruta, anadida_en)
                 VALUES (?1, ?2, ?3, ?4, ?5, '2026-09-01T10:00:00+00:00')",
                rusqlite::params![alias, tipo, huella, origen, ruta],
            )
            .unwrap();
    }

    almacen.cerrar().unwrap();
    sembrado
}
