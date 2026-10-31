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

pub use magi_nucleo::deliberacion::*;

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
            usuario: "usuario".to_string(),
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
            usuario: "usuario".to_string(),
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
