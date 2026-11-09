//! Orquestación de las comprobaciones de la deliberación (F6 §7.3): todas en
//! paralelo, cada una con el mismo plazo duro de 2 s (T43) y siempre con un
//! veredicto (el plazo vencido rechaza, no deja la celda colgada).
//!
//! Lo que no es local se pide a los proveedores que pone cada plataforma: el
//! sondeo de MELCHIOR-1, el listado por SFTP de BALTHASAR-2 y el comando
//! local de CASPER-3. En el móvil, CASPER-3 no está disponible y rechaza
//! («solo en el escritorio»): una comprobación activa nunca se omite (T116).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use crate::deliberacion::{Comprobacion, Veredicto};
use crate::flota::estado::Umbrales;
use crate::modelo::Sondeo;
use crate::sftp_lectura::EntradaListado;

use super::{backup, salud};

/// Futuro que devuelve una fuente de datos de las comprobaciones.
pub type Futuro<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// Una comprobación que la plataforma no puede hacer aquí. Rechaza, con el
/// motivo que se enseña en la celda.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoDisponible {
    pub motivo: String,
}

impl NoDisponible {
    pub fn nuevo(motivo: impl Into<String>) -> Self {
        Self {
            motivo: motivo.into(),
        }
    }
}

/// De dónde salen los datos que no son locales. Cada plataforma implementa
/// este trait con lo suyo: el escritorio, el servidor de sesiones y sh -c en
/// local; el móvil, su conexión, sftp_lectura y «no disponible».
pub trait Proveedores: Send + Sync {
    /// Un sondeo nuevo del host (MELCHIOR-1), sin contar las unidades de
    /// excluir (el propio objetivo de una acción de servicio).
    fn sondear(&self, host_id: i64, excluir: Vec<String>) -> Futuro<Sondeo>;

    /// El listado del directorio de backups del host (BALTHASAR-2).
    fn listar(&self, host_id: i64, ruta: String) -> Futuro<Result<Vec<EntradaListado>, String>>;

    /// El comando local de tests (CASPER-3): Ok((aprueba, detalle)) o
    /// NoDisponible si la plataforma no puede hacerlo.
    fn tests(&self, host_id: i64, comando: String) -> Futuro<Result<(bool, String), NoDisponible>>;
}

/// Plazos y mínimos de la deliberación, con suelos para que un valor absurdo
/// no desactive nada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limites {
    pub backup_horas: u64,
    pub salud_max: Duration,
    /// Plazo de cada comprobación, medido desde el inicio.
    pub limite: Duration,
    pub motivo_min: usize,
}

impl Limites {
    pub fn nuevos(
        backup_horas: u64,
        salud_max_min: u64,
        limite_seg: u64,
        motivo_min: usize,
    ) -> Self {
        Self {
            backup_horas: backup_horas.max(1),
            salud_max: Duration::from_secs(salud_max_min.max(1) * 60),
            limite: Duration::from_secs(limite_seg.max(1)),
            motivo_min: motivo_min.max(1),
        }
    }
}

impl Default for Limites {
    fn default() -> Self {
        Self::nuevos(24, 5, 2, 10)
    }
}

/// Una comprobación que hay que hacer (las que se deciden sin esperar, como
/// un último sondeo reciente, no llegan aquí).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trabajo {
    Salud {
        /// Unidades que MELCHIOR-1 no cuenta (el objetivo de la acción).
        excluir: Vec<String>,
    },
    Backup {
        ruta: String,
        patron: Option<String>,
    },
    Tests {
        comando: String,
    },
}

/// Resultado de una comprobación, para la celda host × comprobación.
/// token identifica la deliberación: un resultado de otra ya cerrada se
/// descarta.
pub struct ResultadoComprobacion {
    pub token: u64,
    pub host_id: i64,
    pub comprobacion: Comprobacion,
    pub veredicto: Veredicto,
    /// El sondeo nuevo de MELCHIOR-1, para guardarlo como cualquier otro.
    pub sondeo: Option<Sondeo>,
}

/// Lanza todas las comprobaciones a la vez, cada una con el mismo plazo duro
/// (limite, desde el inicio). Siempre llega un resultado por trabajo: el plazo
/// vencido es un rechazo, no una espera (T43). Devuelve con qué abortarlas si
/// la deliberación se cancela.
#[allow(clippy::too_many_arguments)]
pub fn lanzar(
    runtime: &tokio::runtime::Handle,
    token: u64,
    trabajos: Vec<(i64, Comprobacion, Trabajo)>,
    proveedores: Arc<dyn Proveedores>,
    limites: Limites,
    umbrales: Umbrales,
    limite: tokio::time::Instant,
    enviar: Arc<dyn Fn(ResultadoComprobacion) + Send + Sync>,
) -> Vec<tokio::task::AbortHandle> {
    let inicio = std::time::Instant::now();
    trabajos
        .into_iter()
        .map(|(host_id, comprobacion, trabajo)| {
            let proveedores = proveedores.clone();
            let umbrales = umbrales.clone();
            let enviar = enviar.clone();
            runtime
                .spawn(async move {
                    let (veredicto, sondeo) = comprobar(
                        trabajo,
                        host_id,
                        proveedores.as_ref(),
                        &limites,
                        &umbrales,
                        limite,
                        inicio,
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
    proveedores: &dyn Proveedores,
    limites: &Limites,
    umbrales: &Umbrales,
    limite: tokio::time::Instant,
    inicio: std::time::Instant,
) -> (Veredicto, Option<Sondeo>) {
    let milisegundos = || inicio.elapsed().as_millis() as u64;
    let veredicto = |aprueba: bool, detalle: String, ms: u64| {
        if aprueba {
            Veredicto::Aprueba { detalle, ms }
        } else {
            Veredicto::Rechaza { detalle, ms }
        }
    };
    match trabajo {
        Trabajo::Salud { excluir } => {
            match tokio::time::timeout_at(limite, proveedores.sondear(host_id, excluir)).await {
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
            let listado = tokio::time::timeout_at(limite, proveedores.listar(host_id, ruta)).await;
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
        Trabajo::Tests { comando } => {
            let resultado =
                tokio::time::timeout_at(limite, proveedores.tests(host_id, comando)).await;
            let (aprueba, detalle) = match resultado {
                Ok(Ok((aprueba, detalle))) => (aprueba, detalle),
                Ok(Err(no_disponible)) => (
                    false,
                    crate::snippets::salida::sanear_linea(&no_disponible.motivo, 60),
                ),
                Err(_) => (false, "plazo vencido".to_string()),
            };
            (veredicto(aprueba, detalle, milisegundos()), None)
        }
    }
}

/// El texto de un veredicto con el dato saneado (texto de un remoto o de un
/// comando local), como lo enseña la tabla.
pub fn saneado(veredicto: Veredicto, largo: usize) -> Veredicto {
    let limpiar = |detalle: String| crate::snippets::salida::sanear_linea(&detalle, largo);
    match veredicto {
        Veredicto::Aprueba { detalle, ms } => Veredicto::Aprueba {
            detalle: limpiar(detalle),
            ms,
        },
        Veredicto::Rechaza { detalle, ms } => Veredicto::Rechaza {
            detalle: limpiar(detalle),
            ms,
        },
        otro => otro,
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use std::sync::Mutex;

    struct ProveedoresDePrueba {
        retraso_sondeo: u64,
        listado: Result<Vec<EntradaListado>, String>,
        tests: Result<(bool, String), NoDisponible>,
        llamadas: Mutex<Vec<String>>,
    }

    impl Proveedores for ProveedoresDePrueba {
        fn sondear(&self, host_id: i64, excluir: Vec<String>) -> Futuro<Sondeo> {
            self.llamadas
                .lock()
                .unwrap()
                .push(format!("sondear {host_id} {excluir:?}"));
            let retraso = self.retraso_sondeo;
            Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(retraso)).await;
                Sondeo {
                    host_id,
                    resultado: crate::modelo::ResultadoSondeo::Ok,
                    nucleos: Some(2),
                    carga_1m: Some(0.1),
                    mem_total_kb: Some(100),
                    mem_disponible_kb: Some(90),
                    disco_total_kb: Some(100),
                    disco_usado_kb: Some(10),
                    uptime_seg: Some(10),
                    ..Sondeo::vacio(host_id)
                }
            })
        }

        fn listar(
            &self,
            _host_id: i64,
            _ruta: String,
        ) -> Futuro<Result<Vec<EntradaListado>, String>> {
            let listado = self.listado.clone();
            Box::pin(async move { listado })
        }

        fn tests(
            &self,
            _host_id: i64,
            _comando: String,
        ) -> Futuro<Result<(bool, String), NoDisponible>> {
            let tests = self.tests.clone();
            Box::pin(async move { tests })
        }
    }

    fn proveedores(
        retraso_sondeo: u64,
        listado: Result<Vec<EntradaListado>, String>,
        tests: Result<(bool, String), NoDisponible>,
    ) -> Arc<ProveedoresDePrueba> {
        Arc::new(ProveedoresDePrueba {
            retraso_sondeo,
            listado,
            tests,
            llamadas: Mutex::new(Vec::new()),
        })
    }

    async fn deliberar(
        trabajos: Vec<(i64, Comprobacion, Trabajo)>,
        proveedores: Arc<dyn Proveedores>,
    ) -> Vec<ResultadoComprobacion> {
        let esperados = trabajos.len();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let limites = Limites::default();
        lanzar(
            &tokio::runtime::Handle::current(),
            7,
            trabajos,
            proveedores,
            limites,
            Umbrales::default(),
            tokio::time::Instant::now() + limites.limite,
            Arc::new(move |resultado| {
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
        let inicio = std::time::Instant::now();
        let resultados = deliberar(
            vec![
                (
                    1,
                    Comprobacion::Salud,
                    Trabajo::Salud {
                        excluir: Vec::new(),
                    },
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
                    },
                ),
            ],
            proveedores(
                10,
                Err("permiso denegado".to_string()),
                Ok((false, "plazo vencido".to_string())),
            ),
        )
        .await;
        // Todas en paralelo: el sondeo tarda 10 ms, no se espera en serie.
        assert!(inicio.elapsed() < Duration::from_millis(1500));
        assert!(resultados.iter().all(|r| r.token == 7));
        let salud = resultados
            .iter()
            .find(|r| r.comprobacion == Comprobacion::Salud)
            .unwrap();
        assert!(salud.veredicto.aprueba());
        assert!(salud.sondeo.is_some());
        let backup = resultados
            .iter()
            .find(|r| r.comprobacion == Comprobacion::Backup)
            .unwrap();
        assert_eq!(
            backup.veredicto.detalle(),
            Some("sin acceso SFTP: permiso denegado")
        );
        let tests = resultados
            .iter()
            .find(|r| r.comprobacion == Comprobacion::Tests)
            .unwrap();
        assert_eq!(tests.veredicto.detalle(), Some("plazo vencido"));
    }

    #[tokio::test]
    async fn un_sondeo_lento_rechaza_sin_datos() {
        let resultados = deliberar(
            vec![(
                1,
                Comprobacion::Salud,
                Trabajo::Salud {
                    excluir: Vec::new(),
                },
            )],
            proveedores(5000, Ok(Vec::new()), Ok((true, "verde".to_string()))),
        )
        .await;
        assert_eq!(
            resultados[0].veredicto.detalle(),
            Some("sin datos recientes (plazo vencido)")
        );
        assert!(resultados[0].sondeo.is_none());
    }

    #[tokio::test]
    async fn una_comprobacion_no_disponible_rechaza_con_su_motivo() {
        let resultados = deliberar(
            vec![(
                3,
                Comprobacion::Tests,
                Trabajo::Tests {
                    comando: "gh run list".to_string(),
                },
            )],
            proveedores(
                0,
                Ok(Vec::new()),
                Err(NoDisponible::nuevo("solo en el escritorio")),
            ),
        )
        .await;
        assert_eq!(
            resultados[0].veredicto.detalle(),
            Some("solo en el escritorio")
        );
        assert!(resultados[0].veredicto.rechaza());
    }

    #[test]
    fn los_limites_tienen_suelos() {
        let limites = Limites::nuevos(0, 0, 0, 0);
        assert_eq!(limites.backup_horas, 1);
        assert_eq!(limites.salud_max, Duration::from_secs(60));
        assert_eq!(limites.limite, Duration::from_secs(1));
        assert_eq!(limites.motivo_min, 1);
        assert_eq!(Limites::default().limite, Duration::from_secs(2));
    }
}
