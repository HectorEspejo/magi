//! Lo propio de Termux (Fase 17): rutas, órdenes `termux-*`, propiedades de
//! Android, portapapeles, wake lock, avisos, fila de teclas, diagnóstico y CLI.
//!
//! Se elige al compilar (`cfg(target_os = "android")`); en Linux solo se activa
//! en compilaciones de depuración con `MAGI_PLATAFORMA=termux`, para las
//! pruebas. Este módulo existe siempre (también en el binario de Linux, donde
//! no hace nada) y es el único sitio del crate que pregunta por la plataforma:
//! el resto solo llama a su interfaz. Nada de lo de aquí decide por
//! `EQUIPOS.plataforma`.
//!
//! La lógica nunca lee `std::env`: recibe un `EntornoTermux` (el del proceso en
//! producción, uno de mentira en las pruebas). Las únicas excepciones son
//! `es_termux`, que decide una vez al arrancar, y `EntornoTermux::del_proceso`,
//! que es quien construye el entorno real.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use magi_nucleo::llavero_fichero::{LlaveroFichero, NOMBRE_FICHERO};

use crate::config::Rutas;

pub mod avisos;
pub mod cli;
pub mod diagnostico;
pub mod estado;
pub mod ordenes;
pub mod portapapeles;
pub mod propiedades;
pub mod rutas;
pub mod teclas;
pub mod wake_lock;

pub use ordenes::{ApiProceso, EstadoApi, FalloOrden, Ordenes};
pub use propiedades::Propiedades;
pub use rutas::{EntornoTermux, RutasTermux};

/// Decide si este proceso se porta como Termux. Pura, para probarla: en
/// Android siempre; en otro sistema, solo con una compilación de depuración y
/// `MAGI_PLATAFORMA=termux`. Un binario de release de Linux con la variable
/// puesta se porta como en Linux.
pub fn decidir(
    compilado_para_android: bool,
    depuracion: bool,
    magi_plataforma: Option<&str>,
) -> bool {
    compilado_para_android || (depuracion && magi_plataforma == Some("termux"))
}

/// ¿Es este proceso un MAGI de Termux? Se decide una vez, al primer uso.
pub fn es_termux() -> bool {
    static ES_TERMUX: OnceLock<bool> = OnceLock::new();
    *ES_TERMUX.get_or_init(|| {
        decidir(
            cfg!(target_os = "android"),
            cfg!(debug_assertions),
            std::env::var("MAGI_PLATAFORMA").ok().as_deref(),
        )
    })
}

/// El llavero de fichero de este equipo, si es Termux: `llavero.age` en el
/// directorio de datos. `None` fuera de Termux.
pub fn llavero_fichero(rutas: &Rutas) -> Option<Arc<LlaveroFichero>> {
    es_termux().then(|| Arc::new(LlaveroFichero::nuevo(rutas.datos.join(NOMBRE_FICHERO))))
}

/// La shell con la que corre el comando local de CASPER-3 en Termux
/// (`$PREFIX/bin/sh`, §7.1): en Termux no existe `/bin/sh` y no se debe confiar
/// en lo que haya en el `PATH`. `None` fuera de Termux, donde sigue el `sh` del
/// núcleo; `Some(Err(..))` es el error con instrucción si no se pudo calcular
/// (sin `$PREFIX`). Quien la pide la fija al abrir su diálogo y no la relee.
pub fn shell_local() -> Option<Result<PathBuf, String>> {
    es_termux().then(|| shell_de(&EntornoTermux::del_proceso()))
}

/// La shell local de un entorno dado (la parte de [`shell_local`] que se
/// puede probar sin depender del proceso).
pub fn shell_de(entorno: &EntornoTermux) -> Result<PathBuf, String> {
    rutas::calcular(entorno).map(|rutas| rutas.sh)
}

/// Nombre por defecto del equipo en Termux: `termux-<modelo>` con la regla de
/// la etiqueta de la F15 («ABC-X10» → «termux-abc-x10», 20 caracteres como
/// mucho). El nombre del sistema en Termux es `localhost` y no sirve.
pub fn nombre_equipo(modelo: &str) -> String {
    magi_nucleo::terminal::tmux::etiqueta_equipo(&format!("termux {modelo}"))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn solo_se_elige_termux_en_android_o_en_depuracion_con_la_variable() {
        assert!(decidir(true, false, None), "compilado para Android");
        assert!(
            decidir(false, true, Some("termux")),
            "depuración y variable"
        );
        assert!(
            !decidir(false, false, Some("termux")),
            "un release de Linux con la variable se porta como en Linux"
        );
        assert!(!decidir(false, true, None));
        assert!(!decidir(false, true, Some("linux")));
    }

    #[test]
    fn el_nombre_del_equipo_sale_del_modelo() {
        assert_eq!(nombre_equipo("ABC-X10"), "termux-abc-x10");
        assert_eq!(nombre_equipo(""), "termux");
    }
}
