//! Vista Archivos (F4): modelo de panel y utilidades puras. El panel local y
//! el remoto usan la misma `Entrada`, de modo que el widget y las marcas son
//! los mismos para los dos lados.
//!
//! `Entrada` viaja por el protocolo (el listado remoto lo construye el
//! servidor), así que su definición vive aquí y no en la capa de interfaz.
//! La `marca` la calcula siempre el cliente comparando los dos paneles: por el
//! cable va siempre sin marca.

pub mod local;
pub mod marcas;
pub mod panel;
pub mod sensibles;

pub use marcas::{Entrada, Marca, TipoEntrada};
pub use panel::{EstadoArchivos, Lado, Panel, Peticion};

/// Fecha relativa de una entrada del panel: `hoy`, `ayer`, `12 sep` o `2025`.
/// Los mtime desconocidos (cero o fuera de rango) se pintan con `—`.
///
/// Los instantes se interpretan en la zona local: es lo que espera quien mira
/// el panel, aunque el remoto esté en otra zona.
pub fn fecha_legible(mtime: i64, ahora: i64) -> String {
    let (fecha, hoy) = (crono_local(mtime), crono_local(ahora));
    let (Some(fecha), Some(hoy)) = (fecha, hoy) else {
        return "—".to_string();
    };
    let dias = hoy
        .date_naive()
        .signed_duration_since(fecha.date_naive())
        .num_days();
    match dias {
        0 => "hoy".to_string(),
        1 => "ayer".to_string(),
        _ if fecha.format("%Y").to_string() == hoy.format("%Y").to_string() => {
            format!(
                "{} {}",
                fecha.format("%d"),
                mes_abreviado(fecha.format("%m").to_string().parse().unwrap_or(1))
            )
        }
        _ => fecha.format("%Y").to_string(),
    }
}

/// Fecha completa del detalle: `12 sep 2026 09:41:07`.
pub fn fecha_completa(mtime: i64) -> String {
    match crono_local(mtime) {
        Some(fecha) => {
            let mes: u32 = fecha.format("%m").to_string().parse().unwrap_or(1);
            format!(
                "{} {} {} {}",
                fecha.format("%d"),
                mes_abreviado(mes),
                fecha.format("%Y"),
                fecha.format("%H:%M:%S"),
            )
        }
        None => "—".to_string(),
    }
}

/// Instante en la zona local; `None` si el sello no es representable.
fn crono_local(epoca: i64) -> Option<chrono::DateTime<chrono::Local>> {
    use chrono::TimeZone as _;
    chrono::Local.timestamp_opt(epoca, 0).single()
}

fn mes_abreviado(mes: u32) -> &'static str {
    match mes {
        1 => "ene",
        2 => "feb",
        3 => "mar",
        4 => "abr",
        5 => "may",
        6 => "jun",
        7 => "jul",
        8 => "ago",
        9 => "sep",
        10 => "oct",
        11 => "nov",
        _ => "dic",
    }
}

/// Tamaño legible: `14 kB`, `1.2 MB`. Los directorios los pinta el widget con
/// `—`; aquí solo llegan ficheros.
pub fn tamano_legible(bytes: u64) -> String {
    const K: f64 = 1024.0;
    let valor = bytes as f64;
    if bytes < 1024 {
        format!("{bytes} B")
    } else if valor < K * K {
        format!("{:.0} kB", valor / K)
    } else if valor < K * K * K {
        format!("{:.1} MB", valor / (K * K))
    } else {
        format!("{:.1} GB", valor / (K * K * K))
    }
}

/// Permisos en la notación de `ls`: `-rw-r--r--`, `drwxr-xr-x`.
pub fn permisos_legibles(modo: u32, es_dir: bool) -> String {
    let mut texto = String::with_capacity(10);
    texto.push(if es_dir { 'd' } else { '-' });
    for (bit, letra) in [
        (0o400, 'r'),
        (0o200, 'w'),
        (0o100, 'x'),
        (0o040, 'r'),
        (0o020, 'w'),
        (0o010, 'x'),
        (0o004, 'r'),
        (0o002, 'w'),
        (0o001, 'x'),
    ] {
        texto.push(if modo & bit != 0 { letra } else { '-' });
    }
    texto
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_tamano_se_pinta_en_unidades_legibles() {
        assert_eq!(tamano_legible(0), "0 B");
        assert_eq!(tamano_legible(1023), "1023 B");
        assert_eq!(tamano_legible(2048), "2 kB");
        assert_eq!(tamano_legible(1_310_720), "1.2 MB");
        assert_eq!(tamano_legible(3_221_225_472), "3.0 GB");
    }

    #[test]
    fn los_permisos_se_pintan_como_los_de_ls() {
        assert_eq!(permisos_legibles(0o644, false), "-rw-r--r--");
        assert_eq!(permisos_legibles(0o755, true), "drwxr-xr-x");
        assert_eq!(permisos_legibles(0o600, false), "-rw-------");
    }

    #[test]
    fn la_fecha_se_abrevia_a_hoy_ayer_dia_o_ano() {
        use chrono::TimeZone as _;
        // 12 de septiembre de 2026, 09:41:07 en la zona local: la referencia se
        // construye en local para que la prueba valga en cualquier zona.
        let referencia = chrono::Local
            .with_ymd_and_hms(2026, 9, 12, 9, 41, 7)
            .single()
            .expect("fecha de prueba")
            .timestamp();
        assert_eq!(fecha_legible(referencia, referencia), "hoy");
        assert_eq!(fecha_legible(referencia, referencia + 86_400), "ayer");
        assert_eq!(fecha_legible(referencia, referencia + 86_400 * 5), "12 sep");
        assert_eq!(fecha_legible(referencia, referencia + 86_400 * 400), "2026");
        assert_eq!(fecha_completa(referencia), "12 sep 2026 09:41:07");
    }
}
