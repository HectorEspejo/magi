//! Portapapeles de Termux: `termux-clipboard-set` → OSC 52 → aviso (§4.4, §7.4).
//!
//! Aquí vive solo la decisión, con funciones puras que se prueban sin Termux:
//! qué camino sigue una copia según el tamaño y el estado de Termux:API, los
//! topes, la secuencia OSC 52 y los textos de la barra. La ejecución (la orden
//! en una tarea con plazo, el evento de vuelta y la escritura del OSC 52 en la
//! terminal, que es cosa del hilo de la interfaz) vive en `app/portapapeles.rs`
//! y `app/termux.rs`.
//!
//! ```text
//! ¿Termux:API apagada? ─ no ─▶ termux-clipboard-set por la entrada estándar
//!     ├─ más de TOPE_API ─▶ «demasiado para el portapapeles (900 KiB): …»
//!     ├─ responde ─▶ «copiado (3,2 KiB)»
//!     └─ no responde o falla ─▶ Termux:API apagada (aviso una vez) ─▶ OSC 52
//! OSC 52 ─ base64 ≤ TOPE_OSC52 ─▶ «copiado»
//!        ─ más ─▶ «no cabe en el portapapeles de Termux (20 KiB): …»
//! ```
//!
//! MAGI nunca lee el portapapeles: pegar es cosa de Termux, que llega como
//! pegado entre corchetes.

use base64::Engine as _;

/// Lo máximo que se manda a `termux-clipboard-set` (512 KiB): la orden recibe
/// el texto por la entrada estándar, nunca por un argumento.
pub const TOPE_API: usize = 512 * 1024;

/// Lo máximo que cabe en un OSC 52, ya en base64: con más, Termux no copia
/// nada. PROVISIONAL-POC: la prueba de concepto en el plegable decide su valor
/// (una sola constante).
pub const TOPE_OSC52: usize = 6000;

/// Lo que dice la barra, una sola vez por proceso, cuando Termux:API se apaga
/// (§6 «Marcas»): el efecto, no el motivo.
pub const AVISO_API_APAGADA: &str =
    "Termux:API no responde: se copia por OSC 52 y no habrá avisos de Android";

/// Lo que dice la barra tras copiar por OSC 52.
pub const COPIADO_OSC52: &str = "copiado";

/// Por dónde sigue una copia.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanCopia {
    /// `termux-clipboard-set` con el texto por la entrada estándar.
    Api,
    /// OSC 52 desde el hilo de la interfaz: el texto cabe.
    Osc52,
    /// No se copia: el texto es el mensaje completo de la barra.
    Rechazada(String),
}

/// El camino de una copia de `bytes` bytes. Con Termux:API apagada va directa
/// al OSC 52; con la API disponible, hasta [`TOPE_API`] va por ella y con más
/// se rechaza (no se recorta ni se parte: se dice cómo guardarlo).
#[must_use]
pub fn planificar(bytes: usize, api_apagada: bool) -> PlanCopia {
    if api_apagada {
        return plan_osc52(bytes);
    }
    if bytes > TOPE_API {
        return PlanCopia::Rechazada(mensaje_demasiado(bytes));
    }
    PlanCopia::Api
}

/// El camino por OSC 52 (la API apagada, o fallando): cabe, o se rechaza con
/// el tamaño y la alternativa. Nunca se manda un OSC 52 que Termux ignoraría.
#[must_use]
pub fn plan_osc52(bytes: usize) -> PlanCopia {
    if longitud_base64(bytes) <= TOPE_OSC52 {
        PlanCopia::Osc52
    } else {
        PlanCopia::Rechazada(mensaje_no_cabe(bytes))
    }
}

/// Longitud en base64 (con relleno) de `bytes` bytes, sin codificar nada: se
/// usa para decidir antes de gastar memoria con un texto enorme.
#[must_use]
pub fn longitud_base64(bytes: usize) -> usize {
    bytes.div_ceil(3).saturating_mul(4)
}

/// La secuencia OSC 52 que copia `datos` al portapapeles del terminal:
/// `ESC ] 52 ; c ; <base64> BEL`. Quien la llama ya comprobó con [`plan_osc52`]
/// que cabe.
#[must_use]
pub fn secuencia_osc52(datos: &[u8]) -> Vec<u8> {
    let codificado = base64::engine::general_purpose::STANDARD.encode(datos);
    let mut secuencia = Vec::with_capacity(codificado.len() + 8);
    secuencia.extend_from_slice(b"\x1b]52;c;");
    secuencia.extend_from_slice(codificado.as_bytes());
    secuencia.push(0x07);
    secuencia
}

/// Tamaño legible en KiB (o MiB desde 1024 KiB), con coma decimal y sin el
/// «,0» de las cifras redondas: «312 B», «3,2 KiB», «20 KiB», «900 KiB»,
/// «2,5 MiB». Con enteros, sin pasar por coma flotante.
#[must_use]
pub fn tamano_kib(bytes: usize) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    // Décimas de KiB, redondeando a la más cercana.
    let decimas = bytes.saturating_mul(10).saturating_add(512) / 1024;
    if decimas < 10 * 1024 {
        return con_decimas(decimas, "KiB");
    }
    let decimas_mib = bytes.saturating_mul(10).saturating_add(512 * 1024) / (1024 * 1024);
    con_decimas(decimas_mib, "MiB")
}

fn con_decimas(decimas: usize, unidad: &str) -> String {
    let (entero, resto) = (decimas / 10, decimas % 10);
    if resto == 0 {
        format!("{entero} {unidad}")
    } else {
        format!("{entero},{resto} {unidad}")
    }
}

/// «copiado (3,2 KiB)»: Termux:API respondió.
#[must_use]
pub fn mensaje_copiado_api(bytes: usize) -> String {
    format!("copiado ({})", tamano_kib(bytes))
}

/// Más de [`TOPE_API`] con la API disponible.
#[must_use]
pub fn mensaje_demasiado(bytes: usize) -> String {
    format!(
        "demasiado para el portapapeles ({}): guárdalo en un fichero",
        tamano_kib(bytes)
    )
}

/// Con la API apagada, un texto que no cabe en el OSC 52.
#[must_use]
pub fn mensaje_no_cabe(bytes: usize) -> String {
    format!(
        "no cabe en el portapapeles de Termux ({}): instala Termux:API o guárdalo",
        tamano_kib(bytes)
    )
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_tamano_se_dice_en_kib_con_coma_y_sin_decimales_redondos() {
        assert_eq!(tamano_kib(0), "0 B");
        assert_eq!(tamano_kib(312), "312 B");
        assert_eq!(tamano_kib(1023), "1023 B");
        assert_eq!(tamano_kib(1024), "1 KiB");
        assert_eq!(tamano_kib(3277), "3,2 KiB");
        assert_eq!(tamano_kib(20 * 1024), "20 KiB");
        assert_eq!(tamano_kib(20 * 1024 + 51), "20 KiB", "50 B no llegan a 0,1");
        assert_eq!(tamano_kib(20 * 1024 + 52), "20,1 KiB");
        assert_eq!(tamano_kib(900 * 1024), "900 KiB");
        assert_eq!(tamano_kib(TOPE_API), "512 KiB");
        // Redondea a la décima más cercana, también en la frontera de la unidad.
        assert_eq!(tamano_kib(1024 * 1024 - 1), "1 MiB");
        assert_eq!(tamano_kib(1024 * 1024), "1 MiB");
        assert_eq!(tamano_kib(2_621_440), "2,5 MiB");
        assert_eq!(tamano_kib(usize::MAX).chars().last(), Some('B'));
    }

    #[test]
    fn los_textos_de_la_barra_son_los_de_la_especificacion() {
        assert_eq!(mensaje_copiado_api(3277), "copiado (3,2 KiB)");
        assert_eq!(
            mensaje_demasiado(900 * 1024),
            "demasiado para el portapapeles (900 KiB): guárdalo en un fichero"
        );
        assert_eq!(
            mensaje_no_cabe(20 * 1024),
            "no cabe en el portapapeles de Termux (20 KiB): instala Termux:API o guárdalo"
        );
        assert_eq!(
            AVISO_API_APAGADA,
            "Termux:API no responde: se copia por OSC 52 y no habrá avisos de Android"
        );
        assert_eq!(COPIADO_OSC52, "copiado");
    }

    #[test]
    fn con_la_api_disponible_hasta_512_kib_va_por_la_orden() {
        assert_eq!(planificar(1, false), PlanCopia::Api);
        assert_eq!(planificar(TOPE_API, false), PlanCopia::Api);
        assert_eq!(
            planificar(TOPE_API + 1, false),
            PlanCopia::Rechazada(mensaje_demasiado(TOPE_API + 1))
        );
        assert_eq!(
            planificar(900 * 1024, false),
            PlanCopia::Rechazada(
                "demasiado para el portapapeles (900 KiB): guárdalo en un fichero".to_string()
            ),
            "con la API disponible no se cae al OSC 52: se pide guardarlo"
        );
    }

    #[test]
    fn con_la_api_apagada_manda_el_osc_52_si_cabe_y_si_no_lo_dice() {
        assert_eq!(planificar(1, true), PlanCopia::Osc52);
        // 4500 bytes son justo 6000 en base64.
        assert_eq!(longitud_base64(4500), 6000);
        assert_eq!(planificar(4500, true), PlanCopia::Osc52);
        assert_eq!(longitud_base64(4501), 6004);
        assert_eq!(
            planificar(4501, true),
            PlanCopia::Rechazada(mensaje_no_cabe(4501))
        );
        assert_eq!(
            planificar(20 * 1024, true),
            PlanCopia::Rechazada(
                "no cabe en el portapapeles de Termux (20 KiB): instala Termux:API o guárdalo"
                    .to_string()
            )
        );
        // Un texto enorme se rechaza sin codificarlo.
        assert!(matches!(
            planificar(usize::MAX, true),
            PlanCopia::Rechazada(_)
        ));
        // Tras un fallo de la API, el mismo criterio.
        assert_eq!(plan_osc52(10), PlanCopia::Osc52);
        assert_eq!(
            plan_osc52(TOPE_API),
            PlanCopia::Rechazada(mensaje_no_cabe(TOPE_API))
        );
    }

    #[test]
    fn la_longitud_en_base64_coincide_con_la_real() {
        for bytes in 0..64usize {
            let datos = vec![b'x'; bytes];
            let real = base64::engine::general_purpose::STANDARD.encode(&datos);
            assert_eq!(longitud_base64(bytes), real.len(), "{bytes} bytes");
        }
        // Sin desbordar con un tamaño absurdo.
        assert_eq!(longitud_base64(usize::MAX), usize::MAX);
    }

    #[test]
    fn la_secuencia_osc_52_es_esc_52_c_base64_bel() {
        assert_eq!(secuencia_osc52(b"hola"), b"\x1b]52;c;aG9sYQ==\x07".to_vec());
        assert_eq!(secuencia_osc52(b""), b"\x1b]52;c;\x07".to_vec());
        // Lo que cabe en el tope, con su envoltorio, es el tope más 8 bytes.
        let datos = vec![b'a'; 4500];
        let secuencia = secuencia_osc52(&datos);
        assert_eq!(secuencia.len(), TOPE_OSC52 + 7 + 1);
        assert!(secuencia.starts_with(b"\x1b]52;c;"));
        assert_eq!(secuencia.last(), Some(&0x07));
    }
}
