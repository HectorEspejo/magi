//! Deliberación MAGI (Fase 6): la pausa obligatoria antes de una ejecución
//! arriesgada. Tres comprobaciones por host —MELCHIOR-1 salud, BALTHASAR-2
//! backup reciente, CASPER-3 tests en verde—, unanimidad para ejecutar y
//! forzado solo con motivo escrito, que queda registrado.
//!
//! Este módulo tiene el modelo (qué exige cada host, veredictos, la fila de
//! `DELIBERACIONES`); las comprobaciones y su orquestación, en sus submódulos.

pub mod backup;
pub mod estado;
pub mod salud;
pub mod tests;

use serde::{Deserialize, Serialize};

/// Qué comprobaciones exige un host antes de una ejecución
/// (`VERIFICACIONES_HOST`; sin fila, ninguna).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Verificaciones {
    pub host_id: i64,
    pub salud: bool,
    pub backup: bool,
    /// Directorio remoto de los backups.
    pub backup_ruta: Option<String>,
    /// Glob opcional sobre el nombre de fichero (`*.sql.gz`).
    pub backup_patron: Option<String>,
    pub tests: bool,
    /// Comando local (`sh -c`) que dice si los tests están en verde.
    pub tests_comando: Option<String>,
    pub actualizado_en: String,
}

impl Verificaciones {
    pub fn alguna_activa(&self) -> bool {
        self.salud || self.backup || self.tests
    }

    pub fn activa(&self, comprobacion: Comprobacion) -> bool {
        match comprobacion {
            Comprobacion::Salud => self.salud,
            Comprobacion::Backup => self.backup,
            Comprobacion::Tests => self.tests,
        }
    }

    pub fn datos(&self) -> DatosVerificaciones {
        DatosVerificaciones {
            salud: self.salud,
            backup: self.backup,
            backup_ruta: self.backup_ruta.clone(),
            backup_patron: self.backup_patron.clone(),
            tests: self.tests,
            tests_comando: self.tests_comando.clone(),
        }
    }
}

/// Lo que se edita en el bloque «Verificaciones previas» de la ficha.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DatosVerificaciones {
    pub salud: bool,
    pub backup: bool,
    pub backup_ruta: Option<String>,
    pub backup_patron: Option<String>,
    pub tests: bool,
    pub tests_comando: Option<String>,
}

impl DatosVerificaciones {
    pub fn alguna_activa(&self) -> bool {
        self.salud || self.backup || self.tests
    }

    /// Recorta los textos y deja en `None` los vacíos.
    pub fn normalizada(mut self) -> Self {
        let limpiar = |texto: Option<String>| {
            texto
                .map(|texto| texto.trim().to_string())
                .filter(|texto| !texto.is_empty())
        };
        self.backup_ruta = limpiar(self.backup_ruta);
        self.backup_patron = limpiar(self.backup_patron);
        self.tests_comando = limpiar(self.tests_comando);
        self
    }
}

/// Valida el bloque de la ficha: con backup, la ruta es obligatoria (absoluta
/// o relativa al inicio del usuario remoto) y el patrón, si lo hay, un glob
/// válido; con tests, el comando es obligatorio.
pub fn validar_verificaciones(datos: &DatosVerificaciones) -> Result<(), String> {
    if datos.backup {
        let Some(ruta) = datos.backup_ruta.as_deref() else {
            return Err("la comprobación de backup necesita el directorio remoto".to_string());
        };
        if ruta.chars().any(char::is_control) {
            return Err("la ruta del backup no puede llevar caracteres de control".to_string());
        }
    }
    if let Some(patron) = datos.backup_patron.as_deref() {
        globset::Glob::new(patron)
            .map_err(|error| format!("el patrón del backup no es válido: {error}"))?;
    }
    if datos.tests && datos.tests_comando.is_none() {
        return Err("la comprobación de tests necesita un comando".to_string());
    }
    Ok(())
}

/// Las tres comprobaciones, con su nombre MAGI y la palabra que dice qué
/// comprueban (la metáfora nunca va sola).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Comprobacion {
    Salud,
    Backup,
    Tests,
}

impl Comprobacion {
    pub const TODAS: [Comprobacion; 3] = [
        Comprobacion::Salud,
        Comprobacion::Backup,
        Comprobacion::Tests,
    ];

    pub fn nombre_magi(self) -> &'static str {
        match self {
            Comprobacion::Salud => "MELCHIOR-1",
            Comprobacion::Backup => "BALTHASAR-2",
            Comprobacion::Tests => "CASPER-3",
        }
    }

    pub fn palabra(self) -> &'static str {
        match self {
            Comprobacion::Salud => "salud",
            Comprobacion::Backup => "backup",
            Comprobacion::Tests => "tests",
        }
    }
}

/// Veredicto de una comprobación en un host. Siempre con el dato que lo
/// justifica: nunca un ✕ sin motivo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "estado")]
pub enum Veredicto {
    /// La comprobación no está activa en ese host (`—`).
    #[serde(rename = "n/a")]
    NoActiva,
    /// Aún comprobando (`◐`); en la fila de una deliberación cancelada.
    #[serde(rename = "pendiente")]
    Pendiente,
    #[serde(rename = "aprueba")]
    Aprueba { detalle: String, ms: u64 },
    #[serde(rename = "rechaza")]
    Rechaza { detalle: String, ms: u64 },
}

impl Veredicto {
    pub fn cuenta(&self) -> bool {
        !matches!(self, Veredicto::NoActiva)
    }

    pub fn aprueba(&self) -> bool {
        matches!(self, Veredicto::Aprueba { .. })
    }

    pub fn rechaza(&self) -> bool {
        matches!(self, Veredicto::Rechaza { .. })
    }

    pub fn detalle(&self) -> Option<&str> {
        match self {
            Veredicto::Aprueba { detalle, .. } | Veredicto::Rechaza { detalle, .. } => {
                Some(detalle)
            }
            _ => None,
        }
    }
}

/// Las tres celdas de un host en la deliberación (`comprobaciones_json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComprobacionesHost {
    pub host_id: i64,
    pub host: String,
    pub salud: Veredicto,
    pub backup: Veredicto,
    pub tests: Veredicto,
}

impl ComprobacionesHost {
    pub fn veredicto(&self, comprobacion: Comprobacion) -> &Veredicto {
        match comprobacion {
            Comprobacion::Salud => &self.salud,
            Comprobacion::Backup => &self.backup,
            Comprobacion::Tests => &self.tests,
        }
    }
}

/// Cómo se resolvió una deliberación (`DELIBERACIONES.resultado`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultadoDeliberacion {
    Aprobada,
    Forzada,
    Cancelada,
}

impl ResultadoDeliberacion {
    pub fn como_texto(self) -> &'static str {
        match self {
            ResultadoDeliberacion::Aprobada => "aprobada",
            ResultadoDeliberacion::Forzada => "forzada",
            ResultadoDeliberacion::Cancelada => "cancelada",
        }
    }

    pub fn desde_texto(texto: &str) -> Option<Self> {
        match texto {
            "aprobada" => Some(ResultadoDeliberacion::Aprobada),
            "forzada" => Some(ResultadoDeliberacion::Forzada),
            "cancelada" => Some(ResultadoDeliberacion::Cancelada),
            _ => None,
        }
    }
}

/// Cómo acabó la ejecución deliberada (`DELIBERACIONES.ejecucion_resultado`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EjecucionResultado {
    Ok,
    Parcial,
    Error,
    Cancelada,
}

impl EjecucionResultado {
    pub fn como_texto(self) -> &'static str {
        match self {
            EjecucionResultado::Ok => "ok",
            EjecucionResultado::Parcial => "parcial",
            EjecucionResultado::Error => "error",
            EjecucionResultado::Cancelada => "cancelada",
        }
    }

    pub fn desde_texto(texto: &str) -> Option<Self> {
        match texto {
            "ok" => Some(EjecucionResultado::Ok),
            "parcial" => Some(EjecucionResultado::Parcial),
            "error" => Some(EjecucionResultado::Error),
            "cancelada" => Some(EjecucionResultado::Cancelada),
            _ => None,
        }
    }

    /// `ok` si todos ok; `error` si ninguno; `parcial` en otro caso.
    pub fn de_hosts(ok: usize, total: usize, cancelada: bool) -> Self {
        if cancelada {
            EjecucionResultado::Cancelada
        } else if total > 0 && ok == total {
            EjecucionResultado::Ok
        } else if ok == 0 {
            EjecucionResultado::Error
        } else {
            EjecucionResultado::Parcial
        }
    }
}

/// Una deliberación resuelta, lista para insertar.
#[derive(Debug, Clone, PartialEq)]
pub struct NuevaDeliberacion {
    pub snippet_id: Option<i64>,
    /// «reiniciar nginx → hetzner-01, hetzner-02».
    pub accion: String,
    pub hosts: Vec<(i64, String)>,
    pub comprobaciones: Vec<ComprobacionesHost>,
    pub resultado: ResultadoDeliberacion,
    /// Hubo al menos un rechazo.
    pub bloqueada: bool,
    pub motivo: Option<String>,
    pub usuario: String,
}

/// Una fila de `DELIBERACIONES`.
#[derive(Debug, Clone, PartialEq)]
pub struct RegistroDeliberacion {
    pub id: i64,
    pub fecha: String,
    pub snippet_id: Option<i64>,
    pub accion: String,
    pub hosts: Vec<(i64, String)>,
    pub comprobaciones: Vec<ComprobacionesHost>,
    pub resultado: ResultadoDeliberacion,
    pub bloqueada: bool,
    pub motivo: Option<String>,
    pub usuario: String,
    pub ejecucion_resultado: Option<EjecucionResultado>,
}

impl RegistroDeliberacion {
    /// «N / M»: comprobaciones activas que aprobaron sobre las activas.
    pub fn consenso(&self) -> (usize, usize) {
        let celdas = self.comprobaciones.iter().flat_map(|host| {
            Comprobacion::TODAS
                .iter()
                .map(move |comprobacion| host.veredicto(*comprobacion))
        });
        let (mut aprobadas, mut activas) = (0, 0);
        for celda in celdas.filter(|celda| celda.cuenta()) {
            activas += 1;
            if celda.aprueba() {
                aprobadas += 1;
            }
        }
        (aprobadas, activas)
    }
}

/// Prefijo del detalle de las anotaciones `deliberacion_*` en `REGISTRO`: con
/// él, la vista Registro encuentra la fila de `DELIBERACIONES`.
const PREFIJO_DETALLE: &str = "deliberación #";

/// Detalle compacto de una anotación `deliberacion_*`: acción, hosts,
/// consenso, motivo, usuario y, si ya se ejecutó, el resultado. Los veredictos
/// por celda viven en `DELIBERACIONES`, no en el registro.
pub fn detalle_registro(
    deliberacion: &RegistroDeliberacion,
    resultado: Option<EjecucionResultado>,
) -> String {
    let (aprobadas, activas) = deliberacion.consenso();
    let mut detalle = format!(
        "{PREFIJO_DETALLE}{} · {} · {} host(s) · consenso {aprobadas}/{activas}",
        deliberacion.id,
        deliberacion.accion,
        deliberacion.hosts.len()
    );
    if let Some(motivo) = &deliberacion.motivo {
        detalle.push_str(&format!(" · motivo «{motivo}»"));
    }
    detalle.push_str(&format!(" · usuario {}", deliberacion.usuario));
    if let Some(resultado) = resultado {
        detalle.push_str(&format!(" · resultado {}", resultado.como_texto()));
    }
    detalle
}

/// Id de la deliberación en el detalle de una anotación `deliberacion_*`.
pub fn id_en_detalle(detalle: &str) -> Option<i64> {
    let resto = detalle.strip_prefix(PREFIJO_DETALLE)?;
    let numero: String = resto.chars().take_while(char::is_ascii_digit).collect();
    numero.parse().ok()
}

/// Líneas del detalle de una deliberación para la vista Registro: cómo se
/// resolvió, quién, el motivo y el veredicto de cada comprobación por host
/// (leídos de `DELIBERACIONES`). Cada línea cabe en `ancho` caracteres.
pub fn lineas_detalle(deliberacion: &RegistroDeliberacion, ancho: usize) -> Vec<String> {
    // Se sanea cada dato (vienen de remotos y comandos) y se acota la línea
    // entera sin tocar la sangría.
    let limpio = |texto: &str| crate::snippets::salida::sanear_linea(texto, ancho);
    let recortar = |texto: String| {
        if texto.chars().count() <= ancho {
            texto
        } else {
            let mut recortado: String = texto.chars().take(ancho.saturating_sub(1)).collect();
            recortado.push('…');
            recortado
        }
    };
    let (aprobadas, activas) = deliberacion.consenso();
    let mut lineas = vec![
        format!(
            "DELIBERACIÓN #{} · {}{}",
            deliberacion.id,
            deliberacion.resultado.como_texto(),
            if deliberacion.bloqueada {
                " · hubo rechazos"
            } else {
                ""
            }
        ),
        recortar(format!("Acción:    {}", limpio(&deliberacion.accion))),
        recortar(format!("Usuario:   {}", limpio(&deliberacion.usuario))),
    ];
    if let Some(motivo) = &deliberacion.motivo {
        lineas.push(recortar(format!("Motivo:    {}", limpio(motivo))));
    }
    let ejecucion = match (deliberacion.resultado, deliberacion.ejecucion_resultado) {
        (ResultadoDeliberacion::Cancelada, _) => "no se ejecutó",
        (_, Some(resultado)) => resultado.como_texto(),
        (_, None) => "sin resultado aún",
    };
    lineas.push(format!("Ejecución: {ejecucion}"));
    lineas.push(format!("Consenso:  {aprobadas} / {activas}"));
    for host in &deliberacion.comprobaciones {
        lineas.push(recortar(format!("· {}", limpio(&host.host))));
        for comprobacion in Comprobacion::TODAS {
            let veredicto = host.veredicto(comprobacion);
            let (glifo, dato) = match veredicto {
                Veredicto::NoActiva => ("—", "no activa".to_string()),
                Veredicto::Pendiente => ("◐", "sin resolver".to_string()),
                Veredicto::Aprueba { detalle, .. } => ("✓", limpio(detalle)),
                Veredicto::Rechaza { detalle, .. } => ("✕", limpio(detalle)),
            };
            let nombre = format!("{} {}", comprobacion.nombre_magi(), comprobacion.palabra());
            lineas.push(recortar(format!("    {nombre:<19}{glifo} {dato}")));
        }
    }
    lineas
}

/// Usuario local para `DELIBERACIONES.usuario`: el del uid del proceso (una
/// variable de entorno se puede falsear y esto es un registro de auditoría);
/// `$USER` solo si el sistema no lo sabe.
pub fn usuario_del_sistema() -> String {
    nix::unistd::User::from_uid(nix::unistd::Uid::current())
        .ok()
        .flatten()
        .map(|usuario| usuario.name)
        .unwrap_or_else(crate::conexion::usuario_local)
}

// ---------------------------------------------------------------- orquestación

/// Plazos y mínimos de la deliberación (`[deliberacion]` de `config.toml`),
/// con suelos para que un valor absurdo no desactive nada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limites {
    pub backup_horas: u64,
    pub salud_max: std::time::Duration,
    /// Plazo de cada comprobación, medido desde el inicio.
    pub limite: std::time::Duration,
    pub motivo_min: usize,
}

impl Limites {
    pub fn desde_config(seccion: &crate::config::SeccionDeliberacion) -> Self {
        Self {
            backup_horas: seccion.backup_horas.max(1),
            salud_max: std::time::Duration::from_secs(seccion.salud_max_min.max(1) * 60),
            limite: std::time::Duration::from_secs(seccion.limite_seg.max(1)),
            motivo_min: seccion.motivo_min.max(1),
        }
    }
}

/// Futuro que devuelve una fuente de datos de las comprobaciones.
pub type Futuro<T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>;

/// Sondeo nuevo de un host (MELCHIOR-1).
pub type FuenteSondeo = std::sync::Arc<
    dyn Fn(crate::flota::PeticionSondeo) -> Futuro<crate::modelo::Sondeo> + Send + Sync,
>;

/// Listado del directorio de backups de un host por SFTP (BALTHASAR-2).
pub type FuenteListado = std::sync::Arc<
    dyn Fn(i64, String) -> Futuro<Result<Vec<crate::archivos::Entrada>, String>> + Send + Sync,
>;

/// De dónde salen los datos que no son locales: un sondeo nuevo (MELCHIOR-1)
/// y el listado del directorio de backups por SFTP (BALTHASAR-2). Se
/// inyectan para poder probar la orquestación sin red.
#[derive(Clone)]
pub struct Fuentes {
    pub sondear: FuenteSondeo,
    pub listar: FuenteListado,
}

impl Fuentes {
    /// Las de verdad, a través del servidor de sesiones: el sondeo va por
    /// `Ejecutar` sobre la conexión viva (sin ella, efímero no interactivo,
    /// como el de Flota) y el listado por un SFTP de comprobación
    /// (`AbrirSftp{no_interactivo}`: no dialoga ni levanta túneles
    /// automáticos) seguido de `ListarDir`. El plazo lo pone `lanzar`.
    pub fn reales(cliente: crate::cliente::Cliente) -> Self {
        let para_sondear = cliente.clone();
        let sondear: FuenteSondeo = std::sync::Arc::new(move |peticion| {
            let cliente = para_sondear.clone();
            Box::pin(async move { crate::flota::sondear_via_servidor(&peticion, &cliente).await })
        });
        let listar: FuenteListado = std::sync::Arc::new(move |host_id, ruta| {
            let cliente = cliente.clone();
            Box::pin(async move { listar_por_sftp(&cliente, host_id, ruta).await })
        });
        Self { sondear, listar }
    }
}

/// `AbrirSftp` no interactivo y `ListarDir` de la ruta de los backups.
async fn listar_por_sftp(
    cliente: &crate::cliente::Cliente,
    host_id: i64,
    ruta: String,
) -> Result<Vec<crate::archivos::Entrada>, String> {
    use crate::protocolo::{MensajeCliente, MensajeServidor};
    let caido = |_| "sin servidor de sesiones".to_string();
    let abierto = cliente
        .peticion(|peticion_id| MensajeCliente::AbrirSftp {
            host_id,
            peticion_id: Some(peticion_id),
            no_interactivo: true,
        })
        .await
        .map_err(caido)?;
    match abierto {
        MensajeServidor::SftpAbierto { .. } => {}
        MensajeServidor::Error { mensaje, .. } => return Err(mensaje),
        _ => return Err("respuesta inesperada del servidor".to_string()),
    }
    let listado = cliente
        .peticion(|peticion_id| MensajeCliente::ListarDir {
            host_id,
            ruta,
            peticion_id,
        })
        .await
        .map_err(caido)?;
    match listado {
        MensajeServidor::DirListado { entradas, .. } => Ok(entradas),
        MensajeServidor::Error { mensaje, .. } => Err(mensaje),
        _ => Err("respuesta inesperada del servidor".to_string()),
    }
}

/// Una comprobación que hay que hacer (las que se deciden sin esperar, como
/// un último sondeo reciente, no llegan aquí).
pub enum Trabajo {
    Salud(Box<crate::flota::PeticionSondeo>),
    Backup {
        ruta: String,
        patron: Option<String>,
    },
    Tests {
        comando: String,
        dir: std::path::PathBuf,
    },
}

/// Resultado de una comprobación, para la celda `host × comprobación`.
/// `token` identifica la deliberación: un resultado de otra ya cerrada se
/// descarta.
pub struct ResultadoComprobacion {
    pub token: u64,
    pub host_id: i64,
    pub comprobacion: Comprobacion,
    pub veredicto: Veredicto,
    /// El sondeo nuevo de MELCHIOR-1, para guardarlo como cualquier otro.
    pub sondeo: Option<crate::modelo::Sondeo>,
}

/// Lanza todas las comprobaciones a la vez, cada una con el mismo plazo duro
/// (`limite`, desde el inicio). Siempre llega un resultado por trabajo: el
/// plazo vencido es un rechazo, no una espera (T43). Devuelve con qué
/// abortarlas si la deliberación se cancela.
#[allow(clippy::too_many_arguments)]
pub fn lanzar(
    runtime: &tokio::runtime::Handle,
    token: u64,
    trabajos: Vec<(i64, Comprobacion, Trabajo)>,
    fuentes: Fuentes,
    limites: Limites,
    umbrales: crate::flota::estado::Umbrales,
    limite: tokio::time::Instant,
    enviar: std::sync::Arc<dyn Fn(ResultadoComprobacion) + Send + Sync>,
) -> Vec<tokio::task::AbortHandle> {
    let inicio = std::time::Instant::now();
    trabajos
        .into_iter()
        .map(|(host_id, comprobacion, trabajo)| {
            let fuentes = fuentes.clone();
            let umbrales = umbrales.clone();
            let enviar = enviar.clone();
            runtime
                .spawn(async move {
                    let (veredicto, sondeo) = comprobar(
                        trabajo, host_id, &fuentes, &limites, &umbrales, limite, inicio,
                    )
                    .await;
                    enviar(ResultadoComprobacion {
                        token,
                        host_id,
                        comprobacion,
                        veredicto,
                        sondeo,
                    });
                })
                .abort_handle()
        })
        .collect()
}

async fn comprobar(
    trabajo: Trabajo,
    host_id: i64,
    fuentes: &Fuentes,
    limites: &Limites,
    umbrales: &crate::flota::estado::Umbrales,
    limite: tokio::time::Instant,
    inicio: std::time::Instant,
) -> (Veredicto, Option<crate::modelo::Sondeo>) {
    let milisegundos = || inicio.elapsed().as_millis() as u64;
    let veredicto = |aprueba: bool, detalle: String, ms: u64| {
        if aprueba {
            Veredicto::Aprueba { detalle, ms }
        } else {
            Veredicto::Rechaza { detalle, ms }
        }
    };
    match trabajo {
        Trabajo::Salud(peticion) => {
            match tokio::time::timeout_at(limite, (fuentes.sondear)(*peticion)).await {
                Ok(sondeo) => {
                    let (aprueba, detalle) = salud::con_sondeo_nuevo(&sondeo, umbrales);
                    let ms = milisegundos();
                    let detalle = if aprueba {
                        format!(
                            "{detalle} · {}",
                            crate::snippets::salida::duracion_legible(ms)
                        )
                    } else {
                        detalle
                    };
                    (veredicto(aprueba, detalle, ms), Some(sondeo))
                }
                Err(_) => (
                    veredicto(
                        false,
                        "sin datos recientes (plazo vencido)".to_string(),
                        milisegundos(),
                    ),
                    None,
                ),
            }
        }
        Trabajo::Backup { ruta, patron } => {
            let listado = tokio::time::timeout_at(limite, (fuentes.listar)(host_id, ruta)).await;
            let (aprueba, detalle) = match listado {
                Ok(Ok(entradas)) => backup::evaluar(
                    &entradas,
                    patron.as_deref(),
                    crate::modelo::fecha_ahora_epoca(),
                    limites.backup_horas,
                ),
                Ok(Err(motivo)) => (
                    false,
                    format!(
                        "sin acceso SFTP: {}",
                        crate::snippets::salida::sanear_linea(&motivo, 60)
                    ),
                ),
                Err(_) => (false, "plazo vencido".to_string()),
            };
            (veredicto(aprueba, detalle, milisegundos()), None)
        }
        Trabajo::Tests { comando, dir } => {
            let (aprueba, detalle) = tests::ejecutar(&comando, limite, &dir).await;
            (veredicto(aprueba, detalle, milisegundos()), None)
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_veredicto_viaja_como_json_legible() {
        let json = serde_json::to_string(&Veredicto::Rechaza {
            detalle: "hace 31 h".to_string(),
            ms: 120,
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"estado":"rechaza","detalle":"hace 31 h","ms":120}"#
        );
        let no_activa = serde_json::to_string(&Veredicto::NoActiva).unwrap();
        assert_eq!(no_activa, r#"{"estado":"n/a"}"#);
        let vuelta: Veredicto = serde_json::from_str(&json).unwrap();
        assert!(vuelta.rechaza());
    }

    #[test]
    fn validar_las_verificaciones() {
        assert!(validar_verificaciones(&DatosVerificaciones::default()).is_ok());
        assert!(validar_verificaciones(&DatosVerificaciones {
            backup: true,
            ..Default::default()
        })
        .is_err());
        assert!(validar_verificaciones(&DatosVerificaciones {
            backup: true,
            backup_ruta: Some("/var/backups".to_string()),
            backup_patron: Some("*.sql.gz".to_string()),
            ..Default::default()
        })
        .is_ok());
        assert!(validar_verificaciones(&DatosVerificaciones {
            backup_patron: Some("[".to_string()),
            ..Default::default()
        })
        .is_err());
        assert!(validar_verificaciones(&DatosVerificaciones {
            tests: true,
            ..Default::default()
        })
        .is_err());
    }

    #[test]
    fn el_resultado_de_la_ejecucion_segun_los_hosts() {
        assert_eq!(
            EjecucionResultado::de_hosts(3, 3, false),
            EjecucionResultado::Ok
        );
        assert_eq!(
            EjecucionResultado::de_hosts(0, 3, false),
            EjecucionResultado::Error
        );
        assert_eq!(
            EjecucionResultado::de_hosts(2, 3, false),
            EjecucionResultado::Parcial
        );
        assert_eq!(
            EjecucionResultado::de_hosts(3, 3, true),
            EjecucionResultado::Cancelada
        );
    }

    #[test]
    fn el_detalle_lleva_el_id_y_se_recupera() {
        let deliberacion = RegistroDeliberacion {
            id: 12,
            fecha: String::new(),
            snippet_id: Some(1),
            accion: "reiniciar nginx → hetzner-01".to_string(),
            hosts: vec![(1, "hetzner-01".to_string())],
            comprobaciones: vec![ComprobacionesHost {
                host_id: 1,
                host: "hetzner-01".to_string(),
                salud: Veredicto::Aprueba {
                    detalle: "NOMINAL".to_string(),
                    ms: 1,
                },
                backup: Veredicto::Rechaza {
                    detalle: "hace 31 h".to_string(),
                    ms: 1,
                },
                tests: Veredicto::NoActiva,
            }],
            resultado: ResultadoDeliberacion::Forzada,
            bloqueada: true,
            motivo: Some("revisado a mano".to_string()),
            usuario: "hector".to_string(),
            ejecucion_resultado: None,
        };
        let detalle = detalle_registro(&deliberacion, Some(EjecucionResultado::Ok));
        assert!(detalle.starts_with("deliberación #12 · "));
        assert!(detalle.contains("consenso 1/2"));
        assert!(detalle.contains("motivo «revisado a mano»"));
        assert!(detalle.contains("resultado ok"));
        assert_eq!(id_en_detalle(&detalle), Some(12));
        assert_eq!(id_en_detalle("otra cosa"), None);
    }

    #[test]
    fn el_detalle_para_el_registro_lista_cada_comprobacion() {
        let deliberacion = RegistroDeliberacion {
            id: 4,
            fecha: String::new(),
            snippet_id: None,
            accion: "reiniciar nginx → hetzner-01".to_string(),
            hosts: vec![(1, "hetzner-01".to_string())],
            comprobaciones: vec![ComprobacionesHost {
                host_id: 1,
                host: "hetzner-01".to_string(),
                salud: Veredicto::Aprueba {
                    detalle: "NOMINAL".to_string(),
                    ms: 1,
                },
                backup: Veredicto::Rechaza {
                    detalle: "hace 31 h\u{1b}[2J".to_string(),
                    ms: 1,
                },
                tests: Veredicto::NoActiva,
            }],
            resultado: ResultadoDeliberacion::Forzada,
            bloqueada: true,
            motivo: Some("revisado a mano".to_string()),
            usuario: "hector".to_string(),
            ejecucion_resultado: Some(EjecucionResultado::Parcial),
        };
        let lineas = lineas_detalle(&deliberacion, 70);
        assert_eq!(lineas[0], "DELIBERACIÓN #4 · forzada · hubo rechazos");
        assert!(lineas.contains(&"Motivo:    revisado a mano".to_string()));
        assert!(lineas.contains(&"Ejecución: parcial".to_string()));
        assert!(lineas.contains(&"Consenso:  1 / 2".to_string()));
        assert!(lineas.contains(&"    MELCHIOR-1 salud   ✓ NOMINAL".to_string()));
        assert!(lineas.contains(&"    BALTHASAR-2 backup ✕ hace 31 h".to_string()));
        assert!(lineas.contains(&"    CASPER-3 tests     — no activa".to_string()));
        assert!(lineas.iter().all(|linea| !linea.contains('\u{1b}')));
        assert!(lineas.iter().all(|linea| linea.chars().count() <= 70));
    }

    #[test]
    fn el_usuario_del_sistema_no_esta_vacio() {
        assert!(!usuario_del_sistema().is_empty());
    }

    fn fuentes_de_prueba(
        retraso_sondeo: u64,
        listado: Result<Vec<crate::archivos::Entrada>, String>,
    ) -> Fuentes {
        Fuentes {
            sondear: std::sync::Arc::new(move |peticion: crate::flota::PeticionSondeo| {
                Box::pin(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(retraso_sondeo)).await;
                    crate::modelo::Sondeo {
                        host_id: peticion.host.id,
                        resultado: crate::modelo::ResultadoSondeo::Ok,
                        nucleos: Some(2),
                        carga_1m: Some(0.1),
                        mem_total_kb: Some(100),
                        mem_disponible_kb: Some(90),
                        disco_total_kb: Some(100),
                        disco_usado_kb: Some(10),
                        uptime_seg: Some(10),
                        ..crate::modelo::Sondeo::vacio(peticion.host.id)
                    }
                })
            }),
            listar: std::sync::Arc::new(move |_host, _ruta| {
                let listado = listado.clone();
                Box::pin(async move { listado })
            }),
        }
    }

    fn peticion_sondeo() -> crate::flota::PeticionSondeo {
        crate::flota::PeticionSondeo {
            host: crate::modelo::host_de_prueba(),
            todos_los_hosts: std::collections::HashMap::new(),
            known_hosts: std::path::PathBuf::new(),
            dir_ssh: std::path::PathBuf::new(),
            hogar: std::path::PathBuf::new(),
            usuario_local: String::new(),
            servicios: Vec::new(),
        }
    }

    async fn deliberar(
        trabajos: Vec<(i64, Comprobacion, Trabajo)>,
        fuentes: Fuentes,
    ) -> Vec<ResultadoComprobacion> {
        let esperados = trabajos.len();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let limites = Limites::desde_config(&crate::config::SeccionDeliberacion::default());
        lanzar(
            &tokio::runtime::Handle::current(),
            7,
            trabajos,
            fuentes,
            limites,
            crate::flota::estado::Umbrales::default(),
            tokio::time::Instant::now() + limites.limite,
            std::sync::Arc::new(move |resultado| {
                let _ = tx.send(resultado);
            }),
        );
        let mut resultados = Vec::new();
        while resultados.len() < esperados {
            resultados.push(rx.recv().await.expect("un resultado por trabajo"));
        }
        resultados
    }

    #[tokio::test]
    async fn todas_en_paralelo_con_el_mismo_plazo() {
        let dir = std::env::temp_dir();
        let inicio = std::time::Instant::now();
        let resultados = deliberar(
            vec![
                (
                    1,
                    Comprobacion::Salud,
                    Trabajo::Salud(Box::new(peticion_sondeo())),
                ),
                (
                    1,
                    Comprobacion::Backup,
                    Trabajo::Backup {
                        ruta: "/b".to_string(),
                        patron: None,
                    },
                ),
                (
                    2,
                    Comprobacion::Tests,
                    Trabajo::Tests {
                        comando: "sleep 3".to_string(),
                        dir: dir.clone(),
                    },
                ),
                (
                    2,
                    Comprobacion::Tests,
                    Trabajo::Tests {
                        comando: "true".to_string(),
                        dir,
                    },
                ),
            ],
            fuentes_de_prueba(10, Err("permiso denegado".to_string())),
        )
        .await;
        // El más lento (sleep 3) cae a los 2 s: todas en paralelo.
        assert!(inicio.elapsed() < std::time::Duration::from_millis(2900));
        let de = |host: i64, comprobacion: Comprobacion| {
            resultados
                .iter()
                .filter(|r| r.host_id == host && r.comprobacion == comprobacion)
                .map(|r| r.veredicto.clone())
                .collect::<Vec<_>>()
        };
        assert!(de(1, Comprobacion::Salud)[0].aprueba());
        assert!(resultados
            .iter()
            .any(|r| r.comprobacion == Comprobacion::Salud && r.sondeo.is_some()));
        assert_eq!(
            de(1, Comprobacion::Backup)[0].detalle(),
            Some("sin acceso SFTP: permiso denegado")
        );
        let tests = de(2, Comprobacion::Tests);
        assert!(tests.iter().any(|v| v.detalle() == Some("plazo vencido")));
        assert!(tests.iter().any(Veredicto::aprueba));
        assert!(resultados.iter().all(|r| r.token == 7));
    }

    #[tokio::test]
    async fn un_sondeo_lento_rechaza_sin_datos() {
        let resultados = deliberar(
            vec![(
                1,
                Comprobacion::Salud,
                Trabajo::Salud(Box::new(peticion_sondeo())),
            )],
            fuentes_de_prueba(5000, Ok(Vec::new())),
        )
        .await;
        assert_eq!(
            resultados[0].veredicto.detalle(),
            Some("sin datos recientes (plazo vencido)")
        );
        assert!(resultados[0].sondeo.is_none());
    }
}
