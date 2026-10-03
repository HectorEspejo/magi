//! `magi sincronizaciones` (Fase 8, §5.1): las sincronizaciones guardadas en
//! una tabla de texto con host, nombre, dirección, borrado, último resultado
//! (con la fecha de la última ejecución) y rutas. Solo lee el inventario: la
//! ejecución necesita la TUI (plan y deliberación, como D72).

use crate::modelo::Sincronizacion;
use crate::protocolo::Direccion;
use crate::snippets::salida::texto_limpio;

/// Anchos de las columnas fijas (las rutas van al final, enteras).
const ANCHO_HOST: usize = 16;
const ANCHO_NOMBRE: usize = 20;
const ANCHO_DIRECCION: usize = 9;
const ANCHO_BORRAR: usize = 6;
const ANCHO_ULTIMO: usize = 32;

/// La tabla de `magi sincronizaciones`, ya ordenada como llega (por host y
/// nombre).
pub fn listado_cli(sincronizaciones: &[Sincronizacion]) -> String {
    if sincronizaciones.is_empty() {
        return "Sin sincronizaciones: créalas en la TUI con S o L.\n".to_string();
    }
    let mut texto = format!(
        "{:<ANCHO_HOST$} {:<ANCHO_NOMBRE$} {:<ANCHO_DIRECCION$} {:<ANCHO_BORRAR$} \
         {:<ANCHO_ULTIMO$} RUTAS\n",
        "HOST", "NOMBRE", "DIRECCIÓN", "BORRAR", "ÚLTIMO RESULTADO"
    );
    for sincronizacion in sincronizaciones {
        texto.push_str(&format!(
            "{:<ANCHO_HOST$} {:<ANCHO_NOMBRE$} {:<ANCHO_DIRECCION$} {:<ANCHO_BORRAR$} \
             {:<ANCHO_ULTIMO$} {}\n",
            recortar(&limpio(&sincronizacion.host_nombre), ANCHO_HOST),
            recortar(&limpio(&sincronizacion.nombre), ANCHO_NOMBRE),
            sincronizacion.direccion.texto(),
            if sincronizacion.borrar { "sí" } else { "no" },
            ultimo_resultado(sincronizacion),
            rutas(sincronizacion),
        ));
    }
    texto
}

/// `ok · 2026-10-02 14:31`; `sin resultado · …` si se lanzó y el servidor no
/// llegó a escribirlo (en curso o servidor caído, R40); `nunca` si no se ha
/// lanzado.
fn ultimo_resultado(sincronizacion: &Sincronizacion) -> String {
    let fecha = sincronizacion.ultima_ejecucion_en.as_deref().map(fecha_cli);
    match (sincronizacion.ultimo_resultado, fecha) {
        (Some(resultado), Some(fecha)) => format!("{} · {fecha}", resultado.como_texto()),
        (Some(resultado), None) => resultado.como_texto().to_string(),
        (None, Some(fecha)) => format!("sin resultado · {fecha}"),
        (None, None) => "nunca".to_string(),
    }
}

/// `local → remoto` en una subida y `local ← remoto` en una bajada: la flecha
/// dice hacia dónde van los ficheros.
fn rutas(sincronizacion: &Sincronizacion) -> String {
    let flecha = match sincronizacion.direccion {
        Direccion::Subida => "→",
        Direccion::Bajada => "←",
    };
    format!(
        "{} {flecha} {}",
        limpio(&sincronizacion.ruta_local),
        limpio(&sincronizacion.ruta_remota)
    )
}

/// `AAAA-MM-DD HH:MM` en la zona con la que se escribió (la local del
/// cliente al lanzar); si no se entiende, el texto tal cual.
fn fecha_cli(texto: &str) -> String {
    match chrono::DateTime::parse_from_rfc3339(texto) {
        Ok(fecha) => fecha.format("%Y-%m-%d %H:%M").to_string(),
        Err(_) => limpio(texto),
    }
}

/// Las filas las escribe otro proceso (o una mano en la base): ningún escape
/// de terminal llega a la salida.
fn limpio(texto: &str) -> String {
    texto_limpio(texto.as_bytes()).replace('\n', " ")
}

fn recortar(texto: &str, ancho: usize) -> String {
    if texto.chars().count() <= ancho {
        return texto.to_string();
    }
    let recortado: String = texto.chars().take(ancho.saturating_sub(1)).collect();
    format!("{recortado}…")
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::modelo::ResultadoSincronizacion;

    fn guardada(host: &str, nombre: &str, direccion: Direccion) -> Sincronizacion {
        Sincronizacion {
            id: 1,
            host_id: 1,
            host_nombre: host.to_string(),
            nombre: nombre.to_string(),
            ruta_local: "/home/hector/proyectos/cooperapp".to_string(),
            ruta_remota: "/var/www/cooperapp".to_string(),
            direccion,
            borrar: false,
            exclusiones: vec!["*.log".to_string()],
            ultima_ejecucion_en: None,
            ultimo_resultado: None,
            creado_en: "2026-09-01T10:00:00+02:00".to_string(),
            actualizado_en: "2026-09-01T10:00:00+02:00".to_string(),
        }
    }

    #[test]
    fn sin_guardadas_dice_como_crearlas() {
        assert_eq!(
            listado_cli(&[]),
            "Sin sincronizaciones: créalas en la TUI con S o L.\n"
        );
    }

    #[test]
    fn cada_fila_lleva_host_nombre_direccion_borrar_resultado_y_rutas() {
        let mut subida = guardada("hetzner-01", "web-prod", Direccion::Subida);
        subida.ultima_ejecucion_en = Some("2026-10-02T14:31:05+02:00".to_string());
        subida.ultimo_resultado = Some(ResultadoSincronizacion::Ok);
        let mut bajada = guardada("hetzner-01", "logs", Direccion::Bajada);
        bajada.ruta_local = "/home/hector/logs".to_string();
        bajada.ruta_remota = "/var/log/nginx".to_string();
        bajada.borrar = true;
        let texto = listado_cli(&[bajada, subida]);
        let lineas: Vec<&str> = texto.lines().collect();
        assert_eq!(lineas.len(), 3, "{texto}");
        for columna in [
            "HOST",
            "NOMBRE",
            "DIRECCIÓN",
            "BORRAR",
            "ÚLTIMO RESULTADO",
            "RUTAS",
        ] {
            assert!(lineas[0].contains(columna), "falta {columna}: {texto}");
        }
        // El orden es el de la lista (el almacén ya ordena por host y nombre).
        assert!(lineas[1].starts_with("hetzner-01"), "{texto}");
        assert!(lineas[1].contains(" logs "), "{texto}");
        assert!(lineas[1].contains(" bajada "), "{texto}");
        assert!(lineas[1].contains(" sí "), "{texto}");
        assert!(lineas[1].contains(" nunca "), "{texto}");
        assert!(
            lineas[1].ends_with("/home/hector/logs ← /var/log/nginx"),
            "{texto}"
        );
        assert!(lineas[2].contains(" web-prod "), "{texto}");
        assert!(lineas[2].contains(" subida "), "{texto}");
        assert!(lineas[2].contains(" no "), "{texto}");
        assert!(lineas[2].contains(" ok · 2026-10-02 14:31 "), "{texto}");
        assert!(
            lineas[2].ends_with("/home/hector/proyectos/cooperapp → /var/www/cooperapp"),
            "{texto}"
        );
    }

    #[test]
    fn las_columnas_quedan_alineadas() {
        let mut larga = guardada(
            "un-host-con-un-nombre-muy-largo",
            "una-sincronizacion-con-nombre-largo",
            Direccion::Subida,
        );
        larga.ultima_ejecucion_en = Some("2026-10-02T14:31:05+02:00".to_string());
        larga.ultimo_resultado = Some(ResultadoSincronizacion::Parcial);
        let corta = guardada("db", "x", Direccion::Bajada);
        let texto = listado_cli(&[larga, corta]);
        // Las rutas empiezan en la misma columna en todas las filas.
        let columna_de = |linea: &str, buscado: &str| {
            let byte = linea.find(buscado).expect("columna de rutas");
            linea[..byte].chars().count()
        };
        let inicio_rutas: Vec<usize> = texto
            .lines()
            .enumerate()
            .map(|(indice, linea)| columna_de(linea, if indice == 0 { "RUTAS" } else { "/home" }))
            .collect();
        assert!(
            inicio_rutas.windows(2).all(|par| par[0] == par[1]),
            "{inicio_rutas:?}\n{texto}"
        );
        assert!(texto.contains("un-host-con-un-…"), "{texto}");
        assert!(texto.contains("parcial · 2026-10-02 14:31"), "{texto}");
    }

    #[test]
    fn el_ultimo_resultado_dice_si_no_llego_a_escribirse() {
        let mut lanzada = guardada("hetzner-01", "web-prod", Direccion::Subida);
        lanzada.ultima_ejecucion_en = Some("2026-10-02T14:31:05+02:00".to_string());
        assert_eq!(
            ultimo_resultado(&lanzada),
            "sin resultado · 2026-10-02 14:31"
        );
        lanzada.ultimo_resultado = Some(ResultadoSincronizacion::Cancelada);
        assert_eq!(ultimo_resultado(&lanzada), "cancelada · 2026-10-02 14:31");
        // Una fecha ilegible se enseña tal cual (sin escapes).
        lanzada.ultima_ejecucion_en = Some("ayer\u{1b}[2J".to_string());
        assert_eq!(ultimo_resultado(&lanzada), "cancelada · ayer");
    }

    #[test]
    fn ningun_escape_de_terminal_llega_a_la_salida() {
        let mut sucia = guardada("hetzner-01", "web-prod", Direccion::Subida);
        sucia.ruta_local = "/tmp/\u{1b}[2Jmal".to_string();
        let texto = listado_cli(&[sucia]);
        assert!(!texto.contains('\u{1b}'), "{texto:?}");
        assert!(texto.contains("/tmp/mal → /var/www/cooperapp"), "{texto}");
    }
}
