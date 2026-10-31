//! Estado de la sincronización del inventario en la TUI (Fase 10): vistas
//! SINCRONIZACIÓN y CONFLICTOS, indicador de la barra, diálogos y disparo de
//! las pasadas.
//!
//! El bucle de UI es el único escritor de SQLite, así que las esperas (clave
//! del llavero, cerrojo entre procesos, retraso de exportación y vigilancia de
//! la carpeta) viven en tareas de tokio; cuando el cerrojo está libre, la tarea
//! devuelve EventoInventario::Listo y es el bucle quien ejecuta
//! sincronizacion::pasada con su propia conexión.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::almacen::conflictos::Conflicto;
use crate::almacen::equipos::Equipo;
use crate::app::Evento;
use crate::sincronizacion::carpeta::Cerrojo;
use crate::sincronizacion::cifrado::ClaveSincronizacion;
use crate::sincronizacion::{
    BorradosPendientes, EstadoPasada, PoliticaBorrados, ResumenImportacion,
};
use crate::ui::Vista;

/// Lo que la tarea de fondo devuelve al bucle de la UI.
pub enum EventoInventario {
    /// El cerrojo está libre: el bucle ya puede hacer la pasada con su
    /// conexión (es el único escritor de SQLite).
    Listo {
        cerrojo: Cerrojo,
        claves: Vec<ClaveSincronizacion>,
    },
    /// No se pudo empezar la pasada (llavero, carpeta o cerrojo ocupado).
    Error(String),
    /// La carpeta compartida ha cambiado (vigilancia o sondeo de respaldo).
    CarpetaCambiada { generacion: u64 },
}

/// Estado de la interfaz de sincronización. Vive en la App y sobrevive al
/// cambiar de vista.
pub struct EstadoSincronizacion {
    /// Filas de EQUIPOS, este equipo primero.
    pub equipos: Vec<Equipo>,
    /// Conflictos pendientes, del más antiguo al más nuevo.
    pub conflictos: Vec<Conflicto>,
    pub seleccion: usize,
    /// Primera fila visible de la lista de equipos (la fija el pintado).
    pub desplazamiento: usize,
    pub seleccion_conflicto: usize,
    /// Primera fila visible de la lista de conflictos (la fija el pintado).
    pub desplazamiento_conflicto: usize,
    /// Huella corta de la clave del llavero, para comparar a simple vista.
    pub huella: Option<String>,
    /// Estado de la última pasada.
    pub estado: EstadoPasada,
    /// Resumen de la última importación aplicada, si la hubo.
    pub resumen: Option<ResumenImportacion>,
    /// Avisos de la última pasada, legibles en la vista.
    pub avisos: Vec<String>,
    pub conflictos_pendientes: i64,
    /// Detalle del conflicto seleccionado (lo prepara la App al moverse).
    pub detalle: Vec<String>,
    /// Hay una pasada en marcha (esperando cerrojo, clave o aplicando).
    pub sincronizando: bool,
    /// Política con la que se pedirá la próxima pasada.
    pub politica: PoliticaBorrados,
    /// Borrados masivos esperando decisión: la sincronización está en pausa.
    pub pausa: Option<BorradosPendientes>,
    /// Se pidió otra pasada mientras una estaba en marcha.
    pub reintentar: bool,
    /// Cuándo terminó la última pasada (para «hace N»).
    pub terminada_en: Option<chrono::DateTime<chrono::Local>>,
    /// Último reloj híbrido visto de este equipo: detecta cambios locales.
    pub reloj_visto: Option<i64>,
    /// Primer instante en que se vio un cambio local pendiente de exportar.
    pub cambio_desde: Option<Instant>,
    /// Vigilancia de la carpeta en marcha; se sustituye al cambiar de carpeta.
    pub vigilancia: Option<JoinHandle<()>>,
    pub generacion_vigilancia: u64,
    /// Vista a la que vuelve q desde el área de sincronización.
    pub vista_previa: Option<Vista>,
}

impl Default for EstadoSincronizacion {
    fn default() -> Self {
        Self {
            equipos: Vec::new(),
            conflictos: Vec::new(),
            seleccion: 0,
            desplazamiento: 0,
            seleccion_conflicto: 0,
            desplazamiento_conflicto: 0,
            huella: None,
            estado: EstadoPasada::SinConfigurar,
            resumen: None,
            avisos: Vec::new(),
            conflictos_pendientes: 0,
            detalle: Vec::new(),
            sincronizando: false,
            politica: PoliticaBorrados::Preguntar,
            pausa: None,
            reintentar: false,
            terminada_en: None,
            reloj_visto: None,
            cambio_desde: None,
            vigilancia: None,
            generacion_vigilancia: 0,
            vista_previa: None,
        }
    }
}

impl EstadoSincronizacion {
    /// Clava la selección de las dos listas dentro de lo que haya.
    pub fn ajustar_selecciones(&mut self) {
        if self.seleccion >= self.equipos.len() {
            self.seleccion = self.equipos.len().saturating_sub(1);
        }
        if self.seleccion_conflicto >= self.conflictos.len() {
            self.seleccion_conflicto = self.conflictos.len().saturating_sub(1);
        }
    }

    /// Equipo seleccionado.
    pub fn equipo_seleccionado(&self) -> Option<&Equipo> {
        self.equipos.get(self.seleccion)
    }

    /// Conflicto seleccionado.
    pub fn conflicto_seleccionado(&self) -> Option<&Conflicto> {
        self.conflictos.get(self.seleccion_conflicto)
    }

    /// Texto del estado de la pasada para la cabecera de la vista.
    pub fn texto_estado(&self) -> String {
        match &self.estado {
            EstadoPasada::SinConfigurar => "sin configurar".to_string(),
            EstadoPasada::SinClave => "falta la clave en el llavero".to_string(),
            EstadoPasada::AlDia => "al día".to_string(),
            EstadoPasada::Quitado => "quitado de la sincronización".to_string(),
            EstadoPasada::EsperandoClave(nombre) => {
                format!("esperando la clave nueva ({nombre})")
            }
            EstadoPasada::DecisionPendiente(_) => "en pausa: decidir los borrados".to_string(),
            EstadoPasada::Error(motivo) => format!("error: {motivo}"),
        }
    }
}

/// Espera máxima por el cerrojo desde la TUI: si otra ventana sincroniza, se
/// reintenta y, si no se libera, la pasada se deja para la siguiente.
pub const ESPERA_CERROJO_TUI: Duration = Duration::from_secs(10);
/// Cada cuánto se reintenta el cerrojo mientras está ocupado.
const REINTENTO_CERROJO: Duration = Duration::from_millis(400);
/// Agrupación de los cambios de la carpeta (§4.2: 2 s).
pub const AGRUPACION_CARPETA: Duration = Duration::from_secs(2);
/// Sondeo de respaldo cuando notify no puede vigilar la carpeta.
pub const SONDEO_CARPETA: Duration = Duration::from_secs(30);

/// Lanza la tarea que espera el cerrojo y las claves y avisa al bucle. La
/// pasada la ejecuta la UI, que es el único escritor de SQLite.
pub fn lanzar_espera(
    runtime: &tokio::runtime::Runtime,
    eventos: mpsc::UnboundedSender<Evento>,
    rutas: crate::config::Rutas,
    config: crate::config::Config,
) {
    runtime.spawn(async move {
        let llavero = crate::llavero::LlaveroDelSistema;
        let claves = match crate::sincronizacion::equipos::claves_para_leer(&config, &llavero) {
            Ok(claves) => claves,
            Err(error) => {
                let _ = eventos.send(Evento::Inventario(EventoInventario::Error(error)));
                return;
            }
        };
        let limite = tokio::time::Instant::now() + ESPERA_CERROJO_TUI;
        loop {
            match crate::sincronizacion::carpeta::adquirir_cerrojo(&rutas, None) {
                Ok(Some(cerrojo)) => {
                    let _ = eventos.send(Evento::Inventario(EventoInventario::Listo {
                        cerrojo,
                        claves,
                    }));
                    return;
                }
                Ok(None) => {
                    if tokio::time::Instant::now() >= limite {
                        let _ = eventos.send(Evento::Inventario(EventoInventario::Error(
                            "otra ventana está sincronizando; se reintentará".to_string(),
                        )));
                        return;
                    }
                    tokio::time::sleep(REINTENTO_CERROJO).await;
                }
                Err(error) => {
                    let _ = eventos.send(Evento::Inventario(EventoInventario::Error(format!(
                        "no se pudo tomar el cerrojo de sincronización: {error}"
                    ))));
                    return;
                }
            }
        }
    });
}

/// Vigila la carpeta compartida: avisa de cada cambio agrupado 2 s. Con notify
/// si se puede; si no, sondea cada 30 s. La generación descarta los avisos de
/// una vigilancia que ya no está vigente.
pub async fn vigilar(carpeta: PathBuf, generacion: u64, eventos: mpsc::UnboundedSender<Evento>) {
    use notify::{RecursiveMode, Watcher};
    use tokio::sync::mpsc::unbounded_channel;

    let (avisos, mut cambios) = unbounded_channel::<()>();
    // El vigilante tiene que seguir vivo mientras dure la tarea: si se cae,
    // queda el sondeo de respaldo.
    let vigilante = notify::recommended_watcher(move |resultado: notify::Result<notify::Event>| {
        let Ok(evento) = resultado else {
            return;
        };
        if !evento
            .paths
            .iter()
            .any(|ruta| es_fichero_de_sincronizacion(ruta))
        {
            return;
        }
        let _ = avisos.send(());
    });
    let mut vigilante = match vigilante {
        Ok(mut vigilante) => match vigilante.watch(&carpeta, RecursiveMode::NonRecursive) {
            Ok(()) => Some(vigilante),
            Err(_) => None,
        },
        Err(_) => None,
    };
    if vigilante.is_some() {
        while cambios.recv().await.is_some() {
            tokio::time::sleep(AGRUPACION_CARPETA).await;
            while cambios.try_recv().is_ok() {}
            if eventos
                .send(Evento::Inventario(EventoInventario::CarpetaCambiada {
                    generacion,
                }))
                .is_err()
            {
                return;
            }
        }
        return;
    }
    // Respaldo: un sondeo cada 30 s mientras la tarea siga viva.
    let _ = &mut vigilante;
    loop {
        tokio::time::sleep(SONDEO_CARPETA).await;
        if eventos
            .send(Evento::Inventario(EventoInventario::CarpetaCambiada {
                generacion,
            }))
            .is_err()
        {
            return;
        }
    }
}

/// ¿El nombre del fichero es de un equipo de MAGI? Los temporales y las
/// copias en conflicto de la herramienta de sincronización se ignoran también
/// aquí, para no despertar la pasada por nada.
fn es_fichero_de_sincronizacion(ruta: &std::path::Path) -> bool {
    ruta.file_name()
        .and_then(|nombre| nombre.to_str())
        .and_then(crate::sincronizacion::carpeta::uuid_de_nombre)
        .is_some()
}

/// «hace 2 min» de una fecha RFC 3339, o None si no se puede leer.
pub fn hace(fecha: &str) -> Option<String> {
    let instante = chrono::DateTime::parse_from_rfc3339(fecha).ok()?;
    let segundos = (chrono::Local::now() - instante.with_timezone(&chrono::Local))
        .num_seconds()
        .max(0);
    Some(hace_segundos(segundos))
}

/// «hace N s/min/h/días» de una antigüedad en segundos.
pub fn hace_segundos(segundos: i64) -> String {
    match segundos {
        s if s < 60 => format!("hace {s} s"),
        s if s < 3600 => format!("hace {} min", s / 60),
        s if s < 86_400 => format!("hace {} h", s / 3600),
        86_400 => "hace 1 día".to_string(),
        s => format!("hace {} días", s / 86_400),
    }
}

/// Texto de una fecha absoluta corta (05/11 09:12).
pub fn fecha_corta(fecha: &str) -> String {
    let Ok(instante) = chrono::DateTime::parse_from_rfc3339(fecha) else {
        return "—".to_string();
    };
    instante
        .with_timezone(&chrono::Local)
        .format("%d/%m %H:%M")
        .to_string()
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn las_antiguedades_se_leen_en_castellano() {
        assert_eq!(hace_segundos(0), "hace 0 s");
        assert_eq!(hace_segundos(30), "hace 30 s");
        assert_eq!(hace_segundos(120), "hace 2 min");
        assert_eq!(hace_segundos(3_600), "hace 1 h");
        assert_eq!(hace_segundos(86_400), "hace 1 día");
        assert_eq!(hace_segundos(2 * 86_400), "hace 2 días");
    }

    #[test]
    fn una_fecha_ajena_no_rompe_la_vista() {
        assert_eq!(hace("no es una fecha"), None);
        assert_eq!(fecha_corta("no es una fecha"), "—");
    }

    #[test]
    fn solo_los_nombres_estrictos_despiertan_la_pasada() {
        let uuid = "2f9a1c3e-4b5d-4e6f-8a7b-9c0d1e2f3a4b";
        assert!(es_fichero_de_sincronizacion(std::path::Path::new(
            &format!("magi-{uuid}.age")
        )));
        assert!(!es_fichero_de_sincronizacion(std::path::Path::new(
            "magi-2f9a1c3e-4b5d-4e6f-8a7b-9c0d1e2f3a4b.sync-conflict-20260101.age"
        )));
        assert!(!es_fichero_de_sincronizacion(std::path::Path::new(
            "otro.txt"
        )));
    }
}
